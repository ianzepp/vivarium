use rusqlite::Connection;

use crate::error::VivariumError;

pub(super) fn ensure_schema(conn: &Connection) -> Result<(), VivariumError> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS blobs (
           content_id TEXT PRIMARY KEY,
           blob_relpath TEXT NOT NULL UNIQUE,
           byte_size INTEGER NOT NULL,
           rfc_message_id TEXT,
           parsed_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS messages (
           message_id TEXT PRIMARY KEY,
           account TEXT NOT NULL,
           content_id TEXT NOT NULL REFERENCES blobs(content_id) ON DELETE RESTRICT,
           local_role TEXT NOT NULL,
           read_state INTEGER NOT NULL DEFAULT 0,
           starred INTEGER NOT NULL DEFAULT 0,
           draft_state TEXT,
           discovered_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           deleted_at TEXT
         );
         CREATE INDEX IF NOT EXISTS messages_account_role_idx
           ON messages(account, local_role, updated_at);
         CREATE INDEX IF NOT EXISTS messages_account_content_idx
           ON messages(account, content_id);
         CREATE TABLE IF NOT EXISTS remote_bindings (
           message_id TEXT PRIMARY KEY REFERENCES messages(message_id) ON DELETE CASCADE,
           account TEXT NOT NULL,
           provider TEXT NOT NULL,
           remote_mailbox TEXT NOT NULL,
           remote_uid INTEGER NOT NULL,
           remote_uidvalidity INTEGER NOT NULL,
           last_verified_at TEXT NOT NULL,
           stale INTEGER NOT NULL DEFAULT 0,
           UNIQUE (account, remote_mailbox, remote_uidvalidity, remote_uid)
         );
         CREATE TABLE IF NOT EXISTS message_metadata (
           content_id TEXT PRIMARY KEY REFERENCES blobs(content_id) ON DELETE CASCADE,
           date TEXT NOT NULL,
           from_addr TEXT NOT NULL,
           to_addr TEXT NOT NULL,
           cc_addr TEXT NOT NULL,
           bcc_addr TEXT NOT NULL,
           subject TEXT NOT NULL,
           normalized_message_id TEXT
         );
         COMMIT;",
    )
    .map_err(|e| VivariumError::Other(format!("failed to initialize storage schema: {e}")))
}

pub(super) fn message_query(where_clause: &str) -> String {
    format!(
        "SELECT
            m.message_id,
            m.account,
            m.content_id,
            b.blob_relpath,
            b.byte_size,
            m.local_role,
            m.read_state,
            m.starred,
            md.date,
            md.from_addr,
            md.to_addr,
            md.cc_addr,
            md.bcc_addr,
            md.subject,
            md.normalized_message_id,
            rb.account,
            rb.provider,
            rb.remote_mailbox,
            rb.remote_uid,
            rb.remote_uidvalidity
         FROM messages m
         JOIN blobs b ON b.content_id = m.content_id
         JOIN message_metadata md ON md.content_id = m.content_id
         LEFT JOIN remote_bindings rb ON rb.message_id = m.message_id
         {where_clause}"
    )
}
