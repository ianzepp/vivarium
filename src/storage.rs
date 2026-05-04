use std::collections::HashMap;
use std::fs::{self, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::catalog::{CatalogEntry, RemoteIdentity};
use crate::error::VivariumError;
use crate::message::normalize_message_id;
use crate::store::secure_create_dir_all;

const INTERNAL_DIR: &str = ".vivarium";
const STORAGE_DB_FILENAME: &str = "storage.sqlite";
const BLOBS_DIR: &str = "blobs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageImportResult {
    pub imported_messages: usize,
    pub imported_blobs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub message_id: String,
    pub content_id: String,
    pub blob_relpath: String,
    pub created_blob: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageIngestRequest {
    pub account: String,
    pub local_role: String,
    pub read_state: bool,
    pub starred: bool,
    pub message_id_hint: Option<String>,
    pub seed_hint: String,
    pub remote: Option<RemoteBindingInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBindingInput {
    pub account: String,
    pub provider: String,
    pub remote_mailbox: String,
    pub remote_uid: u32,
    pub remote_uidvalidity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessageView {
    pub handle: String,
    pub message_id: String,
    pub account: String,
    pub content_id: String,
    pub blob_relpath: String,
    pub byte_size: u64,
    pub local_role: String,
    pub read_state: bool,
    pub starred: bool,
    pub date: String,
    pub from_addr: String,
    pub to_addr: String,
    pub cc_addr: String,
    pub bcc_addr: String,
    pub subject: String,
    pub normalized_message_id: Option<String>,
    pub remote: Option<RemoteBindingInput>,
}

pub struct Storage {
    mail_root: PathBuf,
    conn: Connection,
}

impl Storage {
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        let internal_dir = mail_root.join(INTERNAL_DIR);
        secure_create_dir_all(&internal_dir)
            .map_err(|e| VivariumError::Other(format!("failed to create storage dir: {e}")))?;
        secure_create_dir_all(&mail_root.join(BLOBS_DIR))
            .map_err(|e| VivariumError::Other(format!("failed to create blob dir: {e}")))?;

        let db_path = internal_dir.join(STORAGE_DB_FILENAME);
        let conn = Connection::open(&db_path)
            .map_err(|e| VivariumError::Other(format!("failed to open storage database: {e}")))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| VivariumError::Other(format!("failed to set SQLite timeout: {e}")))?;
        #[cfg(unix)]
        fs::set_permissions(&db_path, fs::Permissions::from_mode(0o600))?;

        ensure_schema(&conn)?;

        Ok(Self {
            mail_root: mail_root.to_path_buf(),
            conn,
        })
    }

    pub fn import_catalog_entries(
        &mut self,
        entries: &[CatalogEntry],
    ) -> Result<StorageImportResult, VivariumError> {
        let mut result = StorageImportResult {
            imported_messages: 0,
            imported_blobs: 0,
        };
        for entry in entries {
            let data = fs::read(&entry.raw_path)?;
            let stored = self.ingest_message(&request_from_catalog_entry(entry), &data)?;
            result.imported_messages += 1;
            if stored.created_blob {
                result.imported_blobs += 1;
            }
        }
        Ok(result)
    }

    pub fn read_blob(&self, content_id: &str) -> Result<Vec<u8>, VivariumError> {
        let relpath: String = self
            .conn
            .query_row(
                "SELECT blob_relpath FROM blobs WHERE content_id = ?1",
                params![content_id],
                |row| row.get(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to read blob row: {e}")))?;
        fs::read(self.mail_root.join(relpath)).map_err(Into::into)
    }

    pub fn read_message(&self, message_id: &str) -> Result<Vec<u8>, VivariumError> {
        let resolved = self.resolve_message_token(message_id)?;
        let Some(view) = self.message_by_id(&resolved)? else {
            return Err(VivariumError::Message(format!(
                "message not found: {message_id}"
            )));
        };
        fs::read(self.mail_root.join(view.blob_relpath)).map_err(Into::into)
    }

    pub fn message_by_id(
        &self,
        message_id: &str,
    ) -> Result<Option<StoredMessageView>, VivariumError> {
        let mut message = self
            .conn
            .query_row(
                &message_query("WHERE m.message_id = ?1"),
                params![message_id],
                raw_stored_message_from_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read stored message: {e}")))?;
        if let Some(message) = &mut message {
            message.handle = self.display_handle(&message.message_id)?;
        }
        Ok(message)
    }

    pub fn list_messages_by_role(
        &self,
        local_role: &str,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "{} ORDER BY md.date DESC, m.message_id",
                message_query("WHERE m.local_role = ?1 AND m.deleted_at IS NULL")
            ))
            .map_err(|e| VivariumError::Other(format!("failed to prepare storage listing: {e}")))?;
        let rows = stmt
            .query_map(params![local_role], raw_stored_message_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to list stored messages: {e}")))?;
        let messages: Result<Vec<_>, _> = rows
            .map(|row| {
                row.map_err(|e| {
                    VivariumError::Other(format!("failed to read stored message row: {e}"))
                })
            })
            .collect();
        self.decorate_handles(messages?)
    }

    pub fn list_catalog_entries(&self, account: &str) -> Result<Vec<CatalogEntry>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "{} ORDER BY md.date DESC, m.message_id",
                message_query("WHERE m.account = ?1 AND m.deleted_at IS NULL")
            ))
            .map_err(|e| {
                VivariumError::Other(format!("failed to prepare catalog view listing: {e}"))
            })?;
        let rows = stmt
            .query_map(params![account], raw_stored_message_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to query catalog view: {e}")))?;
        let messages: Result<Vec<_>, _> = rows
            .map(|row| {
                row.map_err(|e| {
                    VivariumError::Other(format!("failed to read catalog view row: {e}"))
                })
            })
            .collect();
        messages?
            .into_iter()
            .map(|message| self.catalog_entry_from_view(message))
            .collect()
    }

    pub fn catalog_entry(
        &self,
        account: &str,
        handle_or_id: &str,
    ) -> Result<Option<CatalogEntry>, VivariumError> {
        let Some(view) = self
            .conn
            .query_row(
                &format!(
                    "{} WHERE m.account = ?1 AND m.deleted_at IS NULL AND m.message_id = ?2",
                    message_query("")
                ),
                params![account, handle_or_id],
                raw_stored_message_from_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read catalog entry: {e}")))?
        else {
            return Ok(None);
        };
        self.catalog_entry_from_view(view).map(Some)
    }

    pub fn count_messages_for_account(&self, account: &str) -> Result<usize, VivariumError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account = ?1 AND deleted_at IS NULL",
                params![account],
                |row| row.get(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to count stored messages: {e}")))
    }

    pub fn local_sizes_by_role(
        &self,
        local_role: &str,
    ) -> Result<HashMap<String, u64>, VivariumError> {
        let messages = self.list_messages_by_role(local_role)?;
        Ok(messages
            .into_iter()
            .map(|message| {
                let key = message
                    .remote
                    .as_ref()
                    .map(|remote| format!("{local_role}-{}", remote.remote_uid))
                    .unwrap_or(message.message_id);
                (key, message.byte_size)
            })
            .collect())
    }

    pub fn rfc_index_by_role(
        &self,
        local_role: &str,
    ) -> Result<HashMap<String, (u32, u64)>, VivariumError> {
        let messages = self.list_messages_by_role(local_role)?;
        let mut map = HashMap::new();
        for message in messages {
            let Some(rfc_message_id) = message.normalized_message_id.clone() else {
                continue;
            };
            let uid = message
                .remote
                .as_ref()
                .map(|remote| remote.remote_uid)
                .or_else(|| {
                    message
                        .message_id
                        .rsplit_once('-')
                        .and_then(|(_, uid)| uid.parse().ok())
                })
                .unwrap_or(0);
            map.insert(rfc_message_id, (uid, message.byte_size));
        }
        Ok(map)
    }

    pub fn resolve_message_token(&self, token: &str) -> Result<String, VivariumError> {
        if self.message_by_id_exact(token)?.is_some() {
            return Ok(token.to_string());
        }
        let message_ids = self.active_message_ids()?;
        let handle_map = short_handle_map(&message_ids);
        let handle_matches: Vec<_> = handle_map
            .iter()
            .filter_map(|(message_id, handle)| (handle == token).then_some(message_id.clone()))
            .collect();
        match handle_matches.len() {
            1 => return Ok(handle_matches[0].clone()),
            n if n > 1 => {
                return Err(VivariumError::Message(format!(
                    "ambiguous handle '{token}'; matches {} messages",
                    n
                )));
            }
            _ => {}
        }
        let id_prefix_matches: Vec<_> = message_ids
            .iter()
            .filter(|message_id| message_id.starts_with(token))
            .cloned()
            .collect();
        match id_prefix_matches.len() {
            1 => return Ok(id_prefix_matches[0].clone()),
            n if n > 1 => {
                return Err(VivariumError::Message(format!(
                    "ambiguous message_id prefix '{token}'; matches {} messages",
                    n
                )));
            }
            _ => {}
        }
        let content_matches = self.content_prefix_matches(token)?;
        match content_matches.len() {
            1 => Ok(content_matches[0].clone()),
            n if n > 1 => Err(VivariumError::Message(format!(
                "ambiguous content_id prefix '{token}'; matches {} messages",
                n
            ))),
            _ => Err(VivariumError::Message(format!(
                "message not found: {token}"
            ))),
        }
    }

    pub fn display_handle(&self, message_id: &str) -> Result<String, VivariumError> {
        Ok(self
            .handle_map()?
            .get(message_id)
            .cloned()
            .unwrap_or_else(|| message_id.to_string()))
    }

    pub fn handle_map(&self) -> Result<HashMap<String, String>, VivariumError> {
        let message_ids = self.active_message_ids()?;
        Ok(short_handle_map(&message_ids))
    }

    #[cfg(test)]
    fn blob_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count blobs: {e}")))
    }

    #[cfg(test)]
    fn message_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count messages: {e}")))
    }

    #[cfg(test)]
    fn remote_binding_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM remote_bindings", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count remote bindings: {e}")))
    }

    pub fn ingest_message(
        &mut self,
        request: &MessageIngestRequest,
        data: &[u8],
    ) -> Result<StoredMessage, VivariumError> {
        let content_id = sha256_hex(data);
        let blob_relpath = blob_relpath(&content_id);
        let blob_abspath = self.mail_root.join(&blob_relpath);
        let created_blob = write_blob_if_absent(&blob_abspath, data)?;
        let metadata = parse_metadata(data);
        let message_id = request.message_id_hint.clone().unwrap_or_else(|| {
            request
                .remote
                .as_ref()
                .map(|remote| {
                    remote_bound_message_id(
                        &request.account,
                        &request.local_role,
                        &content_id,
                        remote,
                    )
                })
                .unwrap_or_else(|| fallback_message_id(request, &content_id))
        });
        let now = Utc::now().to_rfc3339();

        let tx = self.conn.transaction().map_err(|e| {
            VivariumError::Other(format!("failed to open storage transaction: {e}"))
        })?;
        tx.execute(
            "INSERT INTO blobs (content_id, blob_relpath, byte_size, rfc_message_id, parsed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(content_id) DO UPDATE SET
               blob_relpath = excluded.blob_relpath,
               byte_size = excluded.byte_size,
               rfc_message_id = COALESCE(excluded.rfc_message_id, blobs.rfc_message_id)",
            params![
                content_id,
                blob_relpath,
                i64::try_from(data.len()).unwrap_or(i64::MAX),
                metadata.normalized_message_id,
                now,
            ],
        )
        .map_err(|e| VivariumError::Other(format!("failed to upsert blob row: {e}")))?;
        tx.execute(
            "INSERT INTO message_metadata (
               content_id, date, from_addr, to_addr, cc_addr, bcc_addr, subject, normalized_message_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(content_id) DO UPDATE SET
               date = excluded.date,
               from_addr = excluded.from_addr,
               to_addr = excluded.to_addr,
               cc_addr = excluded.cc_addr,
               bcc_addr = excluded.bcc_addr,
               subject = excluded.subject,
               normalized_message_id = excluded.normalized_message_id",
            params![
                content_id,
                metadata.date,
                metadata.from_addr,
                metadata.to_addr,
                metadata.cc_addr,
                metadata.bcc_addr,
                metadata.subject,
                metadata.normalized_message_id,
            ],
        )
        .map_err(|e| VivariumError::Other(format!("failed to upsert message metadata: {e}")))?;
        tx.execute(
            "INSERT INTO messages (
               message_id, account, content_id, local_role, read_state, starred,
               draft_state, discovered_at, updated_at, deleted_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, NULL)
             ON CONFLICT(message_id) DO UPDATE SET
               content_id = excluded.content_id,
               local_role = excluded.local_role,
               read_state = excluded.read_state,
               starred = excluded.starred,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                message_id,
                request.account,
                content_id,
                request.local_role,
                if request.read_state { 1 } else { 0 },
                if request.starred { 1 } else { 0 },
                now,
                now,
            ],
        )
        .map_err(|e| VivariumError::Other(format!("failed to upsert message row: {e}")))?;
        if let Some(remote) = &request.remote {
            tx.execute(
                "INSERT INTO remote_bindings (
                   message_id, account, provider, remote_mailbox, remote_uid,
                   remote_uidvalidity, last_verified_at, stale
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
                 ON CONFLICT(message_id) DO UPDATE SET
                   account = excluded.account,
                   provider = excluded.provider,
                   remote_mailbox = excluded.remote_mailbox,
                   remote_uid = excluded.remote_uid,
                   remote_uidvalidity = excluded.remote_uidvalidity,
                   last_verified_at = excluded.last_verified_at,
                   stale = 0",
                params![
                    message_id,
                    remote.account,
                    remote.provider,
                    remote.remote_mailbox,
                    remote.remote_uid,
                    remote.remote_uidvalidity,
                    now,
                ],
            )
            .map_err(|e| VivariumError::Other(format!("failed to upsert remote binding: {e}")))?;
        }
        tx.commit().map_err(|e| {
            VivariumError::Other(format!("failed to commit storage transaction: {e}"))
        })?;

        Ok(StoredMessage {
            message_id,
            content_id,
            blob_relpath,
            created_blob,
        })
    }

    #[cfg(test)]
    fn store_catalog_entry(
        &mut self,
        entry: &CatalogEntry,
        data: &[u8],
    ) -> Result<StoredMessage, VivariumError> {
        self.ingest_message(&request_from_catalog_entry(entry), data)
    }

    fn active_message_ids(&self) -> Result<Vec<String>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare("SELECT message_id FROM messages WHERE deleted_at IS NULL ORDER BY message_id")
            .map_err(|e| {
                VivariumError::Other(format!("failed to prepare message id query: {e}"))
            })?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| VivariumError::Other(format!("failed to query message ids: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read message id row: {e}")))
        })
        .collect()
    }

    fn message_by_id_exact(&self, message_id: &str) -> Result<Option<String>, VivariumError> {
        self.conn
            .query_row(
                "SELECT message_id FROM messages WHERE message_id = ?1 AND deleted_at IS NULL",
                params![message_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read exact message id: {e}")))
    }

    fn content_prefix_matches(&self, token: &str) -> Result<Vec<String>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT message_id
                 FROM messages
                 WHERE deleted_at IS NULL AND content_id LIKE ?1
                 ORDER BY message_id",
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to prepare content prefix query: {e}"))
            })?;
        let rows = stmt
            .query_map(params![format!("{token}%")], |row| row.get::<_, String>(0))
            .map_err(|e| {
                VivariumError::Other(format!("failed to query content prefix matches: {e}"))
            })?;
        rows.map(|row| {
            row.map_err(|e| {
                VivariumError::Other(format!("failed to read content prefix match row: {e}"))
            })
        })
        .collect()
    }

    fn decorate_handles(
        &self,
        mut messages: Vec<StoredMessageView>,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let handle_map = short_handle_map(&self.active_message_ids()?);
        for message in &mut messages {
            message.handle = handle_map
                .get(&message.message_id)
                .cloned()
                .unwrap_or_else(|| message.message_id.clone());
        }
        Ok(messages)
    }
}

