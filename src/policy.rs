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
pub fn classifies_as_trash(account: &Account, folder: &str) -> bool {
    let lower = folder.to_ascii_lowercase();
    lower == "trash" || lower == "deleted" || lower == account.trash_folder().to_ascii_lowercase()
}

/// Classify a queued command into the remote mutation kind it represents.
///
/// Returns `None` for commands that cause no remote side effects (e.g. local
/// reply draft creation) and are therefore not subject to mutation policy.
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
mod tests {
    use super::*;

    fn account_with_policy(name: &str, policy: MutationPolicy) -> Account {
        Account {
            name: name.into(),
            email: "u@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: None,
            imap_security: None,
            smtp_host: "localhost".into(),
            smtp_port: None,
            smtp_security: None,
            username: "u".into(),
            auth: crate::config::Auth::Password,
            password: Some("pw".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            mail_dir: None,
            inbox_folder: None,
            archive_folder: None,
            trash_folder: None,
            sent_folder: None,
            drafts_folder: None,
            label_roots: None,
            storage_mode: None,
            provider: crate::config::Provider::Standard,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
            policy,
        }
    }

    fn gmail_account_with_policy(policy: MutationPolicy) -> Account {
        let mut acct = account_with_policy("gmail-acct", policy);
        acct.provider = crate::config::Provider::Gmail;
        acct
    }

    #[test]
    fn full_write_allows_all_mutations() {
        let acct = account_with_policy("fw", MutationPolicy::FullWrite);
        let mutations = [
            RemoteMutation::Archive,
            RemoteMutation::MoveToTrash,
            RemoteMutation::MoveOther,
            RemoteMutation::Expunge,
            RemoteMutation::Flag,
            RemoteMutation::Send,
            RemoteMutation::AppendDraft,
        ];
        for m in mutations {
            assert!(policy_allows(acct.policy, m), "{m} should be allowed");
        }
    }

    #[test]
    fn read_only_denies_all_mutations() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let mutations = [
            RemoteMutation::Archive,
            RemoteMutation::MoveToTrash,
            RemoteMutation::MoveOther,
            RemoteMutation::Expunge,
            RemoteMutation::Flag,
            RemoteMutation::Send,
            RemoteMutation::AppendDraft,
        ];
        for m in mutations {
            assert!(!policy_allows(acct.policy, m), "{m} should be denied");
        }
    }

