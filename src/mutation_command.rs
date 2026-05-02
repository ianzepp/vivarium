use std::path::Path;

use serde::Serialize;

use crate::catalog::{Catalog, CatalogEntry, RemoteIdentity};
use crate::config::Account;
use crate::error::VivariumError;
use crate::imap::{FlagMutation, MutationCapabilities, MutationPlan, MutationTarget};
use crate::store::{MailStore, message_id_from_path};

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
    pub raw_path: Option<String>,
    pub folder: Option<String>,
    pub maildir_subdir: Option<String>,
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
    let local_message_id = message_id_from_path(Path::new(&entry.raw_path)).ok_or_else(|| {
        VivariumError::Message(format!(
            "catalog entry has no local message id: {}",
            entry.handle
        ))
    })?;
    let plan = mutation_plan(account, &action, capabilities)?;
    let target = target_folders(account, &action)?;
    let preview = MutationPreview {
        account: account.name.clone(),
        handle: entry.handle.clone(),
        input: input.into(),
        local_message_id,
        operation: action.name().into(),
        source_mailbox: remote.remote_mailbox.clone(),
        source_local_folder: entry.folder.clone(),
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
    store: &MailStore,
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
    let dst = store.move_message(
        &prepared.preview.local_message_id,
        &prepared.entry.folder,
        target,
    )?;
    let subdir = maildir_subdir(&dst)?;
    let folder = canonical_local_folder(target).to_string();
    catalog.update_local_location(
        &prepared.preview.account,
        &prepared.preview.handle,
        dst.to_string_lossy().to_string(),
        folder.clone(),
        subdir.clone(),
        None,
    )?;
    Ok(LocalReconciliation {
        action: "move_local_copy".into(),
        raw_path: Some(dst.to_string_lossy().to_string()),
        folder: Some(folder),
        maildir_subdir: Some(subdir),
    })
}

fn reconcile_expunge(
    store: &MailStore,
    catalog: &mut Catalog,
    prepared: &PreparedMutation,
) -> Result<LocalReconciliation, VivariumError> {
    store.remove_message(&prepared.preview.local_message_id, &prepared.entry.folder)?;
    catalog.remove_entry(&prepared.preview.account, &prepared.preview.handle)?;
    Ok(LocalReconciliation {
        action: "remove_local_copy".into(),
        raw_path: None,
        folder: None,
        maildir_subdir: None,
    })
}

fn reconcile_flag(
    store: &MailStore,
    catalog: &mut Catalog,
    prepared: &PreparedMutation,
    mutation: &FlagMutation,
) -> Result<LocalReconciliation, VivariumError> {
    let (flag, enabled) = flag_change(mutation);
    let dst = store.set_message_flag(
        &prepared.preview.local_message_id,
        &prepared.entry.folder,
        flag,
        enabled,
    )?;
    let subdir = maildir_subdir(&dst)?;
    catalog.update_local_location(
        &prepared.preview.account,
        &prepared.preview.handle,
        dst.to_string_lossy().to_string(),
        prepared.entry.folder.clone(),
        subdir.clone(),
        prepared.entry.remote.clone(),
    )?;
    Ok(LocalReconciliation {
        action: "refresh_local_flags".into(),
        raw_path: Some(dst.to_string_lossy().to_string()),
        folder: Some(prepared.entry.folder.clone()),
        maildir_subdir: Some(subdir),
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

fn flag_change(mutation: &FlagMutation) -> (char, bool) {
    match mutation {
        FlagMutation::Read => ('S', true),
        FlagMutation::Unread => ('S', false),
        FlagMutation::Starred => ('F', true),
        FlagMutation::Unstarred => ('F', false),
    }
}

fn canonical_local_folder(folder: &str) -> &'static str {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" | "new" => "INBOX",
        "archive" | "archives" | "all" => "Archive",
        "trash" | "deleted" => "Trash",
        "sent" => "Sent",
        "draft" | "drafts" => "Drafts",
        _ => "INBOX",
    }
}

fn maildir_subdir(path: &Path) -> Result<String, VivariumError> {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| VivariumError::Message("message path has no maildir subdir".into()))
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
