use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn expand_tilde_no_tilde() {
    assert_eq!(expand_tilde("/tmp/mail"), PathBuf::from("/tmp/mail"));
}

#[test]
fn expand_tilde_with_tilde() {
    let expanded = expand_tilde("~/mail");
    assert!(!expanded.starts_with("~"));
}

#[test]
fn config_defaults_when_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nonexistent.toml");
    let config = Config::load(&path).unwrap();
    assert!(config.defaults.mail_root.is_none());
    assert!(!config.defaults.reject_invalid_certs);
}

#[test]
fn config_parses_reject_invalid_certs_default() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(&path, "[defaults]\nreject_invalid_certs = true\n").unwrap();
    let config = Config::load(&path).unwrap();
    assert!(config.defaults.reject_invalid_certs);
}

#[test]
fn accounts_file_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nonexistent.toml");
    let err = AccountsFile::load(&path).unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn accounts_file_parses() {
    let path = accounts_file(
        r#"
        [[accounts]]
        name = "test"
        email = "test@example.com"
        imap_host = "imap.example.com"
        smtp_host = "smtp.example.com"
        username = "test"
        password = "secret"
    "#,
        0o600,
    );
    let accounts = AccountsFile::load(&path).unwrap();
    assert_eq!(accounts.accounts.len(), 1);
    assert_eq!(accounts.accounts[0].name, "test");
}

#[test]
fn accounts_file_rejects_insecure_permissions() {
    let path = accounts_file(
        r#"
        [[accounts]]
        name = "test"
        email = "test@example.com"
        imap_host = "imap.example.com"
        smtp_host = "smtp.example.com"
        username = "test"
        password = "secret"
    "#,
        0o644,
    );

    let err = AccountsFile::load(&path).unwrap_err();
    assert!(err.to_string().contains("insecure permissions"));
}

#[test]
fn accounts_file_can_ignore_insecure_permissions() {
    let path = accounts_file(
        "[[accounts]]\nname=\"t\"\nemail=\"e\"\nimap_host=\"h\"\nsmtp_host=\"s\"\nusername=\"u\"\npassword=\"p\"\n",
        0o644,
    );
    let accounts = AccountsFile::load_with_options(&path, true).unwrap();
    assert_eq!(accounts.accounts.len(), 1);
}

#[test]
fn find_account_by_name() {
    let path = accounts_file(
        "[[accounts]]\nname = \"a\"\nemail = \"a@b\"\nimap_host = \"h\"\nsmtp_host = \"s\"\nusername = \"u\"\npassword = \"p\"\n",
        0o600,
    );
    let accounts = AccountsFile::load(&path).unwrap();
    assert_eq!(accounts.find_account("a").unwrap().name, "a");
    assert!(accounts.find_account("nope").is_err());
}

#[test]
fn account_parses_xoauth2_auth() {
    let path = accounts_file(
        r#"
        [[accounts]]
        name = "gmail"
        email = "ian@gmail.com"
        imap_host = "imap.gmail.com"
        smtp_host = "smtp.gmail.com"
        username = "ian@gmail.com"
        auth = "xoauth2"
        provider = "gmail"
        token_cmd = "pass gmail-token"
        oauth_client_id = "client-id"
        oauth_client_secret = "client-secret"
    "#,
        0o600,
    );

    let accounts = AccountsFile::load(&path).unwrap();
    let gmail = accounts.find_account("gmail").unwrap();
    assert_eq!(gmail.auth, types::Auth::Xoauth2);
    assert_eq!(gmail.token_cmd.as_deref(), Some("pass gmail-token"));
    assert_eq!(gmail.oauth_client_id.as_deref(), Some("client-id"));
    let urls = gmail.oauth_urls().unwrap();
    assert!(urls.auth_url.contains("google"));
    assert!(urls.token_url.contains("google"));
    assert!(urls.scope.contains("mail.google"));
}

#[test]
fn provider_standard_has_no_oauth_defaults() {
    assert!(types::Provider::Standard.oauth_config().is_none());
}

