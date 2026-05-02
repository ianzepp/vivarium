use crate::catalog::RemoteIdentityCandidate;
use crate::config::Account;
use crate::sync::SyncResult;

use super::transport::RemoteMessage;

pub(super) fn remote_identity_candidates(
    account: &Account,
    remote_folder: &str,
    local_folder: &str,
    remote_messages: &[RemoteMessage],
) -> Vec<RemoteIdentityCandidate> {
    remote_messages
        .iter()
        .map(|remote| RemoteIdentityCandidate {
            account: account.name.clone(),
            provider: account.provider.to_string(),
            remote_mailbox: remote_folder.to_string(),
            local_folder: local_folder.to_string(),
            uid: remote.uid,
            uidvalidity: remote.uidvalidity,
            rfc_message_id: remote.rfc_message_id.clone(),
            size: remote.size,
        })
        .collect()
}

pub(super) fn remote_identity_result(
    remote_identities: Vec<RemoteIdentityCandidate>,
) -> SyncResult {
    SyncResult {
        remote_identities,
        ..SyncResult::default()
    }
}
