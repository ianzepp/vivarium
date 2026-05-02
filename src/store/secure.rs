use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::error::VivariumError;

pub(crate) fn secure_create_dir_all(path: &Path) -> Result<(), VivariumError> {
    fs::create_dir_all(path)?;
    secure_dir(path)
}

pub(crate) fn secure_write(path: &Path, data: &[u8]) -> Result<(), VivariumError> {
    if let Some(parent) = path.parent() {
        secure_create_dir_all(parent)?;
    }
    let mut file = secure_create_file(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn secure_create_file(path: &Path) -> Result<fs::File, VivariumError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    secure_file(path)?;
    Ok(file)
}

fn secure_dir(path: &Path) -> Result<(), VivariumError> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn secure_file(path: &Path) -> Result<(), VivariumError> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}
