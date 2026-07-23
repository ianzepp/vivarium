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
