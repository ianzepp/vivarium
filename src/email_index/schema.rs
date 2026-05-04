use rusqlite::{Connection, params};

use crate::error::VivariumError;

const INDEX_SCHEMA_VERSION: &str = "2";

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), VivariumError> {
    let existing_version: Option<String> = conn
        .query_row(
            "SELECT value FROM index_metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    if existing_version.as_deref() != Some(INDEX_SCHEMA_VERSION) {
        conn.execute_batch(
            "
            DROP TABLE IF EXISTS message_links;
            DROP TABLE IF EXISTS messages;
            DROP TABLE IF EXISTS index_metadata;
            ",
        )
        .map_err(|e| VivariumError::Other(format!("failed to reset email index schema: {e}")))?;
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS index_metadata (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
          account TEXT NOT NULL,
          message_id TEXT NOT NULL,
          content_id TEXT NOT NULL,
          blob_path TEXT NOT NULL,
          local_role TEXT NOT NULL,
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
          PRIMARY KEY (account, message_id)
        );

        CREATE TABLE IF NOT EXISTS message_links (
          account TEXT NOT NULL,
          message_id TEXT NOT NULL,
          link_kind TEXT NOT NULL,
          normalized_message_id TEXT NOT NULL,
          PRIMARY KEY (account, message_id, link_kind, normalized_message_id),
          FOREIGN KEY (account, message_id) REFERENCES messages(account, message_id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS messages_account_role_idx ON messages(account, local_role, date);
        CREATE INDEX IF NOT EXISTS messages_rfc_message_id_idx ON messages(account, rfc_message_id);
        CREATE INDEX IF NOT EXISTS message_links_rfc_idx ON message_links(account, normalized_message_id);
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
