use std::collections::{BTreeSet, VecDeque};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::VivariumError;
use crate::store::{MessageLocation, secure_create_dir_all};

mod links;
mod rebuild;
mod schema;
#[cfg(test)]
mod tests;

const THREAD_WALK_LIMIT: usize = 10_000;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IndexStats {
    pub scanned: usize,
    pub updated: usize,
    pub reused: usize,
    pub stale: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedMessage {
    pub account: String,
    pub handle: String,
    pub catalog_handle: String,
    pub fingerprint: String,
    pub raw_path: String,
    pub folder: String,
    pub maildir_subdir: String,
    pub date: String,
    pub from_addr: String,
    pub to_addr: String,
    pub cc_addr: String,
    pub bcc_addr: String,
    pub subject: String,
    pub rfc_message_id: Option<String>,
}

impl IndexedMessage {
    pub fn location(&self) -> MessageLocation {
        MessageLocation {
            folder: self.folder.clone(),
            maildir_subdir: self.maildir_subdir.clone(),
            path: PathBuf::from(&self.raw_path),
        }
    }
}

pub struct EmailIndex {
    conn: Connection,
}

impl EmailIndex {
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        let vivarium_dir = mail_root.join(".vivarium");
        secure_create_dir_all(&vivarium_dir)?;
        let path = vivarium_dir.join("index.sqlite");
        let conn = Connection::open(&path)
            .map_err(|e| VivariumError::Other(format!("failed to open email index: {e}")))?;
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let index = Self { conn };
        schema::ensure_schema(&index.conn)?;
        Ok(index)
    }

    pub fn rebuild(mail_root: &Path, account: &str) -> Result<IndexStats, VivariumError> {
        rebuild::rebuild(mail_root, account)
    }

    pub fn message(
        &self,
        account: &str,
        handle: &str,
    ) -> Result<Option<IndexedMessage>, VivariumError> {
        self.conn
            .query_row(
                "SELECT account, handle, catalog_handle, fingerprint, raw_path, folder,
                        maildir_subdir, date, from_addr, to_addr, cc_addr, bcc_addr,
                        subject, rfc_message_id
                 FROM messages
                 WHERE account = ?1 AND handle = ?2",
                params![account, handle],
                indexed_message_from_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read indexed message: {e}")))
    }

    pub fn thread_messages(
        &self,
        account: &str,
        seed_handle: &str,
        _limit: usize,
    ) -> Result<Vec<IndexedMessage>, VivariumError> {
        let Some(seed) = self.message(account, seed_handle)? else {
            return Err(VivariumError::Message(format!(
                "message not found in index: {seed_handle}"
            )));
        };
        let handles = self.thread_handles(account, seed_handle, &seed)?;
        self.messages_for_handles(account, handles)
    }

    pub fn count_messages(&self, account: &str) -> Result<usize, VivariumError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account = ?1",
                params![account],
                |row| row.get::<_, usize>(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to count index rows: {e}")))
    }

    pub fn list_messages(&self, account: &str) -> Result<Vec<IndexedMessage>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT account, handle, catalog_handle, fingerprint, raw_path, folder,
                    maildir_subdir, date, from_addr, to_addr, cc_addr, bcc_addr,
                    subject, rfc_message_id
             FROM messages
             WHERE account = ?1
             ORDER BY date, handle",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare index listing: {e}")))?;
        let rows = stmt
            .query_map(params![account], indexed_message_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to list index rows: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read indexed row: {e}")))
        })
        .collect()
    }

    fn thread_handles(
        &self,
        account: &str,
        seed_handle: &str,
        seed: &IndexedMessage,
    ) -> Result<BTreeSet<String>, VivariumError> {
        let mut thread_ids = self.related_ids(account, seed_handle)?;
        if let Some(message_id) = &seed.rfc_message_id {
            thread_ids.insert(message_id.clone());
        }
        let mut handles = BTreeSet::from([seed_handle.to_string()]);
        let mut queue: VecDeque<String> = thread_ids.iter().cloned().collect();
        let mut seen_ids = thread_ids;

        while let Some(message_id) = queue.pop_front() {
            if seen_ids.len() > THREAD_WALK_LIMIT || handles.len() > THREAD_WALK_LIMIT {
                break;
            }
            self.expand_thread_id(
                account,
                &message_id,
                &mut handles,
                &mut seen_ids,
                &mut queue,
            )?;
        }
        Ok(handles)
    }

    fn expand_thread_id(
        &self,
        account: &str,
        message_id: &str,
        handles: &mut BTreeSet<String>,
        seen_ids: &mut BTreeSet<String>,
        queue: &mut VecDeque<String>,
    ) -> Result<(), VivariumError> {
        for handle in self.handles_linking_to(account, message_id)? {
            if !handles.insert(handle.clone()) {
                continue;
            }
            for related_id in self.related_ids(account, &handle)? {
                if seen_ids.insert(related_id.clone()) {
                    queue.push_back(related_id);
                }
            }
        }
        Ok(())
    }

    fn messages_for_handles(
        &self,
        account: &str,
        handles: BTreeSet<String>,
    ) -> Result<Vec<IndexedMessage>, VivariumError> {
        let mut messages = Vec::new();
        for handle in handles {
            if let Some(message) = self.message(account, &handle)? {
                messages.push(message);
            }
        }
        messages.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.handle.cmp(&b.handle)));
        Ok(messages)
    }

    fn related_ids(&self, account: &str, handle: &str) -> Result<BTreeSet<String>, VivariumError> {
        let mut ids = BTreeSet::new();
        let mut stmt = self
            .conn
            .prepare("SELECT rfc_message_id FROM message_links WHERE account = ?1 AND handle = ?2")
            .map_err(|e| {
                VivariumError::Other(format!("failed to prepare related id query: {e}"))
            })?;
        let rows = stmt
            .query_map(params![account, handle], |row| row.get::<_, String>(0))
            .map_err(|e| VivariumError::Other(format!("failed to query related ids: {e}")))?;
        for row in rows {
            ids.insert(
                row.map_err(|e| VivariumError::Other(format!("failed to read related id: {e}")))?,
            );
        }
        Ok(ids)
    }

    fn handles_linking_to(
        &self,
        account: &str,
        message_id: &str,
    ) -> Result<Vec<String>, VivariumError> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT handle FROM message_links WHERE account = ?1 AND rfc_message_id = ?2",
        ).map_err(|e| VivariumError::Other(format!("failed to prepare thread query: {e}")))?;
        let rows = stmt
            .query_map(params![account, message_id], |row| row.get::<_, String>(0))
            .map_err(|e| VivariumError::Other(format!("failed to query thread handles: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read thread handle: {e}")))
        })
        .collect()
    }
}

