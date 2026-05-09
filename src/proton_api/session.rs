use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{LoginCheck, TwoFaInfo};
use crate::error::VivariumError;
use crate::store::secure_create_dir_all;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtonSession {
    pub uid: String,
    pub access_token: String,
    pub refresh_token: String,
    pub app_version: String,
    pub user_id: String,
    pub scope: String,
    pub password_mode: u8,
    pub two_fa: TwoFaInfo,
    pub updated_at: String,
}

impl ProtonSession {
    pub fn check(&self) -> LoginCheck {
        LoginCheck {
            uid_present: !self.uid.is_empty(),
            user_id_present: !self.user_id.is_empty(),
            scope: self.scope.clone(),
            password_mode: self.password_mode,
            two_fa: self.two_fa.clone(),
            app_version: self.app_version.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    pub(super) fn preserve_metadata_from(&mut self, previous: &ProtonSession) {
        if self.user_id.is_empty() {
            self.user_id.clone_from(&previous.user_id);
        }
        if self.password_mode == 0 {
            self.password_mode = previous.password_mode;
        }
        if self.two_fa.enabled == 0 {
            self.two_fa = previous.two_fa.clone();
        }
        if self.scope.is_empty() {
            self.scope.clone_from(&previous.scope);
        }
    }
}

pub struct ProtonSessionStore {
    path: PathBuf,
}

impl ProtonSessionStore {
    pub fn new(mail_root: &Path) -> Self {
        Self {
            path: mail_root.join(".vivarium").join("proton-session.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self, session: &ProtonSession) -> Result<(), VivariumError> {
        let Some(parent) = self.path.parent() else {
            return Err(VivariumError::Other(
                "Proton session path has no parent".into(),
            ));
        };
        secure_create_dir_all(parent)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&self.path)?;
        let json = serde_json::to_string_pretty(session).map_err(|e| {
            VivariumError::Other(format!("Proton session serialization failed: {e}"))
        })?;
        file.write_all(json.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        #[cfg(unix)]
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    pub fn load(&self) -> Result<ProtonSession, VivariumError> {
        let data = fs::read_to_string(&self.path).map_err(|e| {
            VivariumError::Config(format!(
                "no direct Proton API session found at {}; run `vivi proton login` first: {e}",
                self.path.display()
            ))
        })?;
        serde_json::from_str(&data)
            .map_err(|e| VivariumError::Parse(format!("invalid Proton session file: {e}")))
    }
}
