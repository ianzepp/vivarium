use super::*;

impl Storage {
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

    /// Compute a short display handle for a single message.
    ///
    /// Results are cached on `Storage` to avoid repeated full table scans of
    /// all active message IDs. The cache is invalidated on any write that
    /// affects messages (ingest, move, flag update, delete).
    pub fn display_handle(&self, message_id: &str) -> Result<String, VivariumError> {
        let mut cache = self.handle_cache.borrow_mut();
        if cache.is_none() {
            let message_ids = self.active_message_ids()?;
            *cache = Some(short_handle_map(&message_ids));
        }
        Ok(cache
            .as_ref()
            .unwrap()
            .get(message_id)
            .cloned()
            .unwrap_or_else(|| message_id.to_string()))
    }

    pub fn handle_map(&self) -> Result<HashMap<String, String>, VivariumError> {
        let message_ids = self.active_message_ids()?;
        Ok(short_handle_map(&message_ids))
    }

    #[cfg(test)]
    pub(super) fn blob_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count blobs: {e}")))
    }

    #[cfg(test)]
    pub(super) fn message_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count messages: {e}")))
    }

    #[cfg(test)]
    pub(super) fn remote_binding_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM remote_bindings", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count remote bindings: {e}")))
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

    pub(super) fn decorate_handles(
        &self,
        mut messages: Vec<StoredMessageView>,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        // Use the same cached handle map as display_handle
        let mut cache = self.handle_cache.borrow_mut();
        if cache.is_none() {
            let message_ids = self.active_message_ids()?;
            *cache = Some(short_handle_map(&message_ids));
        }
        let handle_map = cache.as_ref().unwrap();
        for message in &mut messages {
            message.handle = handle_map
                .get(&message.message_id)
                .cloned()
                .unwrap_or_else(|| message.message_id.clone());
        }
        Ok(messages)
    }
}
