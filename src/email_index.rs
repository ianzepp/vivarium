use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::VivariumError;
use crate::storage::Storage;
use crate::store::MessageLocation;

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
    pub handle: String,
    pub account: String,
    pub message_id: String,
    pub content_id: String,
    pub blob_path: String,
    pub local_role: String,
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
            message_id: Some(self.message_id.clone()),
            local_role: self.local_role.clone(),
            content_id: Some(self.content_id.clone()),
            path: Path::new(&self.blob_path).to_path_buf(),
        }
    }
}

pub struct EmailIndex {
    mail_root: std::path::PathBuf,
    conn: Connection,
}

impl EmailIndex {
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        Storage::open(mail_root)?;
        let legacy_path = mail_root.join(".vivarium").join("index.sqlite");
        if legacy_path.exists() {
            fs::remove_file(&legacy_path).map_err(|e| {
                VivariumError::Other(format!(
                    "failed to remove legacy index.sqlite at {}: {e}",
                    legacy_path.display()
                ))
            })?;
        }
        let path = mail_root.join(".vivarium").join("storage.sqlite");
        let conn = Connection::open(&path)
            .map_err(|e| VivariumError::Other(format!("failed to open email index: {e}")))?;
        let index = Self {
            mail_root: mail_root.to_path_buf(),
            conn,
        };
        schema::ensure_schema(&index.conn)?;
        Ok(index)
    }

    pub fn rebuild(mail_root: &Path, account: &str) -> Result<IndexStats, VivariumError> {
        rebuild::rebuild(mail_root, account)
    }

    pub fn message(
        &self,
        account: &str,
        message_id: &str,
    ) -> Result<Option<IndexedMessage>, VivariumError> {
        let mut message = self
            .conn
            .query_row(
                &indexed_message_query("WHERE im.account = ?1 AND im.message_id = ?2"),
                params![account, message_id],
                |row| raw_indexed_message_from_row(row, &self.mail_root),
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read indexed message: {e}")))?;
        if let Some(message) = &mut message {
            message.handle = Storage::open(&self.mail_root)?.display_handle(&message.message_id)?;
        }
        Ok(message)
    }

    pub fn thread_messages(
        &self,
        account: &str,
        seed_message_id: &str,
        _limit: usize,
    ) -> Result<Vec<IndexedMessage>, VivariumError> {
        let Some(seed) = self.message(account, seed_message_id)? else {
            return Err(VivariumError::Message(format!(
                "message not found in index: {seed_message_id}"
            )));
        };
        let message_ids = self.thread_message_ids(account, seed_message_id, &seed)?;
        self.messages_for_message_ids(account, message_ids)
    }

    pub fn count_messages(&self, account: &str) -> Result<usize, VivariumError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM indexed_messages WHERE account = ?1",
                params![account],
                |row| row.get::<_, usize>(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to count index rows: {e}")))
    }

    pub fn list_messages(&self, account: &str) -> Result<Vec<IndexedMessage>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "{} ORDER BY md.date, im.message_id",
                indexed_message_query("WHERE im.account = ?1")
            ))
            .map_err(|e| VivariumError::Other(format!("failed to prepare index listing: {e}")))?;
        let rows = stmt
            .query_map(params![account], |row| {
                raw_indexed_message_from_row(row, &self.mail_root)
            })
            .map_err(|e| VivariumError::Other(format!("failed to list index rows: {e}")))?;
        let mut messages: Vec<_> = rows
            .map(|row| {
                row.map_err(|e| VivariumError::Other(format!("failed to read indexed row: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let storage = Storage::open(&self.mail_root)?;
        let handle_map = storage.handle_map()?;
        for message in &mut messages {
            message.handle = handle_map
                .get(&message.message_id)
                .cloned()
                .unwrap_or_else(|| message.message_id.clone());
        }
        Ok(messages)
    }

    pub fn search_messages(
        &self,
        account: &str,
        query: &str,
        limit: usize,
        offset: usize,
        folder: Option<&str>,
    ) -> Result<(Vec<(IndexedMessage, f64)>, usize), VivariumError> {
        let Some(fts_query) = fts_query(query) else {
            return Ok((Vec::new(), 0));
        };
        let total = self.count_search_matches(account, &fts_query, folder)?;
        let rows = self.page_search_matches(account, &fts_query, limit, offset, folder)?;
        Ok((rows, total))
    }

    fn count_search_matches(
        &self,
        account: &str,
        fts_query: &str,
        folder: Option<&str>,
    ) -> Result<usize, VivariumError> {
        let count = if let Some(folder) = folder {
            self.conn.query_row(
                "SELECT COUNT(*)
                 FROM message_search_fts
                 JOIN messages m
                   ON m.account = message_search_fts.account
                  AND m.message_id = message_search_fts.message_id
                 WHERE message_search_fts.account = ?1
                   AND message_search_fts MATCH ?2
                   AND m.local_role = ?3
                   AND m.deleted_at IS NULL",
                params![account, fts_query, folder],
                |row| row.get::<_, usize>(0),
            )
        } else {
            self.conn.query_row(
                "SELECT COUNT(*)
                 FROM message_search_fts
                 JOIN messages m
                   ON m.account = message_search_fts.account
                  AND m.message_id = message_search_fts.message_id
                 WHERE message_search_fts.account = ?1
                   AND message_search_fts MATCH ?2
                   AND m.deleted_at IS NULL",
                params![account, fts_query],
                |row| row.get::<_, usize>(0),
            )
        };
        count.map_err(|e| VivariumError::Other(format!("failed to count search matches: {e}")))
    }

    fn page_search_matches(
        &self,
        account: &str,
        fts_query: &str,
        limit: usize,
        offset: usize,
        folder: Option<&str>,
    ) -> Result<Vec<(IndexedMessage, f64)>, VivariumError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let mut rows = if let Some(folder) = folder {
            let mut stmt = self
                .conn
                .prepare(&format!(
                    "{}
                     WHERE message_search_fts.account = ?1
                       AND message_search_fts MATCH ?2
                       AND m.local_role = ?3
                       AND m.deleted_at IS NULL
                     ORDER BY rank, md.date DESC, m.message_id
                     LIMIT ?4 OFFSET ?5",
                    indexed_search_query()
                ))
                .map_err(|e| {
                    VivariumError::Other(format!("failed to prepare search query: {e}"))
                })?;
            stmt.query_map(params![account, fts_query, folder, limit, offset], |row| {
                let message = raw_indexed_message_from_row(row, &self.mail_root)?;
                let rank = row.get::<_, f64>(12)?;
                Ok((message, 0.0 - rank))
            })
            .map_err(|e| VivariumError::Other(format!("failed to query search rows: {e}")))?
            .collect::<rusqlite::Result<Vec<_>>>()
        } else {
            let mut stmt = self
                .conn
                .prepare(&format!(
                    "{}
                     WHERE message_search_fts.account = ?1
                       AND message_search_fts MATCH ?2
                       AND m.deleted_at IS NULL
                     ORDER BY rank, md.date DESC, m.message_id
                     LIMIT ?3 OFFSET ?4",
                    indexed_search_query()
                ))
                .map_err(|e| {
                    VivariumError::Other(format!("failed to prepare search query: {e}"))
                })?;
            stmt.query_map(params![account, fts_query, limit, offset], |row| {
                let message = raw_indexed_message_from_row(row, &self.mail_root)?;
                let rank = row.get::<_, f64>(12)?;
                Ok((message, 0.0 - rank))
            })
            .map_err(|e| VivariumError::Other(format!("failed to query search rows: {e}")))?
            .collect::<rusqlite::Result<Vec<_>>>()
        }
        .map_err(|e| VivariumError::Other(format!("failed to read search row: {e}")))?;
        let storage = Storage::open(&self.mail_root)?;
        let handle_map = storage.handle_map()?;
        for (message, _) in &mut rows {
            message.handle = handle_map
                .get(&message.message_id)
                .cloned()
                .unwrap_or_else(|| message.message_id.clone());
        }
        Ok(rows)
    }

    fn thread_message_ids(
        &self,
        account: &str,
        seed_message_id: &str,
        seed: &IndexedMessage,
    ) -> Result<BTreeSet<String>, VivariumError> {
        let mut thread_ids = self.related_ids(account, seed_message_id)?;
        if let Some(message_id) = &seed.rfc_message_id {
            thread_ids.insert(message_id.clone());
        }
        let mut message_ids = BTreeSet::from([seed_message_id.to_string()]);
        let mut queue: VecDeque<String> = thread_ids.iter().cloned().collect();
        let mut seen_ids = thread_ids;

        while let Some(message_id) = queue.pop_front() {
            if seen_ids.len() > THREAD_WALK_LIMIT || message_ids.len() > THREAD_WALK_LIMIT {
                break;
            }
            self.expand_thread_id(
                account,
                &message_id,
                &mut message_ids,
                &mut seen_ids,
                &mut queue,
            )?;
        }
        Ok(message_ids)
    }

    fn expand_thread_id(
        &self,
        account: &str,
        message_id: &str,
        message_ids: &mut BTreeSet<String>,
        seen_ids: &mut BTreeSet<String>,
        queue: &mut VecDeque<String>,
    ) -> Result<(), VivariumError> {
        for indexed_message_id in self.message_ids_linking_to(account, message_id)? {
            if !message_ids.insert(indexed_message_id.clone()) {
                continue;
            }
            for related_id in self.related_ids(account, &indexed_message_id)? {
                if seen_ids.insert(related_id.clone()) {
                    queue.push_back(related_id);
                }
            }
        }
        Ok(())
    }

    fn messages_for_message_ids(
        &self,
        account: &str,
        message_ids: BTreeSet<String>,
    ) -> Result<Vec<IndexedMessage>, VivariumError> {
        let mut messages = Vec::new();
        for message_id in message_ids {
            if let Some(message) = self.message(account, &message_id)? {
                messages.push(message);
            }
        }
        messages.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.message_id.cmp(&b.message_id))
        });
        Ok(messages)
    }

    fn related_ids(
        &self,
        account: &str,
        message_id: &str,
    ) -> Result<BTreeSet<String>, VivariumError> {
        let mut ids = BTreeSet::new();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT normalized_message_id FROM message_links
                 WHERE account = ?1 AND message_id = ?2",
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to prepare related id query: {e}"))
            })?;
        let rows = stmt
            .query_map(params![account, message_id], |row| row.get::<_, String>(0))
            .map_err(|e| VivariumError::Other(format!("failed to query related ids: {e}")))?;
        for row in rows {
            ids.insert(
                row.map_err(|e| VivariumError::Other(format!("failed to read related id: {e}")))?,
            );
        }
        Ok(ids)
    }

    fn message_ids_linking_to(
        &self,
        account: &str,
        message_id: &str,
    ) -> Result<Vec<String>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT message_id FROM message_links
             WHERE account = ?1 AND normalized_message_id = ?2",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare thread query: {e}")))?;
        let rows = stmt
            .query_map(params![account, message_id], |row| row.get::<_, String>(0))
            .map_err(|e| VivariumError::Other(format!("failed to query thread handles: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read thread handle: {e}")))
        })
        .collect()
    }
}

