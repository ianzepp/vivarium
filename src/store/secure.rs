use std::fs;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::error::VivariumError;

pub(crate) fn secure_create_dir_all(path: &Path) -> Result<(), VivariumError> {
    fs::create_dir_all(path)?;
    secure_dir(path)
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
