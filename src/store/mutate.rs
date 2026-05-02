use std::fs;
use std::path::{Path, PathBuf};

use super::path::maildir_filename_with_flags;
use super::{MailStore, resolve_folder};
use crate::error::VivariumError;

impl MailStore {
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

    pub fn set_message_flag(
        &self,
        message_id: &str,
        folder: &str,
        flag: char,
        enabled: bool,
    ) -> Result<PathBuf, VivariumError> {
        let folder = resolve_folder(folder)?;
        let src = self
            .find_message_in_subdirs(message_id, folder, &["new", "cur"])?
            .ok_or_else(|| {
                VivariumError::Message(format!("message not found in {folder}: {message_id}"))
            })?;
        let mut flags = maildir_flags(&src);
        flags.retain(|value| *value != flag);
        if enabled {
            flags.push(flag);
        }
        let (filename, subdir) = maildir_filename_with_flags(message_id, &flags);
        let dst = self.folder_path(folder).join(subdir).join(filename);
        if src != dst {
            fs::rename(&src, &dst)?;
        }
        Ok(dst)
    }
}

fn maildir_flags(path: &Path) -> Vec<char> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once(":2,").map(|(_, flags)| flags))
        .map(|flags| flags.chars().collect())
        .unwrap_or_default()
}
