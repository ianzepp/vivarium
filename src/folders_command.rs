use super::{Runtime, VivariumError};

impl Runtime {
    pub(crate) async fn folders(
        &self,
        account: Option<String>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(account))?;
        let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
        let discovery = vivarium::imap::discover_folders(&acct, reject_invalid_certs).await?;
        if as_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&discovery).unwrap_or_else(|_| "{}".into())
            );
        } else {
            print_folder_discovery(&discovery);
        }
        Ok(())
    }
}

fn print_folder_discovery(discovery: &vivarium::imap::FolderDiscovery) {
    println!("# {}", discovery.account);
    println!("provider: {}", discovery.provider);
    println!("folders:");
    println!("  inbox: {}", discovery.resolved.inbox);
    println!("  archive: {}", discovery.resolved.archive);
    println!("  trash: {}", discovery.resolved.trash);
    println!("  sent: {}", discovery.resolved.sent);
    println!("  drafts: {}", discovery.resolved.drafts);
    println!(
        "capabilities: UIDPLUS={} MOVE={} SPECIAL-USE={} APPEND={} IDLE={}",
        yes_no(discovery.capabilities.uidplus),
        yes_no(discovery.capabilities.move_supported),
        yes_no(discovery.capabilities.special_use),
        yes_no(discovery.capabilities.append),
        yes_no(discovery.capabilities.idle)
    );
    println!("remote folders:");
    for folder in &discovery.folders {
        if folder.attributes.is_empty() {
            println!("  {}", folder.name);
        } else {
            println!("  {} [{}]", folder.name, folder.attributes.join(", "));
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
