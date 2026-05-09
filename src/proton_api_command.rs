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

impl Runtime {
    pub(crate) async fn proton_command(&self, command: ProtonCommand) -> Result<(), VivariumError> {
        match command {
            ProtonCommand::AuthInfo { account, json } => self.proton_auth_info(account, json).await,
            ProtonCommand::LoginCheck {
                account,
                totp_code,
                json,
            } => {
                self.proton_login_check(account, totp_code.as_deref(), json)
                    .await
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
            println!("scope     {}", report.login.scope);
            println!("2fa       {}", report.login.two_fa.enabled);
        })
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

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
