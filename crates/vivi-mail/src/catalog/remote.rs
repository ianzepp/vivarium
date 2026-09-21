use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Catalog, CatalogEntry, canonical_folder};
use crate::error::VivariumError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteIdentity {
    pub account: String,
    pub provider: String,
    pub remote_mailbox: String,
    pub local_folder: String,
    pub uid: u32,
    pub uidvalidity: u32,
    pub rfc_message_id: String,
    pub size: u64,
    pub content_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteIdentityCandidate {
    pub account: String,
    pub provider: String,
    pub remote_mailbox: String,
    pub local_folder: String,
    pub uid: u32,
    pub uidvalidity: Option<u32>,
    pub rfc_message_id: Option<String>,
    pub size: u64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RemoteIdentityAttachResult {
    pub matched: usize,
    pub missing_uidvalidity: usize,
    pub missing_local: usize,
    pub ambiguous: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteReferenceStatus {
    Ready(RemoteIdentity),
    MissingHandle {
        account: String,
        handle: String,
    },
    MissingRemoteIdentity {
        account: String,
        handle: String,
    },
    StaleUidValidity {
        account: String,
        handle: String,
        stored_uidvalidity: u32,
        current_uidvalidity: u32,
    },
}

impl Catalog {
    pub fn remote_reference_status(
        &self,
        account: &str,
        handle: &str,
        current_uidvalidity: Option<u32>,
    ) -> RemoteReferenceStatus {
        let Some(entry) = self.entry(account, handle) else {
            return RemoteReferenceStatus::MissingHandle {
                account: account.to_string(),
                handle: handle.to_string(),
            };
        };
        let Some(remote) = entry.remote.clone() else {
            return RemoteReferenceStatus::MissingRemoteIdentity {
                account: account.to_string(),
                handle: handle.to_string(),
            };
        };
        if let Some(current) = current_uidvalidity
            && remote.uidvalidity != current
        {
            return RemoteReferenceStatus::StaleUidValidity {
                account: account.to_string(),
                handle: handle.to_string(),
                stored_uidvalidity: remote.uidvalidity,
                current_uidvalidity: current,
            };
        }
        RemoteReferenceStatus::Ready(remote)
    }

    /// Resolve a remote identity reference for a catalog entry.
    ///
    /// # Errors
    /// Returns an error if the message or its remote identity is not found.
    pub fn remote_reference(
        &self,
        account: &str,
        handle: &str,
    ) -> Result<RemoteIdentity, VivariumError> {
        match self.remote_reference_status(account, handle, None) {
            RemoteReferenceStatus::Ready(remote) => Ok(remote),
            RemoteReferenceStatus::MissingHandle { .. } => Err(VivariumError::Message(format!(
                "message not found in catalog for account '{account}': {handle}"
            ))),
            RemoteReferenceStatus::MissingRemoteIdentity { .. } => Err(VivariumError::Message(
                format!("message has no remote identity yet: {handle}"),
            )),
            RemoteReferenceStatus::StaleUidValidity { .. } => unreachable!(),
        }
    }

    /// Attach remote identity information to catalog entries based on candidates.
    ///
    /// # Errors
    /// Returns an error if the database update fails.
    pub fn attach_remote_identities(
        &mut self,
        candidates: &[RemoteIdentityCandidate],
    ) -> Result<RemoteIdentityAttachResult, VivariumError> {
        let mut result = RemoteIdentityAttachResult::default();

        for candidate in candidates {
            let Some(uidvalidity) = candidate.uidvalidity else {
                result.missing_uidvalidity += 1;
                continue;
            };
            let matching = self.matching_handles(candidate);
            match matching.as_slice() {
                [handle] => {
                    if let Some(entry) = self.entry(&candidate.account, handle) {
                        let remote = remote_identity_for_entry(&entry, candidate, uidvalidity);
                        self.conn
                            .execute(
                                "INSERT INTO remote_bindings (
                                   message_id, account, provider, remote_mailbox, remote_uid,
                                   remote_uidvalidity, last_verified_at, stale
                                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP, 0)
                                 ON CONFLICT(message_id) DO UPDATE SET
                                   account = excluded.account,
                                   provider = excluded.provider,
                                   remote_mailbox = excluded.remote_mailbox,
                                   remote_uid = excluded.remote_uid,
                                   remote_uidvalidity = excluded.remote_uidvalidity,
                                   last_verified_at = excluded.last_verified_at,
                                   stale = 0",
                                params![
                                    handle,
                                    remote.account,
                                    remote.provider,
                                    remote.remote_mailbox,
                                    remote.uid,
                                    remote.uidvalidity,
                                ],
                            )
                            .map_err(|e| {
                                VivariumError::Other(format!(
                                    "failed to update remote identity: {e}"
                                ))
                            })?;
                        result.matched += 1;
                    }
                }
                [] => result.missing_local += 1,
                _ => result.ambiguous += 1,
            }
        }

        if result.matched > 0 {
            Self::flush();
        }
        Ok(result)
    }

    fn matching_handles(&self, candidate: &RemoteIdentityCandidate) -> Vec<String> {
        let folder = canonical_folder(&candidate.local_folder);
        if let Some(matches) = rfc_matches(self, candidate, folder)
            && !matches.is_empty()
        {
            return matches;
        }
        Vec::new()
    }
}

/// Open the catalog and attach remote identity information to entries.
///
/// # Errors
/// Returns an error if the catalog cannot be opened or the database update fails.
pub fn attach_remote_identities(
    mail_root: &Path,
    candidates: &[RemoteIdentityCandidate],
) -> Result<RemoteIdentityAttachResult, VivariumError> {
    let mut catalog = Catalog::open(mail_root)?;
    catalog.attach_remote_identities(candidates)
}

fn rfc_matches(
    catalog: &Catalog,
    candidate: &RemoteIdentityCandidate,
    folder: &str,
) -> Option<Vec<String>> {
    candidate.rfc_message_id.as_ref().map(|rfc_message_id| {
        let local_role = super::local_role_from_folder(folder);
        let mut stmt = match catalog.conn.prepare(
            "SELECT m.message_id
             FROM messages m
             JOIN message_metadata md ON md.content_id = m.content_id
             WHERE m.account = ?1
               AND m.local_role = ?2
               AND m.deleted_at IS NULL
               AND md.normalized_message_id = ?3
             ORDER BY m.message_id",
        ) {
            Ok(stmt) => stmt,
            Err(e) => {
                tracing::warn!("failed to prepare remote rfc match query: {e}");
                return Vec::new();
            }
        };
        let rows = match stmt.query_map(
            params![candidate.account, local_role, rfc_message_id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("failed to query remote rfc matches: {e}");
                return Vec::new();
            }
        };
        rows.filter_map(|row| {
            row.map_err(|e| tracing::warn!("failed to read remote rfc match: {e}"))
                .ok()
        })
        .collect()
    })
}

fn remote_identity_for_entry(
    entry: &CatalogEntry,
    candidate: &RemoteIdentityCandidate,
    uidvalidity: u32,
) -> RemoteIdentity {
    RemoteIdentity {
        account: candidate.account.clone(),
        provider: candidate.provider.clone(),
        remote_mailbox: candidate.remote_mailbox.clone(),
        local_folder: candidate.local_folder.clone(),
        uid: candidate.uid,
        uidvalidity,
        rfc_message_id: candidate.rfc_message_id.clone().unwrap_or_default(),
        size: candidate.size,
        content_fingerprint: entry.content_id.clone(),
    }
}
