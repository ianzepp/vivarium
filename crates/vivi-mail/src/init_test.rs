use super::*;

#[test]
fn init_creates_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("vivarium");

    // Patch paths by writing directly
    fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    let accounts = dir.join("accounts.toml");

    write_if_missing(&config, DEFAULT_CONFIG).unwrap();
    write_if_missing(&accounts, DEFAULT_ACCOUNTS).unwrap();
    fs::set_permissions(&accounts, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(config.exists());
    assert!(accounts.exists());

    let mode = fs::metadata(&accounts).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn init_does_not_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(&path, "custom content").unwrap();

    write_if_missing(&path, DEFAULT_CONFIG).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "custom content");
}
