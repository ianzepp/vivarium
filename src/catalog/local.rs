use std::path::Path;

use super::{Catalog, CatalogEntry, RemoteIdentity};
use crate::error::VivariumError;

impl Catalog {
    pub fn entry(&self, account: &str, handle: &str) -> Option<CatalogEntry> {
        self.entries
            .get(handle)
            .filter(|entry| entry.account == account)
            .cloned()
    }

    pub fn entry_by_message_id(&self, account: &str, message_id: &str) -> Option<CatalogEntry> {
        self.entries
            .values()
            .find(|entry| {
                entry.account == account
                    && crate::store::message_id_from_path(Path::new(&entry.raw_path)).as_deref()
                        == Some(message_id)
            })
            .cloned()
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
        let Some(entry) = self
            .entries
            .get_mut(handle)
            .filter(|entry| entry.account == account)
        else {
            return Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            )));
        };
        entry.raw_path = raw_path;
        entry.folder = folder;
        entry.maildir_subdir = maildir_subdir;
        entry.remote = remote;
        self.flush()
    }

    pub fn remove_entry(&mut self, account: &str, handle: &str) -> Result<(), VivariumError> {
        let Some(entry) = self
            .entries
            .get(handle)
            .filter(|entry| entry.account == account)
        else {
            return Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            )));
        };
        let handle = entry.handle.clone();
        self.entries.remove(&handle);
        self.flush()
    }
}
