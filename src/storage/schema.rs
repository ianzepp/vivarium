use rusqlite::Connection;

use crate::error::VivariumError;

const STORAGE_SCHEMA_VERSION: &str = "2";

#[allow(clippy::too_many_lines)]
pub(super) fn ensure_schema(conn: &Connection) -> Result<(), VivariumError> {
    // Fast path: skip DDL when schema is already current
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM storage_metadata WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    if existing.as_deref() == Some(STORAGE_SCHEMA_VERSION) {
        return Ok(());
    }

    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS storage_metadata (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );

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
         CREATE TABLE IF NOT EXISTS mailspace_events (
           event_id INTEGER PRIMARY KEY AUTOINCREMENT,
           occurred_at TEXT NOT NULL,
           command TEXT NOT NULL,
           event_type TEXT NOT NULL,
           actor_identity TEXT,
           account TEXT NOT NULL,
           message_id TEXT NOT NULL REFERENCES messages(message_id) ON DELETE CASCADE,
           content_id TEXT NOT NULL,
           from_role TEXT,
           to_role TEXT,
           from_identity TEXT,
           to_identity TEXT,
           subject TEXT NOT NULL,
           note TEXT
         );
         CREATE INDEX IF NOT EXISTS mailspace_events_message_idx
           ON mailspace_events(message_id, occurred_at, event_id);
         CREATE INDEX IF NOT EXISTS mailspace_events_account_idx
           ON mailspace_events(account, occurred_at, event_id);
         CREATE TABLE IF NOT EXISTS mailspace_item_metadata (
           message_id TEXT NOT NULL REFERENCES messages(message_id) ON DELETE CASCADE,
           key TEXT NOT NULL,
           value TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           PRIMARY KEY (message_id, key)
         );
         CREATE INDEX IF NOT EXISTS mailspace_item_metadata_key_idx
           ON mailspace_item_metadata(key, value);
         CREATE TABLE IF NOT EXISTS mailspace_links (
           child_content_id TEXT PRIMARY KEY REFERENCES blobs(content_id) ON DELETE CASCADE,
           parent_content_id TEXT NOT NULL REFERENCES blobs(content_id) ON DELETE RESTRICT,
           source TEXT NOT NULL CHECK (source IN ('captured', 'inferred', 'source'))
         );
         CREATE INDEX IF NOT EXISTS mailspace_links_parent_idx
           ON mailspace_links(parent_content_id, child_content_id);
         CREATE TABLE IF NOT EXISTS work_graphs (
           handle TEXT PRIMARY KEY,
           code TEXT NOT NULL UNIQUE,
           status TEXT NOT NULL DEFAULT 'open',
           current_revision INTEGER NOT NULL DEFAULT 0,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS work_graph_revisions (
           graph_handle TEXT NOT NULL REFERENCES work_graphs(handle) ON DELETE CASCADE,
           revision INTEGER NOT NULL,
           mermaid_source TEXT NOT NULL,
           content_hash TEXT NOT NULL,
           created_at TEXT NOT NULL,
           PRIMARY KEY (graph_handle, revision)
         );
         CREATE TABLE IF NOT EXISTS work_graph_nodes (
           handle TEXT PRIMARY KEY,
           graph_handle TEXT NOT NULL REFERENCES work_graphs(handle) ON DELETE CASCADE,
           source_id TEXT NOT NULL,
           label TEXT NOT NULL,
           state TEXT NOT NULL DEFAULT 'open',
           subgraph TEXT,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           UNIQUE (graph_handle, source_id)
         );
         CREATE INDEX IF NOT EXISTS work_graph_nodes_graph_idx
           ON work_graph_nodes(graph_handle, state);
         CREATE TABLE IF NOT EXISTS work_graph_edges (
           handle TEXT PRIMARY KEY,
           graph_handle TEXT NOT NULL REFERENCES work_graphs(handle) ON DELETE CASCADE,
           from_node TEXT NOT NULL REFERENCES work_graph_nodes(handle) ON DELETE CASCADE,
           to_node TEXT NOT NULL REFERENCES work_graph_nodes(handle) ON DELETE CASCADE,
           label TEXT,
           created_at TEXT NOT NULL,
           UNIQUE (graph_handle, from_node, to_node)
         );
         CREATE INDEX IF NOT EXISTS work_graph_edges_to_idx
           ON work_graph_edges(to_node);
         CREATE INDEX IF NOT EXISTS work_graph_edges_from_idx
           ON work_graph_edges(from_node);
         CREATE TABLE IF NOT EXISTS work_graph_events (
           event_id INTEGER PRIMARY KEY AUTOINCREMENT,
           graph_handle TEXT NOT NULL REFERENCES work_graphs(handle) ON DELETE CASCADE,
           occurred_at TEXT NOT NULL,
           event_type TEXT NOT NULL,
           node_handle TEXT,
           note TEXT
         );
         CREATE INDEX IF NOT EXISTS work_graph_events_graph_idx
           ON work_graph_events(graph_handle, occurred_at, event_id);
         COMMIT;",
    )
    .map_err(|e| VivariumError::Other(format!("failed to initialize storage schema: {e}")))?;

    conn.execute(
        "INSERT OR REPLACE INTO storage_metadata (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![STORAGE_SCHEMA_VERSION],
    )
    .map_err(|e| VivariumError::Other(format!("failed to write storage schema version: {e}")))?;

    Ok(())
}

pub(super) fn message_query(where_clause: &str) -> String {
    [
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
         LEFT JOIN remote_bindings rb ON rb.message_id = m.message_id ",
        where_clause,
    ]
    .concat()
}
