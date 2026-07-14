use std::collections::HashMap;

use super::*;

impl Storage {
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

    pub fn blob_exists(&self, content_id: &str) -> Result<bool, VivariumError> {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM blobs WHERE content_id = ?1)",
                params![content_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists != 0)
            .map_err(|e| VivariumError::Other(format!("failed to check blob row: {e}")))
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

    pub fn message_by_content_account_role(
        &self,
        content_id: &str,
        account: &str,
        local_role: &str,
    ) -> Result<Option<StoredMessageView>, VivariumError> {
        let mut message = self
            .conn
            .query_row(
                &message_query(
                    "WHERE m.content_id = ?1
                       AND m.account = ?2
                       AND m.local_role = ?3
                       AND m.deleted_at IS NULL
                     ORDER BY m.message_id LIMIT 1",
                ),
                params![content_id, account, local_role],
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
        self.list_messages_by_query("WHERE m.local_role = ?1 AND m.deleted_at IS NULL", params![local_role])
    }

    /// List messages filtered by account and a single role.
    pub fn list_messages_by_account_role(
        &self,
        account: &str,
        local_role: &str,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        self.list_messages_by_query(
            "WHERE m.account = ?1 AND m.local_role = ?2 AND m.deleted_at IS NULL",
            params![account, local_role],
        )
    }

    /// List messages filtered by one or more accounts and one or more roles.
    pub fn list_messages_by_account_roles(
        &self,
        accounts: &[String],
        roles: &[String],
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        if accounts.is_empty() || roles.is_empty() {
            return Ok(Vec::new());
        }
        let account_placeholders: Vec<_> = accounts.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let role_offset = accounts.len();
        let role_placeholders: Vec<_> = roles.iter().enumerate().map(|(i, _)| format!("?{}", role_offset + i + 1)).collect();
        let sql = format!(
            "{} WHERE m.account IN ({}) AND m.local_role IN ({}) AND m.deleted_at IS NULL ORDER BY md.date DESC, m.message_id",
            message_query(""),
            account_placeholders.join(","),
            role_placeholders.join(","),
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| {
            VivariumError::Other(format!("failed to prepare account+role listing: {e}"))
        })?;
        let params: Vec<&dyn rusqlite::types::ToSql> = accounts
            .iter()
            .map(|a| a as &dyn rusqlite::types::ToSql)
            .chain(roles.iter().map(|r| r as &dyn rusqlite::types::ToSql))
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), raw_stored_message_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to list account+role messages: {e}")))?;
        let messages: Result<Vec<_>, _> = rows
            .map(|row| {
                row.map_err(|e| VivariumError::Other(format!("failed to read message row: {e}")))
            })
            .collect();
        self.decorate_handles(messages?)
    }

    fn list_messages_by_query(
        &self,
        where_clause: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let sql = format!(
            "{} ORDER BY md.date DESC, m.message_id",
            message_query(where_clause)
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| {
            VivariumError::Other(format!("failed to prepare storage listing: {e}"))
        })?;
        let rows = stmt
            .query_map(params, raw_stored_message_from_row)
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

    pub fn list_messages(&self) -> Result<Vec<StoredMessageView>, VivariumError> {
        self.list_messages_by_query("WHERE m.deleted_at IS NULL", [])
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

    pub fn count_messages_for_account_role(
        &self,
        account: &str,
        local_role: &str,
        read_state: Option<bool>,
    ) -> Result<usize, VivariumError> {
        let read_clause = if read_state.is_some() {
            " AND read_state = ?3"
        } else {
            ""
        };
        let sql = format!(
            "SELECT COUNT(*) FROM messages
             WHERE account = ?1 AND local_role = ?2 AND deleted_at IS NULL{read_clause}"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| {
            VivariumError::Other(format!("failed to prepare message count query: {e}"))
        })?;
        let count = if let Some(read_state) = read_state {
            stmt.query_row(params![account, local_role, i64::from(read_state)], |row| {
                row.get(0)
            })
        } else {
            stmt.query_row(params![account, local_role], |row| row.get(0))
        };
        count.map_err(|e| VivariumError::Other(format!("failed to count stored messages: {e}")))
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
}
