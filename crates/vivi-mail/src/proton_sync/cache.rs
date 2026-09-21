use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::VivariumError;
use crate::proton_api::ProtonFullMessage;
use crate::store::secure_create_dir_all;

const CACHE_DIR: &str = ".vivarium/proton/raw-messages";

pub(super) struct ProtonRawMessageCache {
    root: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct CachedProtonFullMessage {
    sensitive: bool,
    cached_at: DateTime<Utc>,
    message: ProtonFullMessage,
}

impl ProtonRawMessageCache {
    pub(super) fn new(mail_root: &Path) -> Self {
        Self {
            root: mail_root.join(CACHE_DIR),
        }
    }

    pub(super) fn load(
        &self,
        proton_message_id: &str,
    ) -> Result<Option<ProtonFullMessage>, VivariumError> {
        let path = self.path_for(proton_message_id);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path)?;
        let cached: CachedProtonFullMessage = serde_json::from_str(&data).map_err(|e| {
            VivariumError::Other(format!(
                "Proton raw message cache parse failed for {}: {e}",
                path.display()
            ))
        })?;
        Ok(Some(cached.message))
    }

    pub(super) fn store(&self, message: &ProtonFullMessage) -> Result<(), VivariumError> {
        secure_create_dir_all(&self.root)?;
        let path = self.path_for(&message.metadata.id);
        let cached = CachedProtonFullMessage {
            sensitive: true,
            cached_at: Utc::now(),
            message: message.clone(),
        };
        let data = serde_json::to_vec_pretty(&cached)
            .map_err(|e| VivariumError::Other(format!("Proton cache serialization failed: {e}")))?;
        write_private_file(&path, &data)
    }

    fn path_for(&self, proton_message_id: &str) -> PathBuf {
        self.root
            .join(format!("{}.json", cache_key(proton_message_id)))
    }
}

fn cache_key(proton_message_id: &str) -> String {
    let hash = Sha256::digest(proton_message_id.as_bytes());
    hex::encode(hash)
}

fn write_private_file(path: &Path, data: &[u8]) -> Result<(), VivariumError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
