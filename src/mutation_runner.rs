use vivarium::VivariumError;
use vivarium::cli::Command;
use vivarium::config::Account;
use vivarium::imap::{FlagMutation, MutationCapabilities, MutationResult, MutationTarget};
use vivarium::mutation_command::{
    LocalReconciliation, MutationAction, MutationPreview, PreparedMutation, append_audit,
    output_json, prepare_mutation, reconcile_success,
};

use super::Runtime;

pub(super) enum MutationDispatch {
    Handled,
    Unhandled(Command),
}

#[derive(Debug, Clone, Copy)]
struct MutationRunOptions {
    dry_run: bool,
    json: bool,
    confirm: bool,
}

impl Runtime {
    pub(super) async fn run_mutation_command(
        &self,
        command: Command,
    ) -> Result<MutationDispatch, VivariumError> {
        match command {
            Command::Archive {
                handles,
                dry_run,
                json,
            } => {
                self.run_mutations(
                    handles,
                    |_| Ok(MutationAction::Archive),
                    options(dry_run, json),
                )
                .await?;
            }
            Command::Delete {
                handle,
                trash: _,
                expunge,
                confirm,
                dry_run,
                json,
            } => {
                let action = if expunge {
                    MutationAction::Expunge
                } else {
                    MutationAction::Trash
                };
                self.run_mutations(
                    vec![handle],
                    |_| Ok(action.clone()),
                    MutationRunOptions {
                        dry_run,
                        json,
                        confirm,
                    },
                )
                .await?;
            }
            Command::Move {
                handle,
                folder,
                dry_run,
                json,
            } => {
                self.run_mutations(
                    vec![handle],
                    |_| {
                        Ok(MutationAction::Move {
                            folder: folder.clone(),
                        })
                    },
                    options(dry_run, json),
                )
                .await?;
            }
            Command::Flag {
                handle,
                read,
                unread,
                star,
                unstar,
                dry_run,
                json,
            } => {
                let mutation = flag_mutation(read, unread, star, unstar)?;
                self.run_mutations(
                    vec![handle],
                    |_| {
                        Ok(MutationAction::Flag {
                            mutation: mutation.clone(),
                        })
                    },
                    options(dry_run, json),
                )
                .await?;
            }
            other => return Ok(MutationDispatch::Unhandled(other)),
        }
        Ok(MutationDispatch::Handled)
    }

    async fn run_mutations<F>(
        &self,
        inputs: Vec<String>,
        action_for: F,
        options: MutationRunOptions,
    ) -> Result<(), VivariumError>
    where
        F: Fn(&str) -> Result<MutationAction, VivariumError>,
    {
        let acct = self.resolve_account(self.account.clone())?;
        validate_mutation_confirmation(&inputs, &action_for, options)?;
        let mail_root = acct.mail_path(&self.config);
        let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
        let discovery = vivarium::imap::discover_folders(&acct, reject_invalid_certs).await?;
        let capabilities = MutationCapabilities::from(&discovery.capabilities);
        let mut outputs = Vec::new();
        for input in inputs {
            let action = action_for(&input)?;
            let prepared = prepare_mutation(
                &acct,
                &mail_root,
                &input,
                action,
                &capabilities,
                options.dry_run,
            )?;
            let outcome = self
                .run_prepared_mutation(
                    &acct,
                    &prepared,
                    &capabilities,
                    reject_invalid_certs,
                    options,
                )
                .await?;
            if options.json {
                outputs.push(outcome.json);
            } else {
                println!("{}", outcome.text);
            }
        }
        if options.json {
            println!("{}", serde_json::to_string_pretty(&outputs).unwrap());
        }
        Ok(())
    }

    async fn run_prepared_mutation(
        &self,
        acct: &Account,
        prepared: &PreparedMutation,
        capabilities: &MutationCapabilities,
        reject_invalid_certs: bool,
        options: MutationRunOptions,
    ) -> Result<MutationOutcome, VivariumError> {
        let mail_root = acct.mail_path(&self.config);
        if options.dry_run {
            append_audit(&mail_root, prepared, "planned", true, None)?;
            return Ok(MutationOutcome::planned(&prepared.preview));
        }
        append_audit(&mail_root, prepared, "approved", false, None)?;
        let remote = execute_remote(acct, prepared, capabilities, reject_invalid_certs).await;
        match remote {
            Ok(_) => {
                append_audit(&mail_root, prepared, "executed", false, None)?;
                let local = reconcile_success(&mail_root, prepared).inspect_err(|err| {
                    append_audit(&mail_root, prepared, "failed", false, Some(err.to_string())).ok();
                })?;
                append_audit(&mail_root, prepared, "reconciled", false, None)?;
                Ok(MutationOutcome::executed(&prepared.preview, &local))
            }
            Err(err) => {
                append_audit(&mail_root, prepared, "failed", false, Some(err.to_string()))?;
                Err(err)
            }
        }
    }
}

