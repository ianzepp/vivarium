const DEFAULT_BRIDGE_VERSION: &str = "3.24.2";

pub(super) fn default_app_version() -> String {
    format!("{}-bridge@{}", proton_api_os(), DEFAULT_BRIDGE_VERSION)
}

fn proton_api_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}
