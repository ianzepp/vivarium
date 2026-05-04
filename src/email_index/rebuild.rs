use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};

use super::links::{MessageLink, links_from_raw};
use super::{EmailIndex, IndexStats};
use crate::catalog::CatalogEntry;
use crate::error::VivariumError;
use crate::message::normalize_message_id;
use crate::storage::Storage;

pub(crate) fn rebuild(mail_root: &Path, account: &str) -> Result<IndexStats, VivariumError> {
    let mut index = EmailIndex::open(mail_root)?;
    let entries = Storage::open(mail_root)?.list_catalog_entries(account)?;
    let now = Utc::now().to_rfc3339();
    let mut seen = BTreeSet::new();
    let mut stats = IndexStats::default();
    let tx = begin(&mut index)?;

    for entry in entries {
        stats.scanned += 1;
        if stats.scanned == 1 || stats.scanned % 1000 == 0 {
            eprintln!(
                "indexing {account}: scanned={} updated={} reused={} errors={}",
                stats.scanned, stats.updated, stats.reused, stats.errors
            );
        }
        index_entry(&tx, account, &entry, &now, &mut seen, &mut stats)?;
    }
    prune_stale(&tx, account, &seen, &mut stats)?;
    tx.commit()
        .map_err(|e| VivariumError::Other(format!("failed to commit index transaction: {e}")))?;
    Ok(stats)
}

fn begin(index: &mut EmailIndex) -> Result<Transaction<'_>, VivariumError> {
    index
        .conn
        .transaction()
        .map_err(|e| VivariumError::Other(format!("failed to start index transaction: {e}")))
}

fn index_entry(
    tx: &Transaction<'_>,
    account: &str,
    entry: &CatalogEntry,
    now: &str,
    seen: &mut BTreeSet<String>,
    stats: &mut IndexStats,
) -> Result<(), VivariumError> {
    let message_id = entry.handle.clone();
    seen.insert(message_id.clone());
    let blob_path = entry.raw_path.clone();
    update_reuse_stats(tx, account, &message_id, entry, &blob_path, stats)?;
    let data = match fs::read(&blob_path) {
        Ok(data) => data,
        Err(_) => {
            stats.errors += 1;
            return Ok(());
        }
    };
    let links = links_from_raw(&data);
    upsert_message(tx, account, &message_id, entry, &blob_path, &links, now)?;
    replace_links(tx, account, &message_id, &links)
}

fn update_reuse_stats(
    tx: &Transaction<'_>,
    account: &str,
    message_id: &str,
    entry: &CatalogEntry,
    blob_path: &str,
    stats: &mut IndexStats,
) -> Result<(), VivariumError> {
    if unchanged_existing_row(tx, account, message_id, entry, blob_path)? {
        stats.reused += 1;
    } else {
        stats.updated += 1;
    }
    Ok(())
}

