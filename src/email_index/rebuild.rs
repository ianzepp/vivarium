use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};

use super::links::{MessageLink, links_from_raw};
use super::{EmailIndex, IndexStats};
use crate::catalog::CatalogEntry;
use crate::error::VivariumError;
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
    let blob_path = entry.blob_path.clone();
    update_reuse_stats(tx, account, &message_id, entry, &blob_path, stats)?;
    let Ok(data) = fs::read(&blob_path) else {
        stats.errors += 1;
        return Ok(());
    };
    let links = links_from_raw(&data);
    upsert_message(tx, account, &message_id, entry, now)?;
    replace_search_doc(tx, account, &message_id, entry)?;
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
    _blob_path: &str,
) -> Result<bool, VivariumError> {
    let existing = tx
        .query_row(
            "SELECT content_id
             FROM indexed_messages
             WHERE account = ?1 AND message_id = ?2",
            params![account, message_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| VivariumError::Other(format!("failed to read index row: {e}")))?;
    Ok(existing.as_deref() == Some(entry.content_id.as_str()))
}

fn upsert_message(
    tx: &Transaction<'_>,
    account: &str,
    message_id: &str,
    entry: &CatalogEntry,
    now: &str,
) -> Result<(), VivariumError> {
    tx.execute(
        "INSERT INTO indexed_messages (
            account, message_id, content_id, indexed_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(account, message_id) DO UPDATE SET
            content_id = excluded.content_id,
            indexed_at = excluded.indexed_at",
        params![account, message_id, entry.content_id, now,],
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

fn replace_search_doc(
    tx: &Transaction<'_>,
    account: &str,
    message_id: &str,
    entry: &CatalogEntry,
) -> Result<(), VivariumError> {
    tx.execute(
        "DELETE FROM message_search_fts WHERE account = ?1 AND message_id = ?2",
        params![account, message_id],
    )
    .map_err(|e| VivariumError::Other(format!("failed to clear search document: {e}")))?;
    tx.execute(
        "INSERT INTO message_search_fts (
           account, message_id, local_role, date, from_addr, to_addr, cc_addr,
           bcc_addr, subject, rfc_message_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            account,
            message_id,
            entry.local_role,
            entry.date,
            entry.from,
            entry.to,
            entry.cc,
            entry.bcc,
            entry.subject,
            entry.rfc_message_id,
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to upsert search document: {e}")))?;
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
            "DELETE FROM indexed_messages WHERE account = ?1 AND message_id = ?2",
            params![account, message_id],
        )
        .map_err(|e| VivariumError::Other(format!("failed to remove stale index row: {e}")))?;
        tx.execute(
            "DELETE FROM message_search_fts WHERE account = ?1 AND message_id = ?2",
            params![account, message_id],
        )
        .map_err(|e| VivariumError::Other(format!("failed to remove stale search row: {e}")))?;
        stats.stale += 1;
    }
    Ok(())
}

fn existing_message_ids(tx: &Transaction<'_>, account: &str) -> Result<Vec<String>, VivariumError> {
    tx.prepare("SELECT message_id FROM indexed_messages WHERE account = ?1")
        .and_then(|mut stmt| {
            stmt.query_map(params![account], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| VivariumError::Other(format!("failed to load stale index rows: {e}")))
}
