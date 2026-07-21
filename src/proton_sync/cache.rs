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
mod tests {
    use super::*;
    use crate::proton_api::{ProtonAddress, ProtonMessage};

    #[test]
    fn raw_message_cache_round_trips_full_message() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ProtonRawMessageCache::new(tmp.path());
        let message = full_message("proton-id");

        cache.store(&message).unwrap();

        assert_eq!(cache.load("proton-id").unwrap(), Some(message));
    }

    #[test]
    fn raw_message_cache_missing_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ProtonRawMessageCache::new(tmp.path());

        assert_eq!(cache.load("missing").unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn raw_message_cache_uses_private_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = ProtonRawMessageCache::new(tmp.path());
        cache.store(&full_message("private-id")).unwrap();

        let path = cache.path_for("private-id");
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;

        assert_eq!(mode, 0o600);
    }

    fn full_message(id: &str) -> ProtonFullMessage {
        ProtonFullMessage {
            metadata: ProtonMessage {
                id: id.into(),
                conversation_id: "conversation-id".into(),
                external_id: "external@example.com".into(),
                subject: "hello".into(),
                time: 1_778_205_000,
                size: 123,
                flags: 4,
                unread: 0,
                num_attachments: 0,
                sender: ProtonAddress {
                    name: "Sender".into(),
                    address: "sender@example.com".into(),
                },
                to: Vec::new(),
                cc: Vec::new(),
                bcc: Vec::new(),
                label_ids: vec!["0".into()],
            },
            header: "Subject: hello\r\n\r\n".into(),
            body: "encrypted body".into(),
            mime_type: "text/plain".into(),
        }
    }
}
