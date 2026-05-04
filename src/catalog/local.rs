use super::{Catalog, CatalogEntry, RemoteIdentity};
use crate::error::VivariumError;
use rusqlite::params;

impl Catalog {
    pub fn entry(&self, account: &str, handle: &str) -> Option<CatalogEntry> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "{} WHERE m.account = ?1 AND m.deleted_at IS NULL AND m.message_id = ?2",
                super::catalog_select_sql()
            ))
            .ok()?;
        stmt.query_row(params![account, handle], |row| {
            self.catalog_entry_from_row(row)
        })
        .ok()
    }

    pub fn entry_by_message_id(&self, account: &str, message_id: &str) -> Option<CatalogEntry> {
        self.entry(account, message_id)
    }

    pub fn resolve_entry(&self, account: &str, handle_or_id: &str) -> Option<CatalogEntry> {
        self.entry(account, handle_or_id)
            .or_else(|| self.entry_by_message_id(account, handle_or_id))
            .or_else(|| {
                let storage = crate::storage::Storage::open(&self.mail_root).ok()?;
                let resolved = storage.resolve_message_token(handle_or_id).ok()?;
                self.entry(account, &resolved)
            })
    }

    pub fn update_message_state(
        &mut self,
        account: &str,
        handle: &str,
        local_role: &str,
        read_state: bool,
        starred: bool,
        remote: Option<RemoteIdentity>,
    ) -> Result<(), VivariumError> {
        let changes = self
            .conn
            .execute(
                "UPDATE messages
                 SET local_role = ?3,
                     read_state = ?4,
                     starred = ?5,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE account = ?1 AND message_id = ?2 AND deleted_at IS NULL",
                params![
                    account,
                    handle,
                    local_role,
                    if read_state { 1 } else { 0 },
                    if starred { 1 } else { 0 }
                ],
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to update storage-backed catalog row: {e}"))
            })?;
        if changes == 0 {
            return Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            )));
        }
        if let Some(remote) = remote {
            self.conn
                .execute(
                    "INSERT INTO remote_bindings (
                       message_id, account, provider, remote_mailbox, remote_uid,
                       remote_uidvalidity, last_verified_at, stale
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP, 0)
                     ON CONFLICT(message_id) DO UPDATE SET
                       account = excluded.account,
                       provider = excluded.provider,
                       remote_mailbox = excluded.remote_mailbox,
                       remote_uid = excluded.remote_uid,
                       remote_uidvalidity = excluded.remote_uidvalidity,
                       last_verified_at = excluded.last_verified_at,
                       stale = 0",
                    params![
                        handle,
                        remote.account,
                        remote.provider,
                        remote.remote_mailbox,
                        remote.uid,
                        remote.uidvalidity,
                    ],
                )
                .map_err(|e| {
                    VivariumError::Other(format!("failed to update remote binding: {e}"))
                })?;
        } else {
            self.conn
                .execute(
                    "DELETE FROM remote_bindings WHERE message_id = ?1",
                    params![handle],
                )
                .map_err(|e| {
                    VivariumError::Other(format!("failed to clear remote binding: {e}"))
                })?;
        }
        self.flush()
    }

    pub fn update_local_location(
        &mut self,
        account: &str,
        handle: &str,
        _raw_path: String,
        folder: String,
        maildir_subdir: String,
        remote: Option<RemoteIdentity>,
    ) -> Result<(), VivariumError> {
        let local_role = super::local_role_from_folder(&folder);
        let read_state = maildir_subdir == "cur";
        let changes = self
            .conn
            .execute(
                "UPDATE messages
                 SET local_role = ?3,
                     read_state = ?4,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE account = ?1 AND message_id = ?2 AND deleted_at IS NULL",
                params![account, handle, local_role, if read_state { 1 } else { 0 }],
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to update storage-backed catalog row: {e}"))
            })?;
        if changes == 0 {
            return Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            )));
        }
        if let Some(remote) = remote {
            self.conn
                .execute(
                    "INSERT INTO remote_bindings (
                       message_id, account, provider, remote_mailbox, remote_uid,
                       remote_uidvalidity, last_verified_at, stale
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP, 0)
                     ON CONFLICT(message_id) DO UPDATE SET
                       account = excluded.account,
                       provider = excluded.provider,
                       remote_mailbox = excluded.remote_mailbox,
                       remote_uid = excluded.remote_uid,
                       remote_uidvalidity = excluded.remote_uidvalidity,
                       last_verified_at = excluded.last_verified_at,
                       stale = 0",
                    params![
                        handle,
                        remote.account,
                        remote.provider,
                        remote.remote_mailbox,
                        remote.uid,
                        remote.uidvalidity,
                    ],
                )
                .map_err(|e| {
                    VivariumError::Other(format!("failed to update remote binding: {e}"))
                })?;
        } else {
            self.conn
                .execute(
                    "DELETE FROM remote_bindings WHERE message_id = ?1",
                    params![handle],
                )
                .map_err(|e| {
                    VivariumError::Other(format!("failed to clear remote binding: {e}"))
                })?;
        }
        self.flush()
    }

    pub fn remove_entry(&mut self, account: &str, handle: &str) -> Result<(), VivariumError> {
        let changes = self
            .conn
            .execute(
                "DELETE FROM messages WHERE account = ?1 AND message_id = ?2",
                params![account, handle],
            )
            .map_err(|e| VivariumError::Other(format!("failed to remove catalog row: {e}")))?;
        if changes == 0 {
            return Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            )));
        }
        self.flush()
    }
}
