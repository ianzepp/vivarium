use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MailspaceLink {
    pub child_content_id: String,
    pub parent_content_id: String,
    pub source: String,
}

impl Storage {
    pub fn link_mailspace_content(
        &self,
        child_content_id: &str,
        parent_content_id: &str,
        source: &str,
    ) -> Result<(), VivariumError> {
        if child_content_id == parent_content_id {
            return Err(VivariumError::Message(
                "a mailspace message cannot reply to itself".into(),
            ));
        }
        if !matches!(source, "captured" | "inferred" | "source") {
            return Err(VivariumError::Message(format!(
                "unsupported mailspace link source '{source}'"
            )));
        }
        self.conn
            .execute(
                "INSERT INTO mailspace_links (child_content_id, parent_content_id, source)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(child_content_id) DO UPDATE SET
                   parent_content_id = CASE
                     WHEN mailspace_links.source = 'captured' AND excluded.source = 'inferred'
                       THEN mailspace_links.parent_content_id
                     ELSE excluded.parent_content_id
                   END,
                   source = CASE
                     WHEN mailspace_links.source = 'captured' AND excluded.source = 'inferred'
                       THEN mailspace_links.source
                     ELSE excluded.source
                   END",
                params![child_content_id, parent_content_id, source],
            )
            .map_err(|e| VivariumError::Other(format!("failed to store mailspace link: {e}")))?;
        Ok(())
    }

    pub fn mailspace_link_for_child(
        &self,
        child_content_id: &str,
    ) -> Result<Option<MailspaceLink>, VivariumError> {
        self.conn
            .query_row(
                "SELECT child_content_id, parent_content_id, source
                 FROM mailspace_links WHERE child_content_id = ?1",
                params![child_content_id],
                mailspace_link_from_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read mailspace link: {e}")))
    }

    pub fn list_mailspace_links(&self) -> Result<Vec<MailspaceLink>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT child_content_id, parent_content_id, source
                 FROM mailspace_links ORDER BY child_content_id",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare link query: {e}")))?;
        let rows = stmt
            .query_map([], mailspace_link_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to query mailspace links: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read mailspace link row: {e}")))
        })
        .collect()
    }

    pub fn message_by_content_id(
        &self,
        content_id: &str,
    ) -> Result<Option<StoredMessageView>, VivariumError> {
        let mut message = self
            .conn
            .query_row(
                &message_query("WHERE m.content_id = ?1 AND m.deleted_at IS NULL ORDER BY m.message_id LIMIT 1"),
                params![content_id],
                raw_stored_message_from_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read content message: {e}")))?;
        if let Some(message) = &mut message {
            message.handle = self.display_handle(&message.message_id)?;
        }
        Ok(message)
    }
}

fn mailspace_link_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MailspaceLink> {
    Ok(MailspaceLink {
        child_content_id: row.get(0)?,
        parent_content_id: row.get(1)?,
        source: row.get(2)?,
    })
}
