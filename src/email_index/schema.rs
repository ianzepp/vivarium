use rusqlite::{Connection, params};

use crate::error::VivariumError;

const INDEX_SCHEMA_VERSION: &str = "1";

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), VivariumError> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS index_metadata (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
          account TEXT NOT NULL,
          handle TEXT NOT NULL,
          catalog_handle TEXT NOT NULL,
          fingerprint TEXT NOT NULL,
          raw_path TEXT NOT NULL,
          folder TEXT NOT NULL,
          maildir_subdir TEXT NOT NULL,
          date TEXT NOT NULL,
          from_addr TEXT NOT NULL,
          to_addr TEXT NOT NULL,
          cc_addr TEXT NOT NULL,
          bcc_addr TEXT NOT NULL,
          subject TEXT NOT NULL,
          rfc_message_id TEXT,
          remote_mailbox TEXT,
          remote_uid INTEGER,
          remote_uidvalidity INTEGER,
          indexed_at TEXT NOT NULL,
          PRIMARY KEY (account, handle)
        );

        CREATE TABLE IF NOT EXISTS message_links (
          account TEXT NOT NULL,
          handle TEXT NOT NULL,
          link_kind TEXT NOT NULL,
          rfc_message_id TEXT NOT NULL,
          PRIMARY KEY (account, handle, link_kind, rfc_message_id),
          FOREIGN KEY (account, handle) REFERENCES messages(account, handle) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS messages_account_folder_idx ON messages(account, folder, date);
        CREATE INDEX IF NOT EXISTS messages_rfc_message_id_idx ON messages(account, rfc_message_id);
        CREATE INDEX IF NOT EXISTS message_links_rfc_idx ON message_links(account, rfc_message_id);
        ",
    )
    .map_err(|e| VivariumError::Other(format!("failed to initialize email index: {e}")))?;
    conn.execute(
        "INSERT OR REPLACE INTO index_metadata (key, value) VALUES ('schema_version', ?1)",
        params![INDEX_SCHEMA_VERSION],
    )
    .map_err(|e| VivariumError::Other(format!("failed to write index metadata: {e}")))?;
    Ok(())
}
