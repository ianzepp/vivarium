use std::fs;
use std::path::Path;

use rusqlite::{Connection, params};

use super::CatalogEntry;
use crate::error::VivariumError;

pub(super) fn ensure_schema(conn: &Connection) -> Result<(), VivariumError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS catalog_entries (
          account TEXT NOT NULL,
          handle TEXT NOT NULL,
          raw_path TEXT NOT NULL,
          fingerprint TEXT NOT NULL,
          folder TEXT NOT NULL,
          maildir_subdir TEXT NOT NULL,
          date TEXT NOT NULL,
          from_addr TEXT NOT NULL,
          to_addr TEXT NOT NULL,
          cc_addr TEXT NOT NULL,
          bcc_addr TEXT NOT NULL,
          subject TEXT NOT NULL,
          rfc_message_id TEXT NOT NULL,
          remote_json TEXT,
          is_duplicate INTEGER NOT NULL,
          PRIMARY KEY (account, handle)
        );

        CREATE INDEX IF NOT EXISTS catalog_entries_account_date_idx
          ON catalog_entries(account, date DESC);
        CREATE INDEX IF NOT EXISTS catalog_entries_raw_path_idx
          ON catalog_entries(raw_path);
        CREATE INDEX IF NOT EXISTS catalog_entries_rfc_message_id_idx
          ON catalog_entries(account, folder, rfc_message_id);
        ",
    )
    .map_err(|e| VivariumError::Other(format!("failed to initialize catalog schema: {e}")))
}

pub(super) fn import_legacy_json_if_needed(
    conn: &Connection,
    legacy_path: &Path,
) -> Result<(), VivariumError> {
    if !legacy_path.exists() || count_all_entries(conn)? > 0 {
        return Ok(());
    }

    let data = fs::read_to_string(legacy_path)
        .map_err(|e| VivariumError::Other(format!("failed to read legacy catalog: {e}")))?;
    let entries = serde_json::from_str::<Vec<CatalogEntry>>(&data)
        .map_err(|e| VivariumError::Other(format!("failed to parse legacy catalog: {e}")))?;
    for entry in entries {
        upsert_entry(conn, &entry)?;
    }
    Ok(())
}

pub(super) fn upsert_entry(conn: &Connection, entry: &CatalogEntry) -> Result<(), VivariumError> {
    let remote_json = entry
        .remote
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| VivariumError::Other(format!("failed to serialize remote identity: {e}")))?;
    conn.execute(
        "INSERT INTO catalog_entries (
           account, handle, raw_path, fingerprint, folder, maildir_subdir, date,
           from_addr, to_addr, cc_addr, bcc_addr, subject, rfc_message_id,
           remote_json, is_duplicate
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
         )
         ON CONFLICT(account, handle) DO UPDATE SET
           raw_path = excluded.raw_path,
           fingerprint = excluded.fingerprint,
           folder = excluded.folder,
           maildir_subdir = excluded.maildir_subdir,
           date = excluded.date,
           from_addr = excluded.from_addr,
           to_addr = excluded.to_addr,
           cc_addr = excluded.cc_addr,
           bcc_addr = excluded.bcc_addr,
           subject = excluded.subject,
           rfc_message_id = excluded.rfc_message_id,
           remote_json = excluded.remote_json,
           is_duplicate = excluded.is_duplicate",
        params![
            entry.account,
            entry.handle,
            entry.raw_path,
            entry.fingerprint,
            entry.folder,
            entry.maildir_subdir,
            entry.date,
            entry.from,
            entry.to,
            entry.cc,
            entry.bcc,
            entry.subject,
            entry.rfc_message_id,
            remote_json,
            entry.is_duplicate as i64,
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to upsert catalog row: {e}")))?;
    Ok(())
}

pub(super) fn catalog_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogEntry> {
    let remote_json: Option<String> = row.get(13)?;
    let remote = remote_json
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok());
    let is_duplicate: i64 = row.get(14)?;
    Ok(CatalogEntry {
        handle: row.get(0)?,
        raw_path: row.get(1)?,
        fingerprint: row.get(2)?,
        account: row.get(3)?,
        folder: row.get(4)?,
        maildir_subdir: row.get(5)?,
        date: row.get(6)?,
        from: row.get(7)?,
        to: row.get(8)?,
        cc: row.get(9)?,
        bcc: row.get(10)?,
        subject: row.get(11)?,
        rfc_message_id: row.get(12)?,
        remote,
        is_duplicate: is_duplicate != 0,
    })
}

fn count_all_entries(conn: &Connection) -> Result<usize, VivariumError> {
    conn.query_row("SELECT COUNT(*) FROM catalog_entries", [], |row| row.get(0))
        .map_err(|e| VivariumError::Other(format!("failed to count catalog rows: {e}")))
}
