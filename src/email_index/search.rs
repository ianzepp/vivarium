use super::{EmailIndex, IndexedMessage, indexed_search_query, raw_indexed_message_from_row};
use crate::error::VivariumError;
use crate::search::SearchFilters;
use crate::storage::Storage;

impl EmailIndex {
    pub fn search_messages(
        &self,
        account: &str,
        query: &str,
        limit: usize,
        offset: usize,
        filters: Option<SearchFilters<'_>>,
    ) -> Result<(Vec<(IndexedMessage, f64)>, usize), VivariumError> {
        let Some(fts_query) = fts_query(query) else {
            return Ok((Vec::new(), 0));
        };
        let total = self.count_search_matches(account, &fts_query, filters)?;
        let rows = self.page_search_matches(account, &fts_query, limit, offset, filters)?;
        Ok((rows, total))
    }

    fn count_search_matches(
        &self,
        account: &str,
        fts_query: &str,
        filters: Option<SearchFilters<'_>>,
    ) -> Result<usize, VivariumError> {
        let count = count_matches(self, account, fts_query, filters);
        count.map_err(|e| VivariumError::Other(format!("failed to count search matches: {e}")))
    }

    fn page_search_matches(
        &self,
        account: &str,
        fts_query: &str,
        limit: usize,
        offset: usize,
        filters: Option<SearchFilters<'_>>,
    ) -> Result<Vec<(IndexedMessage, f64)>, VivariumError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let mut rows = self.query_matches(account, fts_query, filters, limit, offset)?;
        self.attach_handles(&mut rows)?;
        Ok(rows)
    }

    fn query_matches(
        &self,
        account: &str,
        fts_query: &str,
        filters: Option<SearchFilters<'_>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(IndexedMessage, f64)>, VivariumError> {
        let (where_sql, filter_values) = search_filter_sql(filters);
        let sql = format!(
            "{}{where_sql}
             ORDER BY rank, md.date DESC, m.message_id
             LIMIT ?{} OFFSET ?{}",
            indexed_search_query(),
            filter_values.len() + 3,
            filter_values.len() + 4,
        );
        let mut stmt = self.prepare_search(&sql)?;
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&account, &fts_query];
        params.extend(
            filter_values
                .iter()
                .map(|value| value as &dyn rusqlite::ToSql),
        );
        params.push(&limit);
        params.push(&offset);
        stmt.query_map(params.as_slice(), |row| {
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

fn count_matches(
    index: &EmailIndex,
    account: &str,
    fts_query: &str,
    filters: Option<SearchFilters<'_>>,
) -> rusqlite::Result<usize> {
    let (where_sql, filter_values) = search_filter_sql(filters);
    let sql = format!(
        "SELECT COUNT(*)
         FROM message_search_fts
         JOIN messages m
           ON m.account = message_search_fts.account
          AND m.message_id = message_search_fts.message_id
         JOIN message_metadata md ON md.content_id = m.content_id
         {where_sql}"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&account, &fts_query];
    params.extend(
        filter_values
            .iter()
            .map(|value| value as &dyn rusqlite::ToSql),
    );
    index
        .conn
        .query_row(&sql, params.as_slice(), |row| row.get::<_, usize>(0))
}

fn search_filter_sql(filters: Option<SearchFilters<'_>>) -> (String, Vec<String>) {
    let mut clauses = vec![
        "message_search_fts.account = ?1".to_string(),
        "message_search_fts MATCH ?2".to_string(),
        "m.deleted_at IS NULL".to_string(),
    ];
    let mut values = Vec::new();
    if let Some(filters) = filters {
        if let Some(folder) = filters.folder {
            values.push(folder.to_string());
            clauses.push(format!("m.local_role = ?{}", values.len() + 2));
        }
        if let Some(from_addr) = filters.from_addr {
            values.push(format!("%{}%", escape_like(from_addr.to_ascii_lowercase())));
            clauses.push(format!(
                "LOWER(md.from_addr) LIKE ?{} ESCAPE '\\'",
                values.len() + 2
            ));
        }
        if let Some(domain) = filters.from_domain {
            values.push(format!(
                "%@{}%",
                escape_like(domain.trim_start_matches('@').to_ascii_lowercase())
            ));
            clauses.push(format!(
                "LOWER(md.from_addr) LIKE ?{} ESCAPE '\\'",
                values.len() + 2
            ));
        }
    }
    (format!(" WHERE {}", clauses.join(" AND ")), values)
}

fn escape_like(value: String) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
