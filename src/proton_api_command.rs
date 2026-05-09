use serde::Serialize;

use vivarium::cli::ProtonCommand;
use vivarium::config::{Auth, Provider};
use vivarium::{VivariumError, proton_api};

use super::Runtime;

#[derive(Debug, Serialize)]
struct AuthInfoReport {
    account: String,
    provider: String,
    username: String,
    auth_info: proton_api::AuthInfoSummary,
}

#[derive(Debug, Serialize)]
struct LoginCheckReport {
    account: String,
    provider: String,
    username: String,
    login: proton_api::LoginCheck,
}

#[derive(Debug, Serialize)]
struct SessionReport {
    account: String,
    provider: String,
    username: String,
    session_path: String,
    session: proton_api::LoginCheck,
}

#[derive(Debug, Serialize)]
struct IdentityReport {
    account: String,
    provider: String,
    username: String,
    session_path: String,
    identity: proton_api::ProtonIdentity,
}

impl Runtime {
    pub(crate) async fn proton_command(&self, command: ProtonCommand) -> Result<(), VivariumError> {
        match command {
            ProtonCommand::AuthInfo { account, json } => self.proton_auth_info(account, json).await,
            ProtonCommand::Identity { account, json } => self.proton_identity(account, json).await,
            ProtonCommand::Login {
                account,
                totp_code,
                json,
            } => self.proton_login(account, totp_code.as_deref(), json).await,
            ProtonCommand::LoginCheck {
                account,
                totp_code,
                json,
            } => {
                self.proton_login_check(account, totp_code.as_deref(), json)
                    .await
            }
            ProtonCommand::SessionCheck { account, json } => {
                self.proton_session_check(account, json).await
            }
        }
    }

    async fn proton_auth_info(
        &self,
        account: Option<String>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        let client = proton_api::ProtonApiClient::default();
        let auth_info = client.auth_info(&acct.username).await?;
        let report = AuthInfoReport {
            account: acct.name,
            provider: acct.provider.to_string(),
            username: acct.username,
            auth_info: auth_info.summary(),
        };
        print_report(&report, as_json, |report| {
            println!("Vivi Proton API auth-info: {}", report.account);
            println!("provider  {}", report.provider);
            println!("username  {}", report.username);
            println!("srp       v{}", report.auth_info.version);
            println!("session   {}", yes_no(report.auth_info.srp_session_present));
            println!("2fa       {}", report.auth_info.two_fa.enabled);
        })
    }

    async fn proton_login_check(
        &self,
        account: Option<String>,
        totp_code: Option<&str>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        if acct.auth != Auth::Password {
            return Err(VivariumError::Config(format!(
                "account '{}' uses auth = \"{}\"; direct Proton API login-check requires auth = \"password\"",
                acct.name, acct.auth
            )));
        }
        let password = acct.resolve_secret().await?;
        let client = proton_api::ProtonApiClient::default();
        let login = client
            .login_check(&acct.username, &password, totp_code)
            .await?;
        let report = LoginCheckReport {
            account: acct.name,
            provider: acct.provider.to_string(),
            username: acct.username,
            login,
        };
        print_report(&report, as_json, |report| {
            println!("Vivi Proton API login-check: {}", report.account);
            println!("provider  {}", report.provider);
            println!("username  {}", report.username);
            println!("user      {}", yes_no(report.login.user_id_present));
            println!("uid       {}", yes_no(report.login.uid_present));
            println!("scope     {}", report.login.scope);
            println!("2fa       {}", report.login.two_fa.enabled);
            println!("version   {}", report.login.app_version);
        })
    }

    async fn proton_login(
        &self,
        account: Option<String>,
        totp_code: Option<&str>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        if acct.auth != Auth::Password {
            return Err(VivariumError::Config(format!(
                "account '{}' uses auth = \"{}\"; direct Proton API login requires auth = \"password\"",
                acct.name, acct.auth
            )));
        }
        let mail_root = acct.mail_path(&self.config);
        let password = acct.resolve_secret().await?;
        let client = proton_api::ProtonApiClient::default();
        let session = client.login(&acct.username, &password, totp_code).await?;
        let store = proton_api::ProtonSessionStore::new(&mail_root);
        store.save(&session)?;
        let report = SessionReport {
            account: acct.name,
            provider: acct.provider.to_string(),
            username: acct.username,
            session_path: store.path().display().to_string(),
            session: session.check(),
        };
        print_session_report("Vivi Proton API login", &report, as_json)
    }

