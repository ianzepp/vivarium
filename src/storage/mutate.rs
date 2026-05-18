use super::*;

impl Storage {
    pub fn move_message_to_role(
        &mut self,
        account: &str,
        message_id: &str,
        local_role: &str,
    ) -> Result<(), VivariumError> {
        let resolved = self.resolve_message_token(message_id)?;
        let now = Utc::now().to_rfc3339();
        let changed = self
            .conn
            .execute(
                "UPDATE messages
                 SET local_role = ?3, updated_at = ?4
                 WHERE account = ?1 AND message_id = ?2 AND deleted_at IS NULL",
                params![account, resolved, local_role, now],
            )
            .map_err(|e| VivariumError::Other(format!("failed to move message: {e}")))?;
        if changed == 0 {
            return Err(VivariumError::Message(format!(
                "message not found for {account}: {message_id}"
            )));
        }
        Ok(())
    }

    pub fn update_remote_flags(
        &mut self,
        account: &str,
        remote_mailbox: &str,
        remote_uidvalidity: u32,
        remote_uid: u32,
        read_state: bool,
        starred: bool,
    ) -> Result<bool, VivariumError> {
        let now = Utc::now().to_rfc3339();
        let changed = self
            .conn
            .execute(
                "UPDATE messages
                 SET read_state = ?5, starred = ?6, updated_at = ?7
                 WHERE deleted_at IS NULL
                   AND message_id = (
                       SELECT message_id FROM remote_bindings
                       WHERE account = ?1
                         AND remote_mailbox = ?2
                         AND remote_uidvalidity = ?3
                         AND remote_uid = ?4
                   )",
                params![
                    account,
                    remote_mailbox,
                    remote_uidvalidity,
                    remote_uid,
                    if read_state { 1 } else { 0 },
                    if starred { 1 } else { 0 },
                    now
                ],
            )
            .map_err(|e| VivariumError::Other(format!("failed to update message flags: {e}")))?;
        Ok(changed > 0)
    }

    pub fn mark_message_deleted(
        &mut self,
        account: &str,
        message_id: &str,
    ) -> Result<bool, VivariumError> {
        let now = Utc::now().to_rfc3339();
        let changed = self
            .conn
            .execute(
                "UPDATE messages
                 SET deleted_at = ?3, updated_at = ?3
                 WHERE account = ?1 AND message_id = ?2 AND deleted_at IS NULL",
                params![account, message_id, now],
            )
            .map_err(|e| VivariumError::Other(format!("failed to mark message deleted: {e}")))?;
        Ok(changed > 0)
    }
}
