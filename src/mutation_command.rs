use std::path::Path;

use serde::Serialize;

use crate::catalog::{Catalog, CatalogEntry, RemoteIdentity};
use crate::config::Account;
use crate::error::VivariumError;
use crate::imap::{FlagMutation, MutationCapabilities, MutationPlan, MutationTarget};
use crate::store::MailStore;

mod audit;
pub use audit::{MutationAuditRecord, append_audit, audit_record};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationAction {
    Archive,
    Trash,
    Expunge,
    Move { folder: String },
    Flag { mutation: FlagMutation },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MutationPreview {
    pub account: String,
    pub handle: String,
    pub input: String,
    pub local_message_id: String,
    pub operation: String,
    pub source_mailbox: String,
    pub source_local_folder: String,
    pub target_mailbox: Option<String>,
    pub target_local_folder: Option<String>,
    pub uid: u32,
    pub uidvalidity: u32,
    pub command_path: String,
    pub dry_run: bool,
    pub requires_confirm: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalReconciliation {
    pub action: String,
    pub local_role: Option<String>,
    pub read_state: Option<bool>,
    pub starred: Option<bool>,
    pub remote_binding: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedMutation {
    pub action: MutationAction,
    pub entry: CatalogEntry,
    pub remote: RemoteIdentity,
    pub preview: MutationPreview,
    pub plan: MutationPlan,
}

pub fn prepare_mutation(
    account: &Account,
    mail_root: &Path,
    input: &str,
    action: MutationAction,
    capabilities: &MutationCapabilities,
    dry_run: bool,
) -> Result<PreparedMutation, VivariumError> {
    let catalog = Catalog::open(mail_root)?;
    let entry = catalog.resolve_entry(&account.name, input).ok_or_else(|| {
        VivariumError::Message(format!(
            "message not found in catalog for account '{}': {input}",
            account.name
        ))
    })?;
    let remote = entry.remote.clone().ok_or_else(|| {
        VivariumError::Message(format!(
            "message has no remote identity yet: {}",
            entry.handle
        ))
    })?;
    let plan = mutation_plan(account, &action, capabilities)?;
    let target = target_folders(account, &action)?;
    let preview = MutationPreview {
        account: account.name.clone(),
        handle: entry.handle.clone(),
        input: input.into(),
        local_message_id: entry.handle.clone(),
        operation: action.name().into(),
        source_mailbox: remote.remote_mailbox.clone(),
        source_local_folder: entry.local_role.clone(),
        target_mailbox: target.remote,
        target_local_folder: target.local,
        uid: remote.uid,
        uidvalidity: remote.uidvalidity,
        command_path: command_path(&plan),
        dry_run,
        requires_confirm: matches!(action, MutationAction::Expunge),
    };
    Ok(PreparedMutation {
        action,
        entry,
        remote,
        preview,
        plan,
    })
}

pub fn reconcile_success(
    mail_root: &Path,
    prepared: &PreparedMutation,
) -> Result<LocalReconciliation, VivariumError> {
    let store = MailStore::new(mail_root);
    let mut catalog = Catalog::open(mail_root)?;
    match &prepared.action {
        MutationAction::Archive | MutationAction::Trash | MutationAction::Move { .. } => {
            reconcile_move(&store, &mut catalog, prepared)
        }
        MutationAction::Expunge => reconcile_expunge(&store, &mut catalog, prepared),
        MutationAction::Flag { mutation } => {
            reconcile_flag(&store, &mut catalog, prepared, mutation)
        }
    }
}

pub fn output_json(
    preview: &MutationPreview,
    status: &str,
    reconciliation: Option<&LocalReconciliation>,
) -> serde_json::Value {
    serde_json::json!({
        "status": status,
        "plan": preview,
        "local_reconciliation": reconciliation,
    })
}

fn mutation_plan(
    account: &Account,
    action: &MutationAction,
    capabilities: &MutationCapabilities,
) -> Result<MutationPlan, VivariumError> {
    match action {
        MutationAction::Archive => crate::imap::plan_move(
            MutationTarget::Archive(account.archive_folder()),
            capabilities,
        ),
        MutationAction::Trash => {
            crate::imap::plan_move(MutationTarget::Trash(account.trash_folder()), capabilities)
        }
        MutationAction::Move { folder } => crate::imap::plan_move(
            MutationTarget::Folder(remote_folder(account, folder)?),
            capabilities,
        ),
        MutationAction::Expunge => {
            if capabilities.uidplus {
                Ok(MutationPlan::HardExpunge)
            } else {
                Err(VivariumError::Imap(
                    "hard expunge requires UIDPLUS for scoped UID EXPUNGE".into(),
                ))
            }
        }
        MutationAction::Flag { mutation } => Ok(crate::imap::plan_flag(mutation.clone())),
    }
}

fn reconcile_move(
    _store: &MailStore,
    catalog: &mut Catalog,
    prepared: &PreparedMutation,
) -> Result<LocalReconciliation, VivariumError> {
    let target = prepared
        .preview
        .target_local_folder
        .as_deref()
        .ok_or_else(|| {
            VivariumError::Message("mutation has no supported local mirror target".into())
        })?;
    let read_state = is_read(&prepared.entry);
    let starred = is_starred(&prepared.entry);
    catalog.update_message_state(
        &prepared.preview.account,
        &prepared.preview.handle,
        target,
        read_state,
        starred,
        None,
    )?;
    Ok(LocalReconciliation {
        action: "update_message_row".into(),
        local_role: Some(target.to_string()),
        read_state: Some(read_state),
        starred: Some(starred),
        remote_binding: Some("cleared".into()),
    })
}

fn reconcile_expunge(
    _store: &MailStore,
    catalog: &mut Catalog,
    prepared: &PreparedMutation,
) -> Result<LocalReconciliation, VivariumError> {
    catalog.remove_entry(&prepared.preview.account, &prepared.preview.handle)?;
    Ok(LocalReconciliation {
        action: "remove_message_row".into(),
        local_role: None,
        read_state: None,
        starred: None,
        remote_binding: None,
    })
}

fn reconcile_flag(
    _store: &MailStore,
    catalog: &mut Catalog,
    prepared: &PreparedMutation,
    mutation: &FlagMutation,
) -> Result<LocalReconciliation, VivariumError> {
    let read_state = updated_read_state(&prepared.entry, mutation);
    let starred = updated_starred_state(&prepared.entry, mutation);
    catalog.update_message_state(
        &prepared.preview.account,
        &prepared.preview.handle,
        &prepared.entry.local_role,
        read_state,
        starred,
        prepared.entry.remote.clone(),
    )?;
    Ok(LocalReconciliation {
        action: "update_message_flags".into(),
        local_role: Some(prepared.entry.local_role.clone()),
        read_state: Some(read_state),
        starred: Some(starred),
        remote_binding: Some("preserved".into()),
    })
}

fn target_folders(
    account: &Account,
    action: &MutationAction,
) -> Result<TargetFolders, VivariumError> {
    Ok(match action {
        MutationAction::Archive => TargetFolders::new(account.archive_folder(), "archive"),
        MutationAction::Trash => TargetFolders::new(account.trash_folder(), "trash"),
        MutationAction::Move { folder } => {
            let remote = remote_folder(account, folder)?;
            let local = local_folder_for_move(account, folder)?;
            TargetFolders::new(remote, local)
        }
        MutationAction::Expunge | MutationAction::Flag { .. } => TargetFolders::none(),
    })
}

fn remote_folder(account: &Account, folder: &str) -> Result<String, VivariumError> {
    Ok(match folder.to_ascii_lowercase().as_str() {
        "inbox" => account.inbox_folder(),
        "archive" | "all" => account.archive_folder(),
        "trash" | "deleted" => account.trash_folder(),
        "sent" => account.sent_folder(),
        "draft" | "drafts" => account.drafts_folder(),
        _ if folder == account.inbox_folder() => account.inbox_folder(),
        _ if folder == account.archive_folder() => account.archive_folder(),
        _ if folder == account.trash_folder() => account.trash_folder(),
        _ if folder == account.sent_folder() => account.sent_folder(),
        _ if folder == account.drafts_folder() => account.drafts_folder(),
        _ => return Err(unsupported_folder(folder)),
    })
}

fn local_folder_for_move(account: &Account, folder: &str) -> Result<&'static str, VivariumError> {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" => Ok("inbox"),
        "archive" | "all" => Ok("archive"),
        "trash" | "deleted" => Ok("trash"),
        "sent" => Ok("sent"),
        "draft" | "drafts" => Ok("drafts"),
        _ if folder == account.inbox_folder() => Ok("inbox"),
        _ if folder == account.archive_folder() => Ok("archive"),
        _ if folder == account.trash_folder() => Ok("trash"),
        _ if folder == account.sent_folder() => Ok("sent"),
        _ if folder == account.drafts_folder() => Ok("drafts"),
        _ => Err(unsupported_folder(folder)),
    }
}

fn unsupported_folder(folder: &str) -> VivariumError {
    VivariumError::Message(format!(
        "unsupported local mirror folder '{folder}'; expected inbox, archive, trash, sent, or drafts"
    ))
}

fn command_path(plan: &MutationPlan) -> String {
    match plan {
        MutationPlan::Move { command_path, .. } => match command_path {
            crate::imap::CommandPath::UidMove => "UID MOVE",
            crate::imap::CommandPath::CopyStoreDeletedUidExpunge => {
                "UID COPY + UID STORE + UID EXPUNGE"
            }
        },
        MutationPlan::Flag { .. } => "UID STORE",
        MutationPlan::HardExpunge => "UID EXPUNGE",
    }
    .into()
}

fn is_read(entry: &CatalogEntry) -> bool {
    entry.read_state
}

fn is_starred(entry: &CatalogEntry) -> bool {
    entry.starred
}

fn updated_read_state(entry: &CatalogEntry, mutation: &FlagMutation) -> bool {
    match mutation {
        FlagMutation::Read => true,
        FlagMutation::Unread => false,
        FlagMutation::Starred | FlagMutation::Unstarred => is_read(entry),
    }
}

fn updated_starred_state(entry: &CatalogEntry, mutation: &FlagMutation) -> bool {
    match mutation {
        FlagMutation::Starred => true,
        FlagMutation::Unstarred => false,
        FlagMutation::Read | FlagMutation::Unread => is_starred(entry),
    }
}

struct TargetFolders {
    remote: Option<String>,
    local: Option<String>,
}

impl TargetFolders {
    fn new(remote: String, local: &str) -> Self {
        Self {
            remote: Some(remote),
            local: Some(local.into()),
        }
    }

    fn none() -> Self {
        Self {
            remote: None,
            local: None,
        }
    }
}

impl MutationAction {
    fn name(&self) -> &'static str {
        match self {
            MutationAction::Archive => "archive",
            MutationAction::Trash => "trash",
            MutationAction::Expunge => "expunge",
            MutationAction::Move { .. } => "move",
            MutationAction::Flag {
                mutation: FlagMutation::Read,
            } => "flag_read",
            MutationAction::Flag {
                mutation: FlagMutation::Unread,
            } => "flag_unread",
            MutationAction::Flag {
                mutation: FlagMutation::Starred,
            } => "flag_star",
            MutationAction::Flag {
                mutation: FlagMutation::Unstarred,
            } => "flag_unstar",
        }
    }
}

#[cfg(test)]
mod tests;