    async fn proton_session_check(
        &self,
        account: Option<String>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        let mail_root = acct.mail_path(&self.config);
        let store = proton_api::ProtonSessionStore::new(&mail_root);
        let session = store.load()?;
        let client = proton_api::ProtonApiClient::default();
        let refreshed = client.refresh(&session).await.map_err(|e| {
            VivariumError::Other(format!(
                "stored direct Proton API session could not be refreshed; run `vivi proton login --account {}` again: {e}",
                acct.name
            ))
        })?;
        store.save(&refreshed)?;
        let report = SessionReport {
            account: acct.name,
            provider: acct.provider.to_string(),
            username: acct.username,
            session_path: store.path().display().to_string(),
            session: refreshed.check(),
        };
        print_session_report("Vivi Proton API session-check", &report, as_json)
    }

    async fn proton_identity(
        &self,
        account: Option<String>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        let mail_root = acct.mail_path(&self.config);
        let store = proton_api::ProtonSessionStore::new(&mail_root);
        let session = store.load()?;
        let client = proton_api::ProtonApiClient::default();
        let (session, identity) = client.identity(&session).await.map_err(|e| {
            VivariumError::Other(format!(
                "stored direct Proton API session could not fetch identity; run `vivi proton login --account {}` again if the session is expired: {e}",
                acct.name
            ))
        })?;
        store.save(&session)?;
        let report = IdentityReport {
            account: acct.name,
            provider: acct.provider.to_string(),
            username: acct.username,
            session_path: store.path().display().to_string(),
            identity,
        };
        print_identity_report(&report, as_json)
    }

    fn resolve_proton_api_account(
        &self,
        account: Option<String>,
    ) -> Result<vivarium::config::Account, VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(account))?;
        if acct.provider != Provider::ProtonApi {
            return Err(VivariumError::Config(format!(
                "account '{}' uses provider = \"{}\"; use provider = \"proton-api\" for direct Proton API commands",
                acct.name, acct.provider
            )));
        }
        Ok(acct)
    }
}

fn print_identity_report(report: &IdentityReport, as_json: bool) -> Result<(), VivariumError> {
    print_report(report, as_json, |report| {
        println!("Vivi Proton API identity: {}", report.account);
        println!("provider  {}", report.provider);
        println!("username  {}", report.username);
        println!("path      {}", report.session_path);
        println!("user      {}", yes_no(report.identity.user.id_present));
        println!("email     {}", report.identity.user.email);
        println!("addresses {}", report.identity.addresses.len());
        println!(
            "keys      user={} address={} active={} primary={}",
            report.identity.key_state.user_key_count,
            report.identity.key_state.address_key_count,
            report.identity.key_state.active_address_key_count,
            report.identity.key_state.primary_address_key_count
        );
        println!(
            "locked    private_key_hints={} token_hints={}",
            report.identity.key_state.locked_key_hint_count,
            report.identity.key_state.token_key_hint_count
        );
    })
}

fn print_report<T: Serialize>(
    report: &T,
    as_json: bool,
    print_text: impl FnOnce(&T),
) -> Result<(), VivariumError> {
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".into())
        );
    } else {
        print_text(report);
    }
    Ok(())
}

fn print_session_report(
    title: &str,
    report: &SessionReport,
    as_json: bool,
) -> Result<(), VivariumError> {
    print_report(report, as_json, |report| {
        println!("{title}: {}", report.account);
        println!("provider  {}", report.provider);
        println!("username  {}", report.username);
        println!("path      {}", report.session_path);
        println!("uid       {}", yes_no(report.session.uid_present));
        println!("user      {}", yes_no(report.session.user_id_present));
        println!("scope     {}", report.session.scope);
        println!("2fa       {}", report.session.two_fa.enabled);
        println!("version   {}", report.session.app_version);
        println!("updated   {}", report.session.updated_at);
    })
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
