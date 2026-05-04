use rusqlite::{Connection, params};

use crate::error::VivariumError;

const INDEX_SCHEMA_VERSION: &str = "3";

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
            DROP TABLE IF EXISTS indexed_messages;
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

        CREATE TABLE IF NOT EXISTS indexed_messages (
          account TEXT NOT NULL,
          message_id TEXT NOT NULL,
          content_id TEXT NOT NULL,
          indexed_at TEXT NOT NULL,
          PRIMARY KEY (account, message_id),
          FOREIGN KEY (message_id) REFERENCES messages(message_id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS message_links (
          account TEXT NOT NULL,
          message_id TEXT NOT NULL,
          link_kind TEXT NOT NULL,
          normalized_message_id TEXT NOT NULL,
          PRIMARY KEY (account, message_id, link_kind, normalized_message_id),
          FOREIGN KEY (account, message_id) REFERENCES indexed_messages(account, message_id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS indexed_messages_account_content_idx
          ON indexed_messages(account, content_id);
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
