use super::*;

impl Storage {
    pub fn append_mailspace_event(
        &self,
        event: &MailspaceEventInput,
    ) -> Result<i64, VivariumError> {
        let occurred_at = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO mailspace_events (
                   occurred_at, command, event_type, actor_identity, account,
                   message_id, content_id, from_role, to_role, from_identity,
                   to_identity, subject, note
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    occurred_at,
                    event.command,
                    event.event_type,
                    event.actor_identity,
                    event.account,
                    event.message_id,
                    event.content_id,
                    event.from_role,
                    event.to_role,
                    event.from_identity,
                    event.to_identity,
                    event.subject,
                    event.note,
                ],
            )
            .map_err(|e| VivariumError::Other(format!("failed to append mailspace event: {e}")))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_mailspace_events(
        &self,
        message_id: &str,
    ) -> Result<Vec<MailspaceEvent>, VivariumError> {
        let resolved = self.resolve_message_token(message_id)?;
        let mut stmt = self
            .conn
            .prepare(
                "SELECT event_id, occurred_at, command, event_type, actor_identity,
                        account, message_id, content_id, from_role, to_role,
                        from_identity, to_identity, subject, note
                 FROM mailspace_events
                 WHERE message_id = ?1
                 ORDER BY occurred_at, event_id",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare event query: {e}")))?;
        let rows = stmt
            .query_map(params![resolved], mailspace_event_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to query events: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read event row: {e}")))
        })
        .collect()
    }

    pub fn list_mailspace_events_after(
        &self,
        event_id: i64,
    ) -> Result<Vec<MailspaceEvent>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT event_id, occurred_at, command, event_type, actor_identity,
                        account, message_id, content_id, from_role, to_role,
                        from_identity, to_identity, subject, note
                 FROM mailspace_events
                 WHERE event_id > ?1
                 ORDER BY event_id",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare event scan: {e}")))?;
        let rows = stmt
            .query_map(params![event_id], mailspace_event_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to scan mailspace events: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read event row: {e}")))
        })
        .collect()
    }

    pub fn event_cursor_before(&self, occurred_at: &str) -> Result<i64, VivariumError> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(event_id), 0)
                 FROM mailspace_events WHERE occurred_at < ?1",
                params![occurred_at],
                |row| row.get(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to read event cursor: {e}")))
    }
}

fn mailspace_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MailspaceEvent> {
    Ok(MailspaceEvent {
        event_id: row.get(0)?,
        occurred_at: row.get(1)?,
        command: row.get(2)?,
        event_type: row.get(3)?,
        actor_identity: row.get(4)?,
        account: row.get(5)?,
        message_id: row.get(6)?,
        content_id: row.get(7)?,
        from_role: row.get(8)?,
        to_role: row.get(9)?,
        from_identity: row.get(10)?,
        to_identity: row.get(11)?,
        subject: row.get(12)?,
        note: row.get(13)?,
    })
}