fn fts_query(query: &str) -> Option<String> {
    let terms = query
        .split_whitespace()
        .flat_map(|term| term.split(|ch: char| !ch.is_alphanumeric()))
        .filter_map(fts_term)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

fn fts_term(term: &str) -> Option<String> {
    let normalized = term.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(format!("{normalized}*"))
    }
}

fn raw_indexed_message_from_row(
    row: &rusqlite::Row<'_>,
    mail_root: &Path,
) -> rusqlite::Result<IndexedMessage> {
    Ok(IndexedMessage {
        handle: row.get::<_, String>(1)?,
        account: row.get(0)?,
        message_id: row.get(1)?,
        content_id: row.get(2)?,
        blob_path: mail_root
            .join(row.get::<_, String>(3)?)
            .to_string_lossy()
            .to_string(),
        local_role: row.get(4)?,
        date: row.get(5)?,
        from_addr: row.get(6)?,
        to_addr: row.get(7)?,
        cc_addr: row.get(8)?,
        bcc_addr: row.get(9)?,
        subject: row.get(10)?,
        rfc_message_id: row.get(11)?,
    })
}

fn indexed_message_query(where_clause: &str) -> String {
    format!(
        "SELECT
            m.account,
            m.message_id,
            m.content_id,
            b.blob_relpath,
            m.local_role,
            md.date,
            md.from_addr,
            md.to_addr,
            md.cc_addr,
            md.bcc_addr,
            md.subject,
            md.normalized_message_id
         FROM indexed_messages im
         JOIN messages m ON m.message_id = im.message_id
         JOIN blobs b ON b.content_id = m.content_id
         JOIN message_metadata md ON md.content_id = m.content_id
         {where_clause}"
    )
}

fn indexed_search_query() -> &'static str {
    "SELECT
        m.account,
        m.message_id,
        m.content_id,
        b.blob_relpath,
        m.local_role,
        md.date,
        md.from_addr,
        md.to_addr,
        md.cc_addr,
        md.bcc_addr,
        md.subject,
        md.normalized_message_id,
        bm25(message_search_fts) AS rank
     FROM message_search_fts
     JOIN indexed_messages im
       ON im.account = message_search_fts.account
      AND im.message_id = message_search_fts.message_id
     JOIN messages m
       ON m.account = im.account
      AND m.message_id = im.message_id
     JOIN blobs b ON b.content_id = m.content_id
     JOIN message_metadata md ON md.content_id = m.content_id"
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