fn raw_stored_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessageView> {
    let remote_account: Option<String> = row.get(15)?;
    let remote = if let Some(account) = remote_account {
        Some(RemoteBindingInput {
            account,
            provider: row.get(16)?,
            remote_mailbox: row.get(17)?,
            remote_uid: row.get(18)?,
            remote_uidvalidity: row.get(19)?,
        })
    } else {
        None
    };
    let message_id: String = row.get(0)?;
    Ok(StoredMessageView {
        handle: message_id.clone(),
        message_id,
        account: row.get(1)?,
        content_id: row.get(2)?,
        blob_relpath: row.get(3)?,
        byte_size: row.get::<_, i64>(4)? as u64,
        local_role: row.get(5)?,
        read_state: row.get::<_, i64>(6)? != 0,
        starred: row.get::<_, i64>(7)? != 0,
        date: row.get(8)?,
        from_addr: row.get(9)?,
        to_addr: row.get(10)?,
        cc_addr: row.get(11)?,
        bcc_addr: row.get(12)?,
        subject: row.get(13)?,
        normalized_message_id: row.get(14)?,
        remote,
    })
}

pub fn import_catalog_entries(
    mail_root: &Path,
    entries: &[CatalogEntry],
) -> Result<StorageImportResult, VivariumError> {
    let mut storage = Storage::open(mail_root)?;
    storage.import_catalog_entries(entries)
}

