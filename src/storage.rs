use std::fs::{self, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, params};
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
        let message_id = request
            .remote
            .as_ref()
            .map(|remote| {
                remote_bound_message_id(&request.account, &request.local_role, &content_id, remote)
            })
            .unwrap_or_else(|| fallback_message_id(request, &content_id));
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

fn opaque_message_id(seed: &str) -> String {
    let hash = sha256_hex(seed.as_bytes());
    format!("msg_{}", &hash[..24])
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
