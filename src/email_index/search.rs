use rusqlite::params;

use super::{EmailIndex, IndexedMessage, indexed_search_query, raw_indexed_message_from_row};
use crate::error::VivariumError;
use crate::storage::Storage;

impl EmailIndex {
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
        let count = match folder {
            Some(folder) => count_matches_in_folder(self, account, fts_query, folder),
            None => count_matches(self, account, fts_query),
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
        let mut rows = match folder {
            Some(folder) => self.query_folder_matches(account, fts_query, folder, limit, offset),
            None => self.query_matches(account, fts_query, limit, offset),
        }?;
        self.attach_handles(&mut rows)?;
        Ok(rows)
    }

    fn query_matches(
        &self,
        account: &str,
        fts_query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(IndexedMessage, f64)>, VivariumError> {
        let sql = format!(
            "{}
             WHERE message_search_fts.account = ?1
               AND message_search_fts MATCH ?2
               AND m.deleted_at IS NULL
             ORDER BY rank, md.date DESC, m.message_id
             LIMIT ?3 OFFSET ?4",
            indexed_search_query()
        );
        let mut stmt = self.prepare_search(&sql)?;
        stmt.query_map(params![account, fts_query, limit, offset], |row| {
            indexed_message_with_score(row, &self.mail_root)
        })
        .map_err(|e| VivariumError::Other(format!("failed to query search rows: {e}")))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| VivariumError::Other(format!("failed to read search row: {e}")))
    }

    fn query_folder_matches(
        &self,
        account: &str,
        fts_query: &str,
        folder: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(IndexedMessage, f64)>, VivariumError> {
        let sql = format!(
            "{}
             WHERE message_search_fts.account = ?1
               AND message_search_fts MATCH ?2
               AND m.local_role = ?3
               AND m.deleted_at IS NULL
             ORDER BY rank, md.date DESC, m.message_id
             LIMIT ?4 OFFSET ?5",
            indexed_search_query()
        );
        let mut stmt = self.prepare_search(&sql)?;
        stmt.query_map(params![account, fts_query, folder, limit, offset], |row| {
            indexed_message_with_score(row, &self.mail_root)
        })
        .map_err(|e| VivariumError::Other(format!("failed to query search rows: {e}")))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| VivariumError::Other(format!("failed to read search row: {e}")))
    }

    fn prepare_search(&self, sql: &str) -> Result<rusqlite::Statement<'_>, VivariumError> {
        self.conn
            .prepare(sql)
            .map_err(|e| VivariumError::Other(format!("failed to prepare search query: {e}")))
    }

    fn attach_handles(&self, rows: &mut [(IndexedMessage, f64)]) -> Result<(), VivariumError> {
        let storage = Storage::open(&self.mail_root)?;
        let handle_map = storage.handle_map()?;
        for (message, _) in rows {
            message.handle = handle_map
                .get(&message.message_id)
                .cloned()
                .unwrap_or_else(|| message.message_id.clone());
        }
        Ok(())
    }
}

fn count_matches(index: &EmailIndex, account: &str, fts_query: &str) -> rusqlite::Result<usize> {
    index.conn.query_row(
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
}

fn count_matches_in_folder(
    index: &EmailIndex,
    account: &str,
    fts_query: &str,
    folder: &str,
) -> rusqlite::Result<usize> {
    index.conn.query_row(
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
}

fn indexed_message_with_score(
    row: &rusqlite::Row<'_>,
    mail_root: &std::path::Path,
) -> rusqlite::Result<(IndexedMessage, f64)> {
    let message = raw_indexed_message_from_row(row, mail_root)?;
    let rank = row.get::<_, f64>(12)?;
    Ok((message, 0.0 - rank))
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