fn ensure_schema(conn: &Connection) -> Result<(), VivariumError> {
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

fn message_query(where_clause: &str) -> String {
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

fn blob_relpath(content_id: &str) -> String {
    format!(
        "{}/{}/{}/{}.eml",
        BLOBS_DIR,
        &content_id[..2],
        &content_id[2..4],
        content_id
    )
}

fn write_blob_if_absent(path: &Path, data: &[u8]) -> Result<bool, VivariumError> {
    if path.exists() {
        return Ok(false);
    }
    let Some(parent) = path.parent() else {
        return Err(VivariumError::Other(format!(
            "blob path has no parent: {}",
            path.display()
        )));
    };
    secure_create_dir_all(parent)
        .map_err(|e| VivariumError::Other(format!("failed to create blob dir: {e}")))?;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(data)?;
            file.sync_all()?;
            #[cfg(unix)]
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(VivariumError::Io(e)),
    }
}

fn local_role(folder: &str) -> String {
    match folder {
        "INBOX" | "Inbox" | "inbox" => "inbox".into(),
        "Archive" | "archive" => "archive".into(),
        "Trash" | "trash" => "trash".into(),
        "Sent" | "sent" => "sent".into(),
        "Drafts" | "drafts" => "drafts".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn remote_bound_message_id(
    account: &str,
    local_role: &str,
    content_id: &str,
    remote: &RemoteBindingInput,
) -> String {
    let seed = format!(
        "remote\0{account}\0{local_role}\0{content_id}\0{}\0{}\0{}",
        remote.remote_mailbox, remote.remote_uidvalidity, remote.remote_uid
    );
    opaque_message_id(&seed)
}

fn fallback_message_id(request: &MessageIngestRequest, content_id: &str) -> String {
    let seed = format!(
        "local\0{}\0{}\0{}\0{}",
        request.account, request.local_role, request.seed_hint, content_id
    );
    opaque_message_id(&seed)
}

fn request_from_catalog_entry(entry: &CatalogEntry) -> MessageIngestRequest {
    MessageIngestRequest {
        account: entry.account.clone(),
        local_role: local_role(&entry.folder),
        read_state: entry.maildir_subdir == "cur",
        starred: path_has_maildir_flag(&entry.raw_path, 'F'),
        message_id_hint: Some(entry.handle.clone()),
        seed_hint: entry.handle.clone(),
        remote: entry.remote.as_ref().map(remote_binding_from_catalog),
    }
}

fn remote_binding_from_catalog(remote: &RemoteIdentity) -> RemoteBindingInput {
    RemoteBindingInput {
        account: remote.account.clone(),
        provider: remote.provider.clone(),
        remote_mailbox: remote.remote_mailbox.clone(),
        remote_uid: remote.uid,
        remote_uidvalidity: remote.uidvalidity,
    }
}

impl Storage {
    fn catalog_entry_from_view(
        &self,
        message: StoredMessageView,
    ) -> Result<CatalogEntry, VivariumError> {
        let data = self.read_blob(&message.content_id)?;
        let remote = message.remote.as_ref().map(|binding| RemoteIdentity {
            account: binding.account.clone(),
            provider: binding.provider.clone(),
            remote_mailbox: binding.remote_mailbox.clone(),
            local_folder: message.local_role.clone(),
            uid: binding.remote_uid,
            uidvalidity: binding.remote_uidvalidity,
            rfc_message_id: message.normalized_message_id.clone().unwrap_or_default(),
            size: message.byte_size,
            content_fingerprint: message.content_id.clone(),
        });
        Ok(CatalogEntry {
            handle: message.message_id.clone(),
            raw_path: self
                .mail_root
                .join(&message.blob_relpath)
                .to_string_lossy()
                .to_string(),
            fingerprint: sha256_hex(&data),
            account: message.account,
            folder: folder_name(&message.local_role),
            maildir_subdir: if message.read_state {
                "cur".into()
            } else {
                "new".into()
            },
            date: message.date,
            from: message.from_addr,
            to: message.to_addr,
            cc: message.cc_addr,
            bcc: message.bcc_addr,
            subject: message.subject,
            rfc_message_id: message.normalized_message_id.unwrap_or_default(),
            remote,
            is_duplicate: false,
        })
    }
}

fn folder_name(local_role: &str) -> String {
    match local_role {
        "inbox" => "INBOX".into(),
        "archive" => "Archive".into(),
        "trash" => "Trash".into(),
        "sent" => "Sent".into(),
        "drafts" | "draft" => "Drafts".into(),
        other => other.to_string(),
    }
}

fn opaque_message_id(seed: &str) -> String {
    let hash = sha256_hex(seed.as_bytes());
    format!("msg_{}", &hash[..24])
}

fn short_handle_map(message_ids: &[String]) -> HashMap<String, String> {
    let bases = message_ids
        .iter()
        .map(|message_id| (message_id.clone(), handle_basis(message_id).to_string()))
        .collect::<Vec<_>>();
    let mut map = HashMap::new();
    for (message_id, basis) in &bases {
        if !message_id.starts_with("msg_") {
            map.insert(message_id.clone(), message_id.clone());
            continue;
        }
        let min_len = usize::min(7, basis.len());
        let mut handle = basis.clone();
        for len in min_len..=basis.len() {
            let prefix = &basis[..len];
            let count = bases
                .iter()
                .filter(|(_, other)| other.starts_with(prefix))
                .count();
            if count == 1 {
                handle = prefix.to_string();
                break;
            }
        }
        map.insert(message_id.clone(), handle);
    }
    map
}

fn handle_basis(message_id: &str) -> &str {
    message_id.strip_prefix("msg_").unwrap_or(message_id)
}

fn path_has_maildir_flag(path: &str, flag: char) -> bool {
    path.rsplit_once(":2,")
        .map(|(_, flags)| flags.contains(flag))
        .unwrap_or(false)
}

fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

#[derive(Debug, Clone)]
struct ParsedMetadata {
    date: String,
    from_addr: String,
    to_addr: String,
    cc_addr: String,
    bcc_addr: String,
    subject: String,
    normalized_message_id: Option<String>,
}

fn parse_metadata(data: &[u8]) -> ParsedMetadata {
    let Some(parsed) = mail_parser::MessageParser::default().parse(data) else {
        return ParsedMetadata {
            date: String::new(),
            from_addr: String::new(),
            to_addr: String::new(),
            cc_addr: String::new(),
            bcc_addr: String::new(),
            subject: String::new(),
            normalized_message_id: None,
        };
    };

    ParsedMetadata {
        date: parsed
            .date()
            .and_then(|d| chrono::DateTime::from_timestamp(d.to_timestamp(), 0))
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
        from_addr: address_list(parsed.from()),
        to_addr: address_list(parsed.to()),
        cc_addr: address_list(parsed.cc()),
        bcc_addr: address_list(parsed.bcc()),
        subject: parsed.subject().unwrap_or_default().to_string(),
        normalized_message_id: parsed.message_id().and_then(normalize_message_id),
    }
}

fn address_list(list: Option<&mail_parser::Address<'_>>) -> String {
    list.map(|addresses| {
        addresses
            .iter()
            .filter_map(|addr| {
                let email = addr.address()?;
                let name = addr.name().unwrap_or("");
                if name.is_empty() {
                    Some(email.to_string())
                } else {
                    Some(format!("{name} <{email}>"))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_dedupes_blobs_but_keeps_distinct_message_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = message_bytes("dup@example.com", "same body");
        let first = write_catalog_file(tmp.path(), "inbox-1.eml", &raw);
        let second = write_catalog_file(tmp.path(), "archive-2.eml", &raw);

        let entries = vec![
            catalog_entry("acct", "one", &first, "INBOX", Some(remote("INBOX", 7))),
            catalog_entry(
                "acct",
                "two",
                &second,
                "Archive",
                Some(remote("Archive", 8)),
            ),
        ];

        let result = import_catalog_entries(tmp.path(), &entries).unwrap();
        let storage = Storage::open(tmp.path()).unwrap();

        assert_eq!(result.imported_messages, 2);
        assert_eq!(result.imported_blobs, 1);
        assert_eq!(storage.blob_count().unwrap(), 1);
        assert_eq!(storage.message_count().unwrap(), 2);
        assert_eq!(storage.remote_binding_count().unwrap(), 2);
    }

    #[test]
    fn import_persists_blob_and_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = b"Message-ID: <meta@example.com>\r\nFrom: Agent <agent@example.com>\r\nTo: User <user@example.com>\r\nSubject: hello\r\n\r\nbody";
        let path = write_catalog_file(tmp.path(), "inbox-1.eml", raw);
        let entries = vec![catalog_entry(
            "acct",
            "one",
            &path,
            "INBOX",
            Some(remote("INBOX", 7)),
        )];

        import_catalog_entries(tmp.path(), &entries).unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        let data = storage.read_blob(&resulting_content_id(raw)).unwrap();

        assert_eq!(data, raw);
        assert_eq!(storage.blob_count().unwrap(), 1);
        assert_eq!(storage.message_count().unwrap(), 1);
    }

    #[test]
    fn fallback_message_ids_are_stable_for_unbound_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = message_bytes("local@example.com", "body");
        let path = write_catalog_file(tmp.path(), "draft-1.eml", &raw);
        let entry = catalog_entry("acct", "draft-handle", &path, "Drafts", None);

        let mut storage = Storage::open(tmp.path()).unwrap();
        let first = storage.store_catalog_entry(&entry, &raw).unwrap();
        let second = storage.store_catalog_entry(&entry, &raw).unwrap();

        assert_eq!(first.message_id, second.message_id);
        assert_eq!(storage.message_count().unwrap(), 1);
    }

    #[test]
    fn direct_ingest_api_supports_clean_break_sync_target() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = message_bytes("direct@example.com", "body");
        let request = MessageIngestRequest {
            account: "acct".into(),
            local_role: "inbox".into(),
            read_state: false,
            starred: false,
            message_id_hint: None,
            seed_hint: "uid:99".into(),
            remote: Some(RemoteBindingInput {
                account: "acct".into(),
                provider: "protonmail".into(),
                remote_mailbox: "INBOX".into(),
                remote_uid: 99,
                remote_uidvalidity: 42,
            }),
        };

        let mut storage = Storage::open(tmp.path()).unwrap();
        let stored = storage.ingest_message(&request, &raw).unwrap();

        assert!(stored.message_id.starts_with("msg_"));
        assert_eq!(storage.blob_count().unwrap(), 1);
        assert_eq!(storage.message_count().unwrap(), 1);
        assert_eq!(storage.remote_binding_count().unwrap(), 1);
    }

    #[test]
    fn short_handles_resolve_uniquely_for_storage_native_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let mut storage = Storage::open(tmp.path()).unwrap();

        let first = storage
            .ingest_message(
                &MessageIngestRequest {
                    account: "acct".into(),
                    local_role: "inbox".into(),
                    read_state: false,
                    starred: false,
                    message_id_hint: None,
                    seed_hint: "remote_uid:1".into(),
                    remote: Some(RemoteBindingInput {
                        account: "acct".into(),
                        provider: "protonmail".into(),
                        remote_mailbox: "INBOX".into(),
                        remote_uid: 1,
                        remote_uidvalidity: 42,
                    }),
                },
                &message_bytes("one@example.com", "first"),
            )
            .unwrap();
        let second = storage
            .ingest_message(
                &MessageIngestRequest {
                    account: "acct".into(),
                    local_role: "inbox".into(),
                    read_state: false,
                    starred: false,
                    message_id_hint: None,
                    seed_hint: "remote_uid:2".into(),
                    remote: Some(RemoteBindingInput {
                        account: "acct".into(),
                        provider: "protonmail".into(),
                        remote_mailbox: "INBOX".into(),
                        remote_uid: 2,
                        remote_uidvalidity: 42,
                    }),
                },
                &message_bytes("two@example.com", "second"),
            )
            .unwrap();

        let first_handle = storage.display_handle(&first.message_id).unwrap();
        let second_handle = storage.display_handle(&second.message_id).unwrap();

        assert_ne!(first_handle, second_handle);
        assert!(first_handle.len() >= 7);
        assert_eq!(
            storage.resolve_message_token(&first_handle).unwrap(),
            first.message_id
        );
        assert_eq!(
            storage.resolve_message_token(&second_handle).unwrap(),
            second.message_id
        );
    }

    #[test]
    fn content_id_prefix_can_resolve_message() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = message_bytes("content@example.com", "body");
        let mut storage = Storage::open(tmp.path()).unwrap();
        let stored = storage
            .ingest_message(
                &MessageIngestRequest {
                    account: "acct".into(),
                    local_role: "inbox".into(),
                    read_state: false,
                    starred: false,
                    message_id_hint: None,
                    seed_hint: "remote_uid:3".into(),
                    remote: Some(RemoteBindingInput {
                        account: "acct".into(),
                        provider: "protonmail".into(),
                        remote_mailbox: "INBOX".into(),
                        remote_uid: 3,
                        remote_uidvalidity: 42,
                    }),
                },
                &raw,
            )
            .unwrap();

        let prefix = &stored.content_id[..12];
        assert_eq!(
            storage.resolve_message_token(prefix).unwrap(),
            stored.message_id
        );
    }

    #[test]
    fn local_size_fallback_uses_remote_uid_shape_for_storage_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let raw = message_bytes("size@example.com", "body");
        let mut storage = Storage::open(tmp.path()).unwrap();
        storage
            .ingest_message(
                &MessageIngestRequest {
                    account: "acct".into(),
                    local_role: "inbox".into(),
                    read_state: false,
                    starred: false,
                    message_id_hint: None,
                    seed_hint: "remote_uid:7".into(),
                    remote: Some(RemoteBindingInput {
                        account: "acct".into(),
                        provider: "protonmail".into(),
                        remote_mailbox: "INBOX".into(),
                        remote_uid: 7,
                        remote_uidvalidity: 42,
                    }),
                },
                &raw,
            )
            .unwrap();

        let sizes = storage.local_sizes_by_role("inbox").unwrap();
        assert_eq!(sizes.get("inbox-7"), Some(&(raw.len() as u64)));
    }

    fn message_bytes(message_id: &str, body: &str) -> Vec<u8> {
        format!(
            "Message-ID: <{message_id}>\r\nFrom: Agent <agent@example.com>\r\nTo: User <user@example.com>\r\nSubject: hi\r\n\r\n{body}"
        )
        .into_bytes()
    }

    fn resulting_content_id(data: &[u8]) -> String {
        sha256_hex(data)
    }

    fn write_catalog_file(root: &Path, name: &str, data: &[u8]) -> String {
        let path = root.join(name);
        fs::write(&path, data).unwrap();
        path.to_string_lossy().to_string()
    }

    fn catalog_entry(
        account: &str,
        handle: &str,
        raw_path: &str,
        folder: &str,
        remote: Option<RemoteIdentity>,
    ) -> CatalogEntry {
        CatalogEntry {
            handle: handle.into(),
            raw_path: raw_path.into(),
            fingerprint: sha256_hex(&fs::read(raw_path).unwrap()),
            account: account.into(),
            folder: folder.into(),
            maildir_subdir: "new".into(),
            date: "2026-05-03T12:00:00Z".into(),
            from: "agent@example.com".into(),
            to: "user@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            rfc_message_id: "meta@example.com".into(),
            remote,
            is_duplicate: false,
        }
    }

    fn remote(mailbox: &str, uid: u32) -> RemoteIdentity {
        RemoteIdentity {
            account: "acct".into(),
            provider: "protonmail".into(),
            remote_mailbox: mailbox.into(),
            local_folder: mailbox.to_ascii_lowercase(),
            uid,
            uidvalidity: 42,
            rfc_message_id: "meta@example.com".into(),
            size: 128,
            content_fingerprint: "unused".into(),
        }
    }
}
