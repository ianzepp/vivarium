use std::collections::BTreeMap;

use super::*;

impl Storage {
    pub fn set_item_metadata(
        &self,
        message_id: &str,
        metadata: &BTreeMap<String, String>,
    ) -> Result<(), VivariumError> {
        let resolved = self.resolve_message_token(message_id)?;
        let now = Utc::now().to_rfc3339();
        for (key, value) in metadata {
            self.conn
                .execute(
                    "INSERT INTO mailspace_item_metadata
                       (message_id, key, value, updated_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(message_id, key) DO UPDATE SET
                       value = excluded.value,
                       updated_at = excluded.updated_at",
                    params![resolved, key, value, now],
                )
                .map_err(|e| VivariumError::Other(format!("failed to set item metadata: {e}")))?;
        }
        Ok(())
    }

    pub fn item_metadata(
        &self,
        message_id: &str,
    ) -> Result<BTreeMap<String, String>, VivariumError> {
        let resolved = self.resolve_message_token(message_id)?;
        let mut stmt = self
            .conn
            .prepare(
                "SELECT key, value FROM mailspace_item_metadata
                 WHERE message_id = ?1 ORDER BY key",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare metadata query: {e}")))?;
        let rows = stmt
            .query_map(params![resolved], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| VivariumError::Other(format!("failed to query metadata: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read metadata row: {e}")))
        })
        .collect()
    }
}