    #[test]
    fn archive_policy_allows_non_destructive_denies_destructive() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        assert!(policy_allows(acct.policy, RemoteMutation::Archive));
        assert!(policy_allows(acct.policy, RemoteMutation::MoveOther));
        assert!(policy_allows(acct.policy, RemoteMutation::Flag));
        assert!(!policy_allows(acct.policy, RemoteMutation::MoveToTrash));
        assert!(!policy_allows(acct.policy, RemoteMutation::Expunge));
        assert!(!policy_allows(acct.policy, RemoteMutation::Send));
        assert!(!policy_allows(acct.policy, RemoteMutation::AppendDraft));
    }

    #[test]
    fn authorize_rejects_read_only_delete() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let cmd = QueuedCommand::Delete {
            handles: vec!["h1".into()],
            expunge: false,
            confirm: false,
        };
        let err = authorize(&acct, &cmd).unwrap_err();
        assert!(err.to_string().contains("policy"));
        assert!(err.to_string().contains("read-only"));
    }

    #[test]
    fn authorize_allows_full_write_delete() {
        let acct = account_with_policy("fw", MutationPolicy::FullWrite);
        let cmd = QueuedCommand::Delete {
            handles: vec!["h1".into()],
            expunge: false,
            confirm: false,
        };
        assert!(authorize(&acct, &cmd).is_ok());
    }

    #[test]
    fn authorize_passes_reply_as_local_operation() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let cmd = QueuedCommand::Reply {
            handle: "h1".into(),
            body: "thanks".into(),
        };
        assert!(authorize(&acct, &cmd).is_ok());
    }

    #[test]
    fn classifies_move_to_trash_folder_as_trash() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        assert!(classifies_as_trash(&acct, "trash"));
        assert!(classifies_as_trash(&acct, "Trash"));
        assert!(classifies_as_trash(&acct, "TRASH"));
        assert!(classifies_as_trash(&acct, "deleted"));
        assert!(classifies_as_trash(&acct, "Deleted"));
    }

    #[test]
    fn classifies_provider_trash_folder_as_trash() {
        let acct = gmail_account_with_policy(MutationPolicy::Archive);
        // Gmail's default trash folder
        assert!(classifies_as_trash(&acct, "[Gmail]/Trash"));
        // The account's resolved trash folder
        assert!(classifies_as_trash(&acct, &acct.trash_folder()));
    }

    #[test]
    fn classifies_mixed_case_provider_trash_folder_as_trash() {
        let mut acct = account_with_policy("ar", MutationPolicy::Archive);
        acct.trash_folder = Some("My Trash Folder".into());
        assert!(classifies_as_trash(&acct, "My Trash Folder"));
        assert!(classifies_as_trash(&acct, "my trash folder"));
        assert!(classifies_as_trash(&acct, "MY TRASH FOLDER"));
    }

    #[test]
    fn classifies_gmail_trash_case_insensitively() {
        let acct = gmail_account_with_policy(MutationPolicy::Archive);
        // Gmail's default trash folder is "[Gmail]/Trash"
        assert!(classifies_as_trash(&acct, "[gmail]/trash"));
        assert!(classifies_as_trash(&acct, "[GMAIL]/TRASH"));
    }

    #[test]
    fn does_not_classify_non_trash_as_trash() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        assert!(!classifies_as_trash(&acct, "archive"));
        assert!(!classifies_as_trash(&acct, "inbox"));
        assert!(!classifies_as_trash(&acct, "sent"));
    }

    #[test]
    fn classify_move_to_trash_denied_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let cmd = QueuedCommand::Move {
            handle: "h1".into(),
            folder: "trash".into(),
        };
        let err = authorize(&acct, &cmd).unwrap_err();
        assert!(err.to_string().contains("move-to-trash"));
    }

    #[test]
    fn classify_move_to_archive_allowed_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let cmd = QueuedCommand::Move {
            handle: "h1".into(),
            folder: "archive".into(),
        };
        assert!(authorize(&acct, &cmd).is_ok());
    }

    #[test]
    fn classify_expunge_delete_denied_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let cmd = QueuedCommand::Delete {
            handles: vec!["h1".into()],
            expunge: true,
            confirm: true,
        };
        let err = authorize(&acct, &cmd).unwrap_err();
        assert!(err.to_string().contains("expunge"));
    }

    #[test]
    fn classify_send_denied_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let cmd = QueuedCommand::Send {
            path: std::path::PathBuf::from("test.eml"),
            from: None,
        };
        let err = authorize(&acct, &cmd).unwrap_err();
        assert!(err.to_string().contains("send"));
    }

    #[test]
    fn classify_send_denied_by_read_only_policy() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let cmd = QueuedCommand::Send {
            path: std::path::PathBuf::from("test.eml"),
            from: None,
        };
        assert!(authorize(&acct, &cmd).is_err());
    }

    #[test]
    fn classify_flag_denied_by_read_only_policy() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let cmd = QueuedCommand::Flag {
            handle: "h1".into(),
            read: true,
            unread: false,
            star: false,
            unstar: false,
        };
        assert!(authorize(&acct, &cmd).is_err());
    }

    #[test]
    fn classify_flag_allowed_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let cmd = QueuedCommand::Flag {
            handle: "h1".into(),
            read: true,
            unread: false,
            star: false,
            unstar: false,
        };
        assert!(authorize(&acct, &cmd).is_ok());
    }

    // --- Regression: stale/manually constructed queued items must not bypass policy ---

    #[test]
    fn stale_queued_delete_denied_by_read_only_policy() {
        // Simulates a queued delete from before policy was set.
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let stale_cmd = QueuedCommand::Delete {
            handles: vec!["old-handle".into()],
            expunge: false,
            confirm: true, // confirm was stored but policy now denies
        };
        let err = authorize(&acct, &stale_cmd).unwrap_err();
        assert!(err.to_string().contains("read-only"));
        assert!(err.to_string().contains("move-to-trash"));
    }

    #[test]
    fn stale_queued_expunge_denied_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let stale_cmd = QueuedCommand::Delete {
            handles: vec!["old-handle".into()],
            expunge: true,
            confirm: true,
        };
        let err = authorize(&acct, &stale_cmd).unwrap_err();
        assert!(err.to_string().contains("expunge"));
    }

    #[test]
    fn stale_queued_send_denied_by_read_only_policy() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let stale_cmd = QueuedCommand::Send {
            path: std::path::PathBuf::from("old-draft.eml"),
            from: None,
        };
        assert!(authorize(&acct, &stale_cmd).is_err());
    }

    #[test]
    fn stale_queued_move_to_trash_denied_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        let stale_cmd = QueuedCommand::Move {
            handle: "old-handle".into(),
            folder: "deleted".into(),
        };
        let err = authorize(&acct, &stale_cmd).unwrap_err();
        assert!(err.to_string().contains("move-to-trash"));
    }

    #[test]
    fn full_write_account_authorizes_all_queued_commands() {
        let acct = account_with_policy("fw", MutationPolicy::FullWrite);
        let commands = [
            QueuedCommand::Archive {
                handles: vec!["h".into()],
            },
            QueuedCommand::Delete {
                handles: vec!["h".into()],
                expunge: true,
                confirm: true,
            },
            QueuedCommand::Move {
                handle: "h".into(),
                folder: "trash".into(),
            },
            QueuedCommand::Flag {
                handle: "h".into(),
                read: true,
                unread: false,
                star: false,
                unstar: false,
            },
        ];
        for cmd in commands {
            assert!(authorize(&acct, &cmd).is_ok(), "should allow {:?}", cmd);
        }
    }

    // --- AppendDraft authorization ---

    #[test]
    fn append_draft_denied_by_read_only_policy() {
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);
        let err = authorize_mutation(&acct, RemoteMutation::AppendDraft).unwrap_err();
        assert!(err.to_string().contains("append-draft"));
    }

    #[test]
    fn append_draft_denied_by_archive_policy() {
        let acct = account_with_policy("ar", MutationPolicy::Archive);
        assert!(authorize_mutation(&acct, RemoteMutation::AppendDraft).is_err());
    }

    #[test]
    fn append_draft_allowed_by_full_write_policy() {
        let acct = account_with_policy("fw", MutationPolicy::FullWrite);
        assert!(authorize_mutation(&acct, RemoteMutation::AppendDraft).is_ok());
    }

    // --- End-to-end queue-run regression: persisted stale command rejected ---

    #[test]
    fn stale_queued_command_roundtrip_denied_at_execution_time() {
        use crate::queue::{self, QueueItem, QueueStatus};

        let tmp = tempfile::tempdir().unwrap();
        let acct = account_with_policy("ro", MutationPolicy::ReadOnly);

        // Persist a denied command as if it were enqueued before policy was set.
        let stale_cmd = QueuedCommand::Delete {
            handles: vec!["old-handle".into()],
            expunge: false,
            confirm: true,
        };
        let item = QueueItem::new("ro".into(), stale_cmd);
        queue::enqueue(tmp.path(), &item).unwrap();

        // Simulate what queue_run does: load from disk, then authorize.
        let loaded = queue::load(tmp.path(), &item.id).unwrap();
        assert_eq!(loaded.status, QueueStatus::Pending);
        assert_eq!(loaded.command, item.command);

        // The execution-time authorization gate must reject this.
        let err = authorize(&acct, &loaded.command).unwrap_err();
        assert!(err.to_string().contains("read-only"));
        assert!(err.to_string().contains("move-to-trash"));

        // Verify no provider call would be made: the error fires before
        // any dispatch in execute_queued.
        assert!(matches!(err, VivariumError::Policy(_)));
    }

    #[test]
    fn stale_queued_send_roundtrip_denied_at_execution_time() {
        use crate::queue::{self, QueueItem};

        let tmp = tempfile::tempdir().unwrap();
        let acct = account_with_policy("ar", MutationPolicy::Archive);

        let stale_cmd = QueuedCommand::Send {
            path: std::path::PathBuf::from("old-draft.eml"),
            from: None,
        };
        let item = QueueItem::new("ar".into(), stale_cmd);
        queue::enqueue(tmp.path(), &item).unwrap();

        let loaded = queue::load(tmp.path(), &item.id).unwrap();
        let err = authorize(&acct, &loaded.command).unwrap_err();
        assert!(err.to_string().contains("send"));
        assert!(matches!(err, VivariumError::Policy(_)));
    }
}
