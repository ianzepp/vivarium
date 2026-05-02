use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};

use super::links::{MessageLink, links_from_raw};
use super::{EmailIndex, IndexStats};
use crate::catalog::{Catalog, CatalogEntry};
use crate::error::VivariumError;
use crate::message::normalize_message_id;
use crate::store::message_id_from_path;

pub(crate) fn rebuild(mail_root: &Path, account: &str) -> Result<IndexStats, VivariumError> {
    let mut index = EmailIndex::open(mail_root)?;
    let catalog = Catalog::open(mail_root)?;
    let entries = catalog.list_messages(account)?;
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
    let Some(handle) = message_id_from_path(Path::new(&entry.raw_path)) else {
        stats.errors += 1;
        return Ok(());
    };
    seen.insert(handle.clone());
    update_reuse_stats(tx, account, &handle, entry, stats)?;
    let data = match fs::read(&entry.raw_path) {
        Ok(data) => data,
        Err(_) => {
            stats.errors += 1;
            return Ok(());
        }
    };
    let links = links_from_raw(&data);
    upsert_message(tx, account, &handle, entry, &links, now)?;
    replace_links(tx, account, &handle, &links)
}

fn update_reuse_stats(
    tx: &Transaction<'_>,
    account: &str,
    handle: &str,
    entry: &CatalogEntry,
    stats: &mut IndexStats,
) -> Result<(), VivariumError> {
    if unchanged_existing_row(tx, account, handle, entry)? {
        stats.reused += 1;
    } else {
        stats.updated += 1;
    }
    Ok(())
}

fn unchanged_existing_row(
    tx: &Transaction<'_>,
    account: &str,
    handle: &str,
    entry: &CatalogEntry,
) -> Result<bool, VivariumError> {
    let existing = tx
        .query_row(
            "SELECT fingerprint, raw_path, folder, maildir_subdir
             FROM messages WHERE account = ?1 AND handle = ?2",
            params![account, handle],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|e| VivariumError::Other(format!("failed to read index row: {e}")))?;
    Ok(existing
        .as_ref()
        .is_some_and(|(fingerprint, raw_path, folder, subdir)| {
            fingerprint == &entry.fingerprint
                && raw_path == &entry.raw_path
                && folder == &entry.folder
                && subdir == &entry.maildir_subdir
        }))
}

fn upsert_message(
    tx: &Transaction<'_>,
    account: &str,
    handle: &str,
    entry: &CatalogEntry,
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
    let rfc_message_id = normalize_optional(&entry.rfc_message_id).or_else(|| {
        links
            .iter()
            .find(|link| link.kind == "message_id")
            .map(|link| link.rfc_message_id.clone())
    });
    execute_upsert(
        tx,
        UpsertMessage {
            account,
            handle,
            entry,
            rfc_message_id,
            remote_mailbox,
            remote_uid,
            remote_uidvalidity,
            now,
        },
    )
}

struct UpsertMessage<'a> {
    account: &'a str,
    handle: &'a str,
    entry: &'a CatalogEntry,
    rfc_message_id: Option<String>,
    remote_mailbox: Option<&'a str>,
    remote_uid: Option<i64>,
    remote_uidvalidity: Option<i64>,
    now: &'a str,
}

fn execute_upsert(tx: &Transaction<'_>, message: UpsertMessage<'_>) -> Result<(), VivariumError> {
    tx.execute(
        "INSERT INTO messages (
            account, handle, catalog_handle, fingerprint, raw_path, folder, maildir_subdir,
            date, from_addr, to_addr, cc_addr, bcc_addr, subject, rfc_message_id,
            remote_mailbox, remote_uid, remote_uidvalidity, indexed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        ON CONFLICT(account, handle) DO UPDATE SET
            catalog_handle = excluded.catalog_handle,
            fingerprint = excluded.fingerprint,
            raw_path = excluded.raw_path,
            folder = excluded.folder,
            maildir_subdir = excluded.maildir_subdir,
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
            message.account,
            message.handle,
            message.entry.handle,
            message.entry.fingerprint,
            message.entry.raw_path,
            message.entry.folder,
            message.entry.maildir_subdir,
            message.entry.date,
            message.entry.from,
            message.entry.to,
            message.entry.cc,
            message.entry.bcc,
            message.entry.subject,
            message.rfc_message_id,
            message.remote_mailbox,
            message.remote_uid,
            message.remote_uidvalidity,
            message.now,
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to upsert index row: {e}")))?;
    Ok(())
}

fn replace_links(
    tx: &Transaction<'_>,
    account: &str,
    handle: &str,
    links: &[MessageLink],
) -> Result<(), VivariumError> {
    tx.execute(
        "DELETE FROM message_links WHERE account = ?1 AND handle = ?2",
        params![account, handle],
    )
    .map_err(|e| VivariumError::Other(format!("failed to clear index links: {e}")))?;
    for link in links {
        tx.execute(
            "INSERT OR IGNORE INTO message_links (account, handle, link_kind, rfc_message_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![account, handle, link.kind, link.rfc_message_id],
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
    for handle in existing_handles(tx, account)? {
        if seen.contains(&handle) {
            continue;
        }
        tx.execute(
            "DELETE FROM messages WHERE account = ?1 AND handle = ?2",
            params![account, handle],
        )
        .map_err(|e| VivariumError::Other(format!("failed to remove stale index row: {e}")))?;
        stats.stale += 1;
    }
    Ok(())
}

fn existing_handles(tx: &Transaction<'_>, account: &str) -> Result<Vec<String>, VivariumError> {
    tx.prepare("SELECT handle FROM messages WHERE account = ?1")
        .and_then(|mut stmt| {
            stmt.query_map(params![account], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| VivariumError::Other(format!("failed to load stale index rows: {e}")))
}

fn normalize_optional(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        normalize_message_id(value)
    }
}
