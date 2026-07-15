mod account;
pub mod types;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub use types::{
    Account, AccountsFile, Auth, Config, MutationPolicy, Provider, RenderDefaults, Security,
    StorageMode,
};

use crate::error::VivariumError;

#[cfg(test)]
mod tests;

const VIVI_HOME_ENV: &str = "VIVI_HOME";
const DEFAULT_VIVI_HOME: &str = ".vivarium";
const LEGACY_CONFIG_DIR: &[&str] = &[".config", "vivarium"];
const LEGACY_MAIL_ROOT: &[&str] = &[".local", "share", "vivarium"];

fn vivi_home_env_dir(
    vivi_home: Option<std::ffi::OsString>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    vivi_home
        .filter(|value| !value.is_empty())
        .map(|path| expand_tilde_with_home(&PathBuf::from(path).to_string_lossy(), home))
}

fn vivi_home_dir_from(vivi_home: Option<std::ffi::OsString>, home: Option<PathBuf>) -> PathBuf {
    if let Some(path) = vivi_home_env_dir(vivi_home, home.as_deref()) {
        return path;
    }

    home.map(|path| path.join(DEFAULT_VIVI_HOME))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_VIVI_HOME))
}

fn config_dir() -> PathBuf {
    let home = dirs::home_dir();
    config_dir_from(
        std::env::var_os(VIVI_HOME_ENV),
        home.clone(),
        legacy_config_exists(home.as_deref()),
    )
}

fn config_dir_from(
    vivi_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
    legacy_exists: bool,
) -> PathBuf {
    if let Some(path) = vivi_home_env_dir(vivi_home, home.as_deref()) {
        return path;
    }
    if legacy_exists && let Some(path) = legacy_path(home.as_deref(), LEGACY_CONFIG_DIR) {
        return path;
    }
    vivi_home_dir_from(None, home)
}

fn default_mail_root_from(
    vivi_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
    legacy_exists: bool,
) -> PathBuf {
    if let Some(path) = vivi_home_env_dir(vivi_home, home.as_deref()) {
        return path;
    }
    if legacy_exists && let Some(path) = legacy_path(home.as_deref(), LEGACY_MAIL_ROOT) {
        return path;
    }
    vivi_home_dir_from(None, home)
}

fn legacy_config_exists(home: Option<&Path>) -> bool {
    legacy_path(home, LEGACY_CONFIG_DIR).is_some_and(|path| path.exists())
}

fn legacy_mail_root_exists(home: Option<&Path>) -> bool {
    legacy_config_exists(home)
        || legacy_path(home, LEGACY_MAIL_ROOT).is_some_and(|path| path.exists())
}

fn legacy_path(home: Option<&Path>, parts: &[&str]) -> Option<PathBuf> {
    let mut path = home?.to_path_buf();
    for part in parts {
        path.push(part);
    }
    Some(path)
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, VivariumError> {
        if !path.exists() {
            tracing::debug!(path = %path.display(), "config file not found, using defaults");
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path).map_err(|e| {
            VivariumError::Config(format!("failed to read {}: {e}", path.display()))
        })?;
        toml::from_str(&contents)
            .map_err(|e| VivariumError::Config(format!("failed to parse config: {e}")))
    }

    pub fn default_path() -> PathBuf {
        config_dir().join("config.toml")
    }

    pub fn default_dir() -> PathBuf {
        config_dir()
    }

    pub fn default_mail_root() -> PathBuf {
        let home = dirs::home_dir();
        default_mail_root_from(
            std::env::var_os(VIVI_HOME_ENV),
            home.clone(),
            legacy_mail_root_exists(home.as_deref()),
        )
    }
}

impl AccountsFile {
    pub fn load(path: &Path) -> Result<Self, VivariumError> {
        Self::load_with_options(path, false)
    }

    pub fn load_with_options(path: &Path, ignore_permissions: bool) -> Result<Self, VivariumError> {
        if !path.exists() {
            return Err(VivariumError::Config(format!(
                "accounts file not found: {}",
                path.display()
            )));
        }
        check_permissions(path, ignore_permissions)?;
        let contents = fs::read_to_string(path).map_err(|e| {
            VivariumError::Config(format!("failed to read {}: {e}", path.display()))
        })?;
        toml::from_str(&contents)
            .map_err(|e| VivariumError::Config(format!("failed to parse accounts: {e}")))
    }

    pub fn default_path() -> PathBuf {
        config_dir().join("accounts.toml")
    }

    pub fn find_account(&self, name: &str) -> Result<&Account, VivariumError> {
        self.accounts
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| VivariumError::Config(format!("account not found: {name}")))
    }
}

/// Reject accounts.toml when it is readable by group or others.
fn check_permissions(path: &Path, ignore_permissions: bool) -> Result<(), VivariumError> {
    let metadata = fs::metadata(path)?;
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        if ignore_permissions {
            tracing::warn!(
                path = %path.display(),
                mode = format!("{mode:o}"),
                "accounts file has insecure permissions, ignoring by request"
            );
        } else {
            return Err(VivariumError::Config(format!(
                "insecure permissions on {}: expected mode 600, got {mode:o}; rerun with --ignore-permissions to bypass",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn expand_tilde(path: &str) -> PathBuf {
    expand_tilde_with_home(path, dirs::home_dir().as_deref())
}

fn expand_tilde_with_home(path: &str, home: Option<&Path>) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn vivi_home_overrides_home_for_defaults() {
        assert_eq!(
            vivi_home_dir_from(
                Some(OsString::from("/tmp/vivi-home")),
                Some(PathBuf::from("/tmp/home"))
            ),
            PathBuf::from("/tmp/vivi-home")
        );
    }

    #[test]
    fn vivi_home_expands_tilde_from_real_home() {
        assert_eq!(
            vivi_home_dir_from(
                Some(OsString::from("~/custom-vivi")),
                Some(PathBuf::from("/tmp/home"))
            ),
            PathBuf::from("/tmp/home/custom-vivi")
        );
    }

    #[test]
    fn empty_vivi_home_falls_back_to_default_vivi_home() {
        assert_eq!(
            vivi_home_dir_from(Some(OsString::new()), Some(PathBuf::from("/tmp/home"))),
            PathBuf::from("/tmp/home/.vivarium")
        );
    }

    #[test]
    fn missing_home_falls_back_to_local_vivi_home() {
        assert_eq!(vivi_home_dir_from(None, None), PathBuf::from(".vivarium"));
    }

    #[test]
    fn existing_legacy_config_dir_stays_default_without_vivi_home() {
        assert_eq!(
            config_dir_from(None, Some(PathBuf::from("/tmp/home")), true),
            PathBuf::from("/tmp/home/.config/vivarium")
        );
    }

    #[test]
    fn existing_legacy_mail_root_stays_default_without_vivi_home() {
        assert_eq!(
            default_mail_root_from(None, Some(PathBuf::from("/tmp/home")), true),
            PathBuf::from("/tmp/home/.local/share/vivarium")
        );
    }

    #[test]
    fn vivi_home_overrides_legacy_paths() {
        assert_eq!(
            config_dir_from(
                Some(OsString::from("/tmp/vivi-home")),
                Some(PathBuf::from("/tmp/home")),
                true
            ),
            PathBuf::from("/tmp/vivi-home")
        );
        assert_eq!(
            default_mail_root_from(
                Some(OsString::from("/tmp/vivi-home")),
                Some(PathBuf::from("/tmp/home")),
                true
            ),
            PathBuf::from("/tmp/vivi-home")
        );
    }
}