#[test]
fn provider_protonmail_has_oauth_defaults() {
    let config = types::Provider::Protonmail.oauth_config().unwrap();
    assert!(config.auth_url.contains("proton"));
    assert!(config.token_url.contains("proton"));
    assert!(config.scope.contains("protonmail"));
}

#[test]
fn account_oauth_urls_override_provider_defaults() {
    let path = accounts_file(
        r#"[[accounts]]
name="c"
email="u@e.com"
imap_host="imap.e.com"
smtp_host="smtp.e.com"
username="u"
auth="xoauth2"
provider="standard"
oauth_authorization_url="https://custom.auth/authorize"
oauth_token_url="https://custom.auth/token"
oauth_scope="https://custom.auth/mail"
"#,
        0o600,
    );
    let accounts = AccountsFile::load(&path).unwrap();
    let acct = accounts.find_account("c").unwrap();
    let urls = acct.oauth_urls().unwrap();
    assert!(urls.auth_url.contains("custom.auth/authorize"));
    assert!(urls.token_url.contains("custom.auth/token"));
    assert!(urls.scope.contains("custom.auth/mail"));
}

#[test]
fn account_xoauth2_standard_provider_errors_without_urls() {
    let path = accounts_file(
        "[[accounts]]\nname=\"n\"\nemail=\"u@e\"\nimap_host=\"h\"\nsmtp_host=\"s\"\nusername=\"u\"\nauth=\"xoauth2\"\nprovider=\"standard\"\n",
        0o600,
    );
    let accounts = AccountsFile::load(&path).unwrap();
    let acct = accounts.find_account("n").unwrap();
    assert!(acct.oauth_urls().is_err());
}

#[test]
fn protonmail_defaults_resolved_host_when_empty() {
    let account = account_with_provider(types::Provider::Protonmail);
    assert_eq!(account.resolved_imap_host(), "127.0.0.1");
    assert_eq!(account.resolved_imap_port(), 1143);
    assert_eq!(account.resolved_smtp_port(), 1025);
    assert!(account.defaults_to_accept_invalid_certs());
}

#[test]
fn protonmail_defaults_resolves_explicit_host() {
    let mut account = account_with_provider(types::Provider::Protonmail);
    account.imap_host = "bridge.local".into();
    account.imap_security = types::Security::Starttls;
    assert_eq!(account.resolved_imap_host(), "bridge.local");
    assert_eq!(account.resolved_imap_port(), 1143);
    assert_eq!(account.resolved_smtp_port(), 1025);
}

#[test]
fn protonmail_default_reject_invalid_certs_is_true() {
    let mut account = account_with_provider(types::Provider::Protonmail);
    account.imap_host = "127.0.0.1".into();
    account.smtp_host = "127.0.0.1".into();
    let config = Config::default();
    assert!(account.reject_invalid_certs(&config));
}

#[test]
fn standard_provider_uses_config_default_for_certs() {
    let config = Config {
        defaults: types::Defaults {
            reject_invalid_certs: true,
            ..types::Defaults::default()
        },
    };
    let mut account = account_with_provider(types::Provider::Standard);
    account.name = "custom".into();
    account.email = "u@e.com".into();
    account.imap_host = "imap.e.com".into();
    account.smtp_host = "smtp.e.com".into();
    assert!(account.reject_invalid_certs(&config));
}

fn accounts_file(contents: &str, mode: u32) -> PathBuf {
    let path = tempfile::tempdir().unwrap().keep().join("accounts.toml");
    fs::write(&path, contents).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

fn account_with_provider(provider: types::Provider) -> types::Account {
    types::Account {
        name: "proton".into(),
        email: "u@p.me".into(),
        imap_host: "".into(),
        imap_port: None,
        imap_security: types::Security::Ssl,
        smtp_host: "".into(),
        smtp_port: None,
        smtp_security: types::Security::Ssl,
        username: "u".into(),
        auth: types::Auth::Password,
        password: Some("pw".into()),
        password_cmd: None,
        token_cmd: None,
        oauth_client_id: None,
        oauth_client_secret: None,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        mail_dir: None,
        provider,
        reject_invalid_certs: None,
    }
}
