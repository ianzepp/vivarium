use futures::TryStreamExt;

use super::folders::CapabilityReport;
use super::transport::connect;
use crate::catalog::RemoteIdentity;
use crate::config::Account;
use crate::error::VivariumError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationCapabilities {
    pub move_supported: bool,
    pub uidplus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationTarget {
    Archive(String),
    Trash(String),
    Folder(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlagMutation {
    Read,
    Unread,
    Starred,
    Unstarred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationPlan {
    Move {
        destination: String,
        command_path: CommandPath,
    },
    Flag {
        mutation: FlagMutation,
        store_query: &'static str,
    },
    HardExpunge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPath {
    UidMove,
    CopyStoreDeletedUidExpunge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult {
    pub source_mailbox: String,
    pub destination_mailbox: Option<String>,
    pub uid: u32,
    pub uidvalidity: u32,
    pub command_path: String,
    pub reconciliation: ReconciliationAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationAction {
    RefreshRemoteIdentity,
    RefreshFlags,
    RemoveLocalCopy,
}

impl From<&CapabilityReport> for MutationCapabilities {
    fn from(report: &CapabilityReport) -> Self {
        Self {
            move_supported: report.move_supported,
            uidplus: report.uidplus,
        }
    }
}

pub fn plan_move(
    target: MutationTarget,
    capabilities: &MutationCapabilities,
) -> Result<MutationPlan, VivariumError> {
    let destination = target.destination();
    if capabilities.move_supported {
        return Ok(MutationPlan::Move {
            destination,
            command_path: CommandPath::UidMove,
        });
    }
    if capabilities.uidplus {
        return Ok(MutationPlan::Move {
            destination,
            command_path: CommandPath::CopyStoreDeletedUidExpunge,
        });
    }
    Err(VivariumError::Imap(
        "server lacks MOVE and UIDPLUS; refusing unsafe expunge fallback".into(),
    ))
}

pub fn plan_flag(mutation: FlagMutation) -> MutationPlan {
    let store_query = match mutation {
        FlagMutation::Read => "+FLAGS.SILENT (\\Seen)",
        FlagMutation::Unread => "-FLAGS.SILENT (\\Seen)",
        FlagMutation::Starred => "+FLAGS.SILENT (\\Flagged)",
        FlagMutation::Unstarred => "-FLAGS.SILENT (\\Flagged)",
    };
    MutationPlan::Flag {
        mutation,
        store_query,
    }
}

pub async fn move_message(
    account: &Account,
    remote: &RemoteIdentity,
    target: MutationTarget,
    capabilities: &MutationCapabilities,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    let plan = plan_move(target, capabilities)?;
    execute_plan(account, remote, plan, reject_invalid_certs).await
}

pub async fn mutate_flag(
    account: &Account,
    remote: &RemoteIdentity,
    mutation: FlagMutation,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    execute_plan(account, remote, plan_flag(mutation), reject_invalid_certs).await
}

pub async fn hard_expunge(
    account: &Account,
    remote: &RemoteIdentity,
    capabilities: &MutationCapabilities,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    if !capabilities.uidplus {
        return Err(VivariumError::Imap(
            "hard expunge requires UIDPLUS for scoped UID EXPUNGE".into(),
        ));
    }
    execute_plan(
        account,
        remote,
        MutationPlan::HardExpunge,
        reject_invalid_certs,
    )
    .await
}

async fn execute_plan(
    account: &Account,
    remote: &RemoteIdentity,
    plan: MutationPlan,
    reject_invalid_certs: bool,
) -> Result<MutationResult, VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    let mailbox = session
        .select(&remote.remote_mailbox)
        .await
        .map_err(|e| VivariumError::Imap(format!("select failed: {e}")))?;
    verify_uidvalidity(remote, mailbox.uid_validity)?;
    let result = execute_selected(&mut session, remote, plan).await;
    session.logout().await.ok();
    result
}

async fn execute_selected(
    session: &mut super::transport::ImapSession,
    remote: &RemoteIdentity,
    plan: MutationPlan,
) -> Result<MutationResult, VivariumError> {
    match plan {
        MutationPlan::Move {
            destination,
            command_path,
        } => execute_move(session, remote, destination, command_path).await,
        MutationPlan::Flag { store_query, .. } => {
            let updates = session
                .uid_store(remote.uid.to_string(), store_query)
                .await
                .map_err(|e| VivariumError::Imap(format!("UID STORE failed: {e}")))?;
            let _: Vec<_> = updates
                .try_collect()
                .await
                .map_err(|e| VivariumError::Imap(format!("UID STORE stream failed: {e}")))?;
            Ok(result(
                remote,
                None,
                "UID STORE",
                ReconciliationAction::RefreshFlags,
            ))
        }
        MutationPlan::HardExpunge => {
            let expunged = session
                .uid_expunge(remote.uid.to_string())
                .await
                .map_err(|e| VivariumError::Imap(format!("UID EXPUNGE failed: {e}")))?;
            let _: Vec<_> = expunged
                .try_collect()
                .await
                .map_err(|e| VivariumError::Imap(format!("UID EXPUNGE stream failed: {e}")))?;
            Ok(result(
                remote,
                None,
                "UID EXPUNGE",
                ReconciliationAction::RemoveLocalCopy,
            ))
        }
    }
}

async fn execute_move(
    session: &mut super::transport::ImapSession,
    remote: &RemoteIdentity,
    destination: String,
    command_path: CommandPath,
) -> Result<MutationResult, VivariumError> {
    match command_path {
        CommandPath::UidMove => {
            session
                .uid_mv(remote.uid.to_string(), &destination)
                .await
                .map_err(|e| VivariumError::Imap(format!("UID MOVE failed: {e}")))?;
        }
        CommandPath::CopyStoreDeletedUidExpunge => {
            session
                .uid_copy(remote.uid.to_string(), &destination)
                .await
                .map_err(|e| VivariumError::Imap(format!("UID COPY failed: {e}")))?;
            let updates = session
                .uid_store(remote.uid.to_string(), "+FLAGS.SILENT (\\Deleted)")
                .await
                .map_err(|e| VivariumError::Imap(format!("UID STORE deleted failed: {e}")))?;
            let _: Vec<_> = updates.try_collect().await.map_err(|e| {
                VivariumError::Imap(format!("UID STORE deleted stream failed: {e}"))
            })?;
            let expunged = session
                .uid_expunge(remote.uid.to_string())
                .await
                .map_err(|e| VivariumError::Imap(format!("UID EXPUNGE failed: {e}")))?;
            let _: Vec<_> = expunged
                .try_collect()
                .await
                .map_err(|e| VivariumError::Imap(format!("UID EXPUNGE stream failed: {e}")))?;
        }
    }
    Ok(result(
        remote,
        Some(destination),
        command_path.name(),
        ReconciliationAction::RefreshRemoteIdentity,
    ))
}

fn verify_uidvalidity(remote: &RemoteIdentity, current: Option<u32>) -> Result<(), VivariumError> {
    match current {
        Some(value) if value == remote.uidvalidity => Ok(()),
        Some(value) => Err(VivariumError::Imap(format!(
            "stale remote reference for {}: stored UIDVALIDITY {}, current {}",
            remote.remote_mailbox, remote.uidvalidity, value
        ))),
        None => Err(VivariumError::Imap(format!(
            "server did not return UIDVALIDITY for {}",
            remote.remote_mailbox
        ))),
    }
}

fn result(
    remote: &RemoteIdentity,
    destination: Option<String>,
    command_path: &str,
    reconciliation: ReconciliationAction,
) -> MutationResult {
    MutationResult {
        source_mailbox: remote.remote_mailbox.clone(),
        destination_mailbox: destination,
        uid: remote.uid,
        uidvalidity: remote.uidvalidity,
        command_path: command_path.into(),
        reconciliation,
    }
}

impl MutationTarget {
    fn destination(self) -> String {
        match self {
            MutationTarget::Archive(folder)
            | MutationTarget::Trash(folder)
            | MutationTarget::Folder(folder) => folder,
        }
    }
}

impl CommandPath {
    fn name(&self) -> &'static str {
        match self {
            CommandPath::UidMove => "UID MOVE",
            CommandPath::CopyStoreDeletedUidExpunge => "UID COPY + UID STORE + UID EXPUNGE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_uid_move_when_supported() {
        let plan = plan_move(
            MutationTarget::Archive("Archive".into()),
            &MutationCapabilities {
                move_supported: true,
                uidplus: true,
            },
        )
        .unwrap();

        assert_eq!(
            plan,
            MutationPlan::Move {
                destination: "Archive".into(),
                command_path: CommandPath::UidMove
            }
        );
    }

    #[test]
    fn plans_copy_delete_uid_expunge_fallback_only_with_uidplus() {
        let plan = plan_move(
            MutationTarget::Trash("Trash".into()),
            &MutationCapabilities {
                move_supported: false,
                uidplus: true,
            },
        )
        .unwrap();

        assert_eq!(
            plan,
            MutationPlan::Move {
                destination: "Trash".into(),
                command_path: CommandPath::CopyStoreDeletedUidExpunge
            }
        );
    }

    #[test]
    fn refuses_unsafe_move_fallback_without_uidplus() {
        let err = plan_move(
            MutationTarget::Archive("Archive".into()),
            &MutationCapabilities {
                move_supported: false,
                uidplus: false,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("refusing unsafe expunge"));
    }

    #[test]
    fn plans_flag_store_queries() {
        assert_eq!(
            plan_flag(FlagMutation::Read),
            MutationPlan::Flag {
                mutation: FlagMutation::Read,
                store_query: "+FLAGS.SILENT (\\Seen)"
            }
        );
        assert_eq!(
            plan_flag(FlagMutation::Unstarred),
            MutationPlan::Flag {
                mutation: FlagMutation::Unstarred,
                store_query: "-FLAGS.SILENT (\\Flagged)"
            }
        );
    }

    #[test]
    fn rejects_stale_uidvalidity() {
        let remote = remote_identity();
        let err = verify_uidvalidity(&remote, Some(8)).unwrap_err();

        assert!(err.to_string().contains("stale remote reference"));
    }

    fn remote_identity() -> RemoteIdentity {
        RemoteIdentity {
            account: "acct".into(),
            provider: "protonmail".into(),
            remote_mailbox: "INBOX".into(),
            local_folder: "inbox".into(),
            uid: 42,
            uidvalidity: 7,
            rfc_message_id: "m@example.com".into(),
            size: 123,
            content_fingerprint: "abc".into(),
        }
    }
}
