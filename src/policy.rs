//! Account mutation policy enforcement.
//!
//! Remote side effects are authorized by the selected account's capabilities
//! at execution time. Command names, queue provenance, and provider folder
//! aliases are never authorization. Local mailspace operations are separate
//! from external account mutation policy.

use crate::VivariumError;
use crate::config::{Account, MutationPolicy};
use crate::queue::QueuedCommand;

/// Classified remote mutation kind, used for policy enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteMutation {
    Archive,
    MoveToTrash,
    MoveOther,
    Expunge,
    Flag,
    Send,
    AppendDraft,
}

impl std::fmt::Display for RemoteMutation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteMutation::Archive => write!(f, "archive"),
            RemoteMutation::MoveToTrash => write!(f, "move-to-trash"),
            RemoteMutation::MoveOther => write!(f, "move"),
            RemoteMutation::Expunge => write!(f, "expunge"),
            RemoteMutation::Flag => write!(f, "flag"),
            RemoteMutation::Send => write!(f, "send"),
            RemoteMutation::AppendDraft => write!(f, "append-draft"),
        }
    }
}

/// Normalize folder aliases and check whether a folder resolves to the
/// account's trash/deleted folder.
///
/// Aliases normalized: `"trash"`, `"deleted"` (case-insensitive), and the
/// account's configured provider trash folder name all classify as trash.
#[must_use]
pub fn classifies_as_trash(account: &Account, folder: &str) -> bool {
    let lower = folder.to_ascii_lowercase();
    lower == "trash" || lower == "deleted" || lower == account.trash_folder().to_ascii_lowercase()
}

/// Classify a queued command into the remote mutation kind it represents.
///
/// Returns `None` for commands that cause no remote side effects (e.g. local
/// reply draft creation) and are therefore not subject to mutation policy.
#[must_use]
pub fn classify(account: &Account, command: &QueuedCommand) -> Option<RemoteMutation> {
    match command {
        QueuedCommand::Archive { .. } => Some(RemoteMutation::Archive),
        QueuedCommand::Delete { expunge, .. } => Some(if *expunge {
            RemoteMutation::Expunge
        } else {
            RemoteMutation::MoveToTrash
        }),
        QueuedCommand::Move { folder, .. } => Some(if classifies_as_trash(account, folder) {
            RemoteMutation::MoveToTrash
        } else {
            RemoteMutation::MoveOther
        }),
        QueuedCommand::Flag { .. } => Some(RemoteMutation::Flag),
        QueuedCommand::Send { .. } => Some(RemoteMutation::Send),
        QueuedCommand::Reply { .. } => None,
    }
}

/// Authorize a queued command against the selected account's mutation policy.
///
/// Must be called at both enqueue admission and queue execution to ensure
/// stale or manually constructed queued items cannot bypass policy.
///
/// # Errors
/// Returns a `VivariumError::Policy` error if the command's mutation is
/// denied by the account's policy.
pub fn authorize(account: &Account, command: &QueuedCommand) -> Result<(), VivariumError> {
    let Some(mutation) = classify(account, command) else {
        return Ok(());
    };
    authorize_mutation(account, mutation)
}

/// Authorize a raw remote mutation against the selected account's policy.
///
/// Use this for remote write paths that do not flow through `QueuedCommand`
/// (e.g. `--append-remote` IMAP APPEND, outbox auto-send).
///
/// # Errors
/// Returns a `VivariumError::Policy` error if the mutation is denied by the
/// account's policy.
pub fn authorize_mutation(
    account: &Account,
    mutation: RemoteMutation,
) -> Result<(), VivariumError> {
    if policy_allows(account.policy, mutation) {
        Ok(())
    } else {
        Err(VivariumError::Policy(format!(
            "account '{}' has policy '{}' which denies {}",
            account.name, account.policy, mutation
        )))
    }
}

/// Whether a given policy permits a given remote mutation.
#[must_use]
pub fn policy_allows(policy: MutationPolicy, mutation: RemoteMutation) -> bool {
    match policy {
        MutationPolicy::FullWrite => true,
        MutationPolicy::ReadOnly => false,
        MutationPolicy::Archive => matches!(
            mutation,
            RemoteMutation::Archive | RemoteMutation::MoveOther | RemoteMutation::Flag
        ),
    }
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
