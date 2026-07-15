use std::io::IsTerminal;

use serde::Serialize;

use super::{Runtime, VivariumError};

#[derive(Debug, Serialize)]
struct DoctorReport {
    account: String,
    provider: String,
    policy: String,
    imap: EndpointReport,
    smtp: EndpointReport,
    mail_root: String,
    checks: Vec<DoctorCheck>,
}

#[derive(Debug, Serialize)]
struct EndpointReport {
    host: String,
    port: u16,
    security: String,
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: &'static str,
    ok: bool,
    detail: String,
}

impl Runtime {
    pub(crate) async fn doctor(
        &self,
        account: Option<String>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(account))?;
        let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
        let mut checks = Vec::new();

        let mail_root = acct.mail_path(&self.config);
        checks.push(check(
            "config",
            true,
            format!("loaded account '{}' ({})", acct.name, acct.provider),
        ));
        checks.push(mail_root_check(&mail_root));

        match vivarium::imap::discover_folders(&acct, reject_invalid_certs).await {
            Ok(discovery) => checks.push(check(
                "imap",
                true,
                format!(
                    "authenticated; {} folders; MOVE={}; IDLE={}",
                    discovery.folders.len(),
                    yes_no(discovery.capabilities.move_supported),
                    yes_no(discovery.capabilities.idle)
                ),
            )),
            Err(err) => checks.push(check("imap", false, err.to_string())),
        }

        match vivarium::smtp::test_connection(&acct, reject_invalid_certs).await {
            Ok(true) => checks.push(check("smtp", true, "authenticated; NOOP succeeded")),
            Ok(false) => checks.push(check(
                "smtp",
                false,
                "NOOP reported the connection is closed",
            )),
            Err(err) => checks.push(check("smtp", false, err.to_string())),
        }

        let report = DoctorReport {
            account: acct.name.clone(),
            provider: acct.provider.to_string(),
            policy: acct.policy.to_string(),
            imap: EndpointReport {
                host: acct.resolved_imap_host(),
                port: acct.resolved_imap_port(),
                security: acct.resolved_imap_security().to_string(),
            },
            smtp: EndpointReport {
                host: acct.resolved_smtp_host(),
                port: acct.resolved_smtp_port(),
                security: acct.resolved_smtp_security().to_string(),
            },
            mail_root: mail_root.display().to_string(),
            checks,
        };

        let passed = report.checks.iter().all(|check| check.ok);
        if as_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into())
            );
        } else {
            print_report(&report);
        }

        if passed {
            Ok(())
        } else {
            Err(VivariumError::Other(format!(
                "doctor found problems for account '{}'",
                report.account
            )))
        }
    }
}

fn check(name: &'static str, ok: bool, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name,
        ok,
        detail: detail.into(),
    }
}

fn mail_root_check(mail_root: &std::path::Path) -> DoctorCheck {
    if mail_root.exists() {
        return check(
            "mail-root",
            true,
            format!("exists at {}", mail_root.display()),
        );
    }
    if mail_root.parent().is_some_and(std::path::Path::exists) {
        return check(
            "mail-root",
            true,
            format!("will be created at {}", mail_root.display()),
        );
    }
    check(
        "mail-root",
        false,
        format!("parent directory is missing for {}", mail_root.display()),
    )
}

fn print_report(report: &DoctorReport) {
    let style = Style::detect();
    println!(
        "{}",
        style.heading(&format!("Vivi Doctor: {}", report.account))
    );
    println!("provider  {}", report.provider);
    println!("policy    {}", report.policy);
    println!(
        "imap      {}:{} ({})",
        report.imap.host, report.imap.port, report.imap.security
    );
    println!(
        "smtp      {}:{} ({})",
        report.smtp.host, report.smtp.port, report.smtp.security
    );
    println!("mail      {}", report.mail_root);
    println!();

    for check in &report.checks {
        let marker = if check.ok {
            style.ok_padded("OK", 6)
        } else {
            style.err_padded("FAIL", 6)
        };
        println!("{marker} {:<10} {}", check.name, check.detail);
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

struct Style {
    color: bool,
}

impl Style {
    fn detect() -> Self {
        Self {
            color: std::io::stdout().is_terminal(),
        }
    }

    fn heading(&self, text: &str) -> String {
        if self.color {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn ok(&self, text: &str) -> String {
        if self.color {
            format!("\x1b[32m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn err(&self, text: &str) -> String {
        if self.color {
            format!("\x1b[31m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn ok_padded(&self, text: &str, width: usize) -> String {
        let padded = format!("{text:<width$}");
        self.ok(&padded)
    }

    fn err_padded(&self, text: &str, width: usize) -> String {
        let padded = format!("{text:<width$}");
        self.err(&padded)
    }
}
