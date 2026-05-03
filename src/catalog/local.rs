use std::path::Path;

use rusqlite::{OptionalExtension, params};

use super::sqlite::catalog_entry_from_row;
use super::{Catalog, CatalogEntry, RemoteIdentity};
use crate::error::VivariumError;

impl Catalog {
    pub fn entry(&self, account: &str, handle: &str) -> Option<CatalogEntry> {
        self.conn
            .query_row(
                "SELECT handle, raw_path, fingerprint, account, folder, maildir_subdir,
                        date, from_addr, to_addr, cc_addr, bcc_addr, subject,
                        rfc_message_id, remote_json, is_duplicate
                 FROM catalog_entries
                 WHERE account = ?1 AND handle = ?2",
                params![account, handle],
                catalog_entry_from_row,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn entry_by_message_id(&self, account: &str, message_id: &str) -> Option<CatalogEntry> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT handle, raw_path, fingerprint, account, folder, maildir_subdir,
                        date, from_addr, to_addr, cc_addr, bcc_addr, subject,
                        rfc_message_id, remote_json, is_duplicate
                 FROM catalog_entries
                 WHERE account = ?1",
            )
            .ok()?;
        let rows = stmt
            .query_map(params![account], catalog_entry_from_row)
            .ok()?;
        for row in rows {
            let entry = row.ok()?;
            if crate::store::message_id_from_path(Path::new(&entry.raw_path)).as_deref()
                == Some(message_id)
            {
                return Some(entry);
            }
        }
        None
    }

    pub fn resolve_entry(&self, account: &str, handle_or_id: &str) -> Option<CatalogEntry> {
        self.entry(account, handle_or_id)
            .or_else(|| self.entry_by_message_id(account, handle_or_id))
    }

    pub fn update_local_location(
        &mut self,
        account: &str,
        handle: &str,
        raw_path: String,
        folder: String,
        maildir_subdir: String,
        remote: Option<RemoteIdentity>,
    ) -> Result<(), VivariumError> {
        let remote_json = remote
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| {
                VivariumError::Other(format!("failed to serialize remote identity: {e}"))
            })?;
        let changes = self
            .conn
            .execute(
                "UPDATE catalog_entries
                 SET raw_path = ?3,
                     folder = ?4,
                     maildir_subdir = ?5,
                     remote_json = ?6
                 WHERE account = ?1 AND handle = ?2",
                params![
                    account,
                    handle,
                    raw_path,
                    folder,
                    maildir_subdir,
                    remote_json
                ],
            )
            .map_err(|e| VivariumError::Other(format!("failed to update catalog row: {e}")))?;
        if changes == 0 {
            return Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            )));
        }
        self.flush()
    }

    pub fn remove_entry(&mut self, account: &str, handle: &str) -> Result<(), VivariumError> {
        let changes = self
            .conn
            .execute(
                "DELETE FROM catalog_entries WHERE account = ?1 AND handle = ?2",
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