fn unchanged_existing_row(
    tx: &Transaction<'_>,
    account: &str,
    message_id: &str,
    entry: &CatalogEntry,
    blob_path: &str,
) -> Result<bool, VivariumError> {
    let existing = tx
        .query_row(
            "SELECT content_id, blob_path, local_role, date,
                    from_addr, to_addr, cc_addr, bcc_addr, subject, rfc_message_id
             FROM messages
             WHERE account = ?1 AND message_id = ?2",
            params![account, message_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()
        .map_err(|e| VivariumError::Other(format!("failed to read index row: {e}")))?;
    Ok(existing.as_ref().is_some_and(
        |(
            content_id,
            existing_blob_path,
            local_role,
            date,
            from_addr,
            to_addr,
            cc_addr,
            bcc_addr,
            subject,
            rfc_message_id,
        )| {
            content_id == &entry.fingerprint
                && existing_blob_path == blob_path
                && local_role == &local_role_from_folder(&entry.folder)
                && date == &entry.date
                && from_addr == &entry.from
                && to_addr == &entry.to
                && cc_addr == &entry.cc
                && bcc_addr == &entry.bcc
                && subject == &entry.subject
                && *rfc_message_id == normalized_rfc_message_id(entry)
        },
    ))
}

fn upsert_message(
    tx: &Transaction<'_>,
    account: &str,
    message_id: &str,
    entry: &CatalogEntry,
    blob_path: &str,
    links: &[MessageLink],
    now: &str,
) -> Result<(), VivariumError> {
    let remote_mailbox = entry
        .remote
        .as_ref()
        .map(|remote| remote.remote_mailbox.as_str());
    let remote_uid = entry.remote.as_ref().map(|remote| i64::from(remote.uid));
    let remote_uidvalidity = entry
        .remote
        .as_ref()
        .map(|remote| i64::from(remote.uidvalidity));
    let rfc_message_id = normalized_rfc_message_id(entry).or_else(|| {
        links
            .iter()
            .find(|link| link.kind == "message_id")
            .map(|link| link.rfc_message_id.clone())
    });
    tx.execute(
        "INSERT INTO messages (
            account, message_id, content_id, blob_path, local_role,
            date, from_addr, to_addr, cc_addr, bcc_addr, subject, rfc_message_id,
            remote_mailbox, remote_uid, remote_uidvalidity, indexed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(account, message_id) DO UPDATE SET
            content_id = excluded.content_id,
            blob_path = excluded.blob_path,
            local_role = excluded.local_role,
            date = excluded.date,
            from_addr = excluded.from_addr,
            to_addr = excluded.to_addr,
            cc_addr = excluded.cc_addr,
            bcc_addr = excluded.bcc_addr,
            subject = excluded.subject,
            rfc_message_id = excluded.rfc_message_id,
            remote_mailbox = excluded.remote_mailbox,
            remote_uid = excluded.remote_uid,
            remote_uidvalidity = excluded.remote_uidvalidity,
            indexed_at = excluded.indexed_at",
        params![
            account,
            message_id,
            entry.fingerprint,
            blob_path,
            local_role_from_folder(&entry.folder),
            entry.date,
            entry.from,
            entry.to,
            entry.cc,
            entry.bcc,
            entry.subject,
            rfc_message_id,
            remote_mailbox,
            remote_uid,
            remote_uidvalidity,
            now,
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to upsert index row: {e}")))?;
    Ok(())
}

fn replace_links(
    tx: &Transaction<'_>,
    account: &str,
    message_id: &str,
    links: &[MessageLink],
) -> Result<(), VivariumError> {
    tx.execute(
        "DELETE FROM message_links WHERE account = ?1 AND message_id = ?2",
        params![account, message_id],
    )
    .map_err(|e| VivariumError::Other(format!("failed to clear index links: {e}")))?;
    for link in links {
        tx.execute(
            "INSERT OR IGNORE INTO message_links (account, message_id, link_kind, normalized_message_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![account, message_id, link.kind, link.rfc_message_id],
        )
        .map_err(|e| VivariumError::Other(format!("failed to upsert index link: {e}")))?;
    }
    Ok(())
}

fn prune_stale(
    tx: &Transaction<'_>,
    account: &str,
    seen: &BTreeSet<String>,
    stats: &mut IndexStats,
) -> Result<(), VivariumError> {
    for message_id in existing_message_ids(tx, account)? {
        if seen.contains(&message_id) {
            continue;
        }
        tx.execute(
            "DELETE FROM messages WHERE account = ?1 AND message_id = ?2",
            params![account, message_id],
        )
        .map_err(|e| VivariumError::Other(format!("failed to remove stale index row: {e}")))?;
        stats.stale += 1;
    }
    Ok(())
}

fn existing_message_ids(tx: &Transaction<'_>, account: &str) -> Result<Vec<String>, VivariumError> {
    tx.prepare("SELECT message_id FROM messages WHERE account = ?1")
        .and_then(|mut stmt| {
            stmt.query_map(params![account], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| VivariumError::Other(format!("failed to load stale index rows: {e}")))
}

fn normalized_rfc_message_id(entry: &CatalogEntry) -> Option<String> {
    normalize_optional(&entry.rfc_message_id)
}

fn normalize_optional(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        normalize_message_id(value)
    }
}

fn local_role_from_folder(folder: &str) -> String {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" => "inbox".into(),
        "archive" | "all" => "archive".into(),
        "trash" => "trash".into(),
        "sent" => "sent".into(),
        "drafts" | "draft" => "drafts".into(),
        other => other.to_string(),
    }
}