fn indexed_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedMessage> {
    Ok(IndexedMessage {
        account: row.get(0)?,
        handle: row.get(1)?,
        catalog_handle: row.get(2)?,
        fingerprint: row.get(3)?,
        raw_path: row.get(4)?,
        folder: row.get(5)?,
        maildir_subdir: row.get(6)?,
        date: row.get(7)?,
        from_addr: row.get(8)?,
        to_addr: row.get(9)?,
        cc_addr: row.get(10)?,
        bcc_addr: row.get(11)?,
        subject: row.get(12)?,
        rfc_message_id: row.get(13)?,
    })
}

pub fn rebuild(mail_root: &Path, account: &str) -> Result<IndexStats, VivariumError> {
    EmailIndex::rebuild(mail_root, account)
}

pub fn ensure_for_thread(
    mail_root: &Path,
    account: &str,
    seed_handle: &str,
) -> Result<EmailIndex, VivariumError> {
    let index = EmailIndex::open(mail_root)?;
    if index.message(account, seed_handle)?.is_some() {
        return Ok(index);
    }
    if index.count_messages(account)? == 0 {
        return Err(VivariumError::Message(format!(
            "email index is empty for account '{account}'; run `vivi index rebuild --account {account}`"
        )));
    }
    drop(index);
    EmailIndex::rebuild(mail_root, account)?;
    EmailIndex::open(mail_root)
}
