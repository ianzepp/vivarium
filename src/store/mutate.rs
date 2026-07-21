use std::fs;

use super::{MailStore, resolve_folder};
use crate::error::VivariumError;

impl MailStore {
    /// Remove a message file from the store.
    ///
    /// # Errors
    /// Returns an error if the message is not found or the file cannot be
    /// removed from the filesystem.
    pub fn remove_message(&self, message_id: &str, folder: &str) -> Result<(), VivariumError> {
        let folder = resolve_folder(folder)?;
        let src = self
            .find_message_in_subdirs(message_id, folder, &["new", "cur"])?
            .ok_or_else(|| {
                VivariumError::Message(format!("message not found in {folder}: {message_id}"))
            })?;
        fs::remove_file(src)?;
        Ok(())
    }
}