fn options(dry_run: bool, json: bool) -> MutationRunOptions {
    MutationRunOptions {
        dry_run,
        json,
        confirm: false,
    }
}

fn flag_mutation(
    read: bool,
    unread: bool,
    star: bool,
    unstar: bool,
) -> Result<FlagMutation, VivariumError> {
    match (read, unread, star, unstar) {
        (true, false, false, false) => Ok(FlagMutation::Read),
        (false, true, false, false) => Ok(FlagMutation::Unread),
        (false, false, true, false) => Ok(FlagMutation::Starred),
        (false, false, false, true) => Ok(FlagMutation::Unstarred),
        _ => Err(VivariumError::Message(
            "choose exactly one of --read, --unread, --star, or --unstar".into(),
        )),
    }
}

async fn execute_remote(
    account: &Account,
    prepared: &PreparedMutation,
    capabilities: &MutationCapabilities,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    match &prepared.action {
        MutationAction::Archive => {
            let target = MutationTarget::Archive(account.archive_folder());
            move_remote(
                account,
                prepared,
                target,
                capabilities,
                reject_invalid_certs,
            )
            .await
        }
        MutationAction::Trash => {
            let target = MutationTarget::Trash(account.trash_folder());
            move_remote(
                account,
                prepared,
                target,
                capabilities,
                reject_invalid_certs,
            )
            .await
        }
        MutationAction::Move { folder } => {
            let target = MutationTarget::Folder(
                prepared
                    .preview
                    .target_mailbox
                    .clone()
                    .unwrap_or_else(|| folder.clone()),
            );
            move_remote(
                account,
                prepared,
                target,
                capabilities,
                reject_invalid_certs,
            )
            .await
        }
        MutationAction::Expunge => {
            hard_expunge_remote(account, prepared, capabilities, reject_invalid_certs).await
        }
        MutationAction::Flag { mutation } => {
            flag_remote(account, prepared, mutation.clone(), reject_invalid_certs).await
        }
    }
}

async fn move_remote(
    account: &Account,
    prepared: &PreparedMutation,
    target: MutationTarget,
    capabilities: &MutationCapabilities,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    vivarium::imap::move_message(
        account,
        &prepared.remote,
        target,
        capabilities,
        reject_invalid_certs,
    )
    .await
}

async fn hard_expunge_remote(
    account: &Account,
    prepared: &PreparedMutation,
    capabilities: &MutationCapabilities,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    vivarium::imap::hard_expunge(
        account,
        &prepared.remote,
        capabilities,
        reject_invalid_certs,
    )
    .await
}

async fn flag_remote(
    account: &Account,
    prepared: &PreparedMutation,
    mutation: FlagMutation,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    vivarium::imap::mutate_flag(account, &prepared.remote, mutation, reject_invalid_certs).await
}

fn validate_mutation_confirmation<F>(
    inputs: &[String],
    action_for: &F,
    options: MutationRunOptions,
) -> Result<(), VivariumError>
where
    F: Fn(&str) -> Result<MutationAction, VivariumError>,
{
    if options.dry_run || options.confirm {
        return Ok(());
    }
    for input in inputs {
        if matches!(action_for(input)?, MutationAction::Expunge) {
            return Err(VivariumError::Message(
                "hard expunge requires --confirm; use --dry-run to preview".into(),
            ));
        }
    }
    Ok(())
}

struct MutationOutcome {
    text: String,
    json: serde_json::Value,
}

impl MutationOutcome {
    fn planned(preview: &MutationPreview) -> Self {
        Self {
            text: format!(
                "planned {} {} via {}",
                preview.operation, preview.handle, preview.command_path
            ),
            json: output_json(preview, "planned", None),
        }
    }

    fn executed(preview: &MutationPreview, reconciliation: &LocalReconciliation) -> Self {
        Self {
            text: format!(
                "executed {} {} via {}",
                preview.operation, preview.handle, preview.command_path
            ),
            json: output_json(preview, "executed", Some(reconciliation)),
        }
    }
}

#[cfg(test)]
mod tests;
