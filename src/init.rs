use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::config::{AccountsFile, Config};
use crate::error::VivariumError;

const DEFAULT_CONFIG: &str = r#"[defaults]
# mail_root = "~/.local/share/vivarium"
# check_interval_secs = 300
"#;

const DEFAULT_ACCOUNTS: &str = r#"# Each [[accounts]] entry defines an email account.
# Passwords are stored here — keep this file chmod 600.

# [[accounts]]
# name = "gmail"
# email = "you@gmail.com"
# username = "you@gmail.com"
# auth = "xoauth2"
# provider = "gmail"
# oauth_client_id = "your-google-oauth-client-id.apps.googleusercontent.com"
# oauth_client_secret = "your-google-oauth-client-secret"
# token_cmd = "vivarium token gmail"
# imap_host = "imap.gmail.com"
# imap_security = "ssl"
# smtp_host = "smtp.gmail.com"
# smtp_security = "ssl"

# [[accounts]]
# name = "proton"
# email = "you@proton.me"
# username = "you@proton.me"
# auth = "xoauth2"
# provider = "protonmail"
# oauth_client_id = "your-proton-oauth-client-id"
# oauth_client_secret = "your-proton-oauth-client-secret"
# token_cmd = "vivarium token proton"
# imap_host = "imap.protonmail.com"
# imap_port = 993
# imap_security = "ssl"
# smtp_host = "smtp.protonmail.com"
# smtp_port = 465
# smtp_security = "ssl"

# [[accounts]]
# name = "custom"
# email = "you@example.com"
# username = "you@example.com"
# auth = "xoauth2"
# provider = "standard"
# oauth_authorization_url = "https://your-provider/oauth/authorize"
# oauth_token_url = "https://your-provider/oauth/token"
# oauth_scope = "https://your-provider/mail.readwrite"
# oauth_client_id = "your-custom-oauth-client-id"
# oauth_client_secret = "your-custom-oauth-client-secret"
# token_cmd = "your-token-command"
# imap_host = "imap.example.com"
# imap_port = 993
# imap_security = "ssl"
# smtp_host = "smtp.example.com"
# smtp_port = 465
# smtp_security = "ssl"
"#;

pub fn run_init() -> Result<(), VivariumError> {
    let config_path = Config::default_path();
    let accounts_path = AccountsFile::default_path();
    let dir = config_path.parent().expect("config path has parent");

    if !dir.exists() {
        fs::create_dir_all(dir)?;
        println!("created {}", dir.display());
    }

    write_if_missing(&config_path, DEFAULT_CONFIG)?;
    write_if_missing(&accounts_path, DEFAULT_ACCOUNTS)?;

    // Ensure accounts.toml is 600
    fs::set_permissions(&accounts_path, fs::Permissions::from_mode(0o600))?;

    println!();
    println!("vivarium is ready. Next steps:");
    println!("  1. Edit {} and add an account", accounts_path.display());
    println!("  2. Run: vivarium sync");

    Ok(())
}

fn write_if_missing(path: &Path, content: &str) -> Result<(), VivariumError> {
    if path.exists() {
        println!("exists  {}", path.display());
    } else {
        fs::write(path, content)?;
        println!("created {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
