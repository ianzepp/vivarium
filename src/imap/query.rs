use chrono::{Datelike, NaiveDate};
use futures::{TryStream, TryStreamExt};

use super::transport::{ImapSession, RemoteMessage, connect};
use crate::config::Account;
use crate::error::VivariumError;
use crate::message::message_id_from_bytes;
use crate::sync::SyncWindow;

const METADATA_FETCH_ITEMS: &str = "(UID RFC822.SIZE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])";

pub(super) async fn fetch_remote_messages(
    account: &Account,
    remote_folder: &str,
    reject_invalid_certs: bool,
    window: SyncWindow,
) -> Result<Vec<RemoteMessage>, VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    let mailbox = session
        .select(remote_folder)
        .await
        .map_err(|e| VivariumError::Imap(format!("select {remote_folder} failed: {e}")))?;

    let count = mailbox.exists;
    if count == 0 {
        tracing::info!(folder = remote_folder, "empty folder");
        session.logout().await.ok();
        return Ok(Vec::new());
    }

    tracing::info!(folder = remote_folder, count, "checking messages");
    let messages = if window.is_empty() {
        fetch_remote_metadata(&mut session, format!("1:{count}")).await?
    } else {
        let uid_set = remote_uid_set(&mut session, window).await?;
        if uid_set.is_empty() {
            session.logout().await.ok();
            return Ok(Vec::new());
        }
        uid_fetch_remote_metadata(&mut session, uid_set).await?
    };

    session.logout().await.ok();
    Ok(messages)
}

async fn fetch_remote_metadata(
    session: &mut ImapSession,
    sequence_set: String,
) -> Result<Vec<RemoteMessage>, VivariumError> {
    let fetches = session
        .fetch(sequence_set, METADATA_FETCH_ITEMS)
        .await
        .map_err(|e| VivariumError::Imap(format!("uid/size fetch failed: {e}")))?;
    collect_remote_metadata(fetches).await
}

async fn uid_fetch_remote_metadata(
    session: &mut ImapSession,
    uid_set: String,
) -> Result<Vec<RemoteMessage>, VivariumError> {
    let fetches = session
        .uid_fetch(uid_set, METADATA_FETCH_ITEMS)
        .await
        .map_err(|e| VivariumError::Imap(format!("uid/size fetch failed: {e}")))?;
    collect_remote_metadata(fetches).await
}

async fn collect_remote_metadata(
    fetches: impl TryStream<Ok = async_imap::types::Fetch, Error = async_imap::error::Error>,
) -> Result<Vec<RemoteMessage>, VivariumError> {
    Ok(fetches
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| VivariumError::Imap(format!("uid/size stream failed: {e}")))?
        .iter()
        .filter_map(|f| {
            let uid = f.uid?;
            let size = u64::from(f.size?);
            Some(RemoteMessage {
                uid,
                size,
                rfc_message_id: f.header().and_then(message_id_from_bytes),
            })
        })
        .collect())
}

async fn remote_uid_set(
    session: &mut ImapSession,
    window: SyncWindow,
) -> Result<String, VivariumError> {
    let query = date_search_query(window);
    let mut uids = session
        .uid_search(&query)
        .await
        .map_err(|e| VivariumError::Imap(format!("date search failed ({query}): {e}")))?
        .into_iter()
        .collect::<Vec<_>>();
    uids.sort_unstable();
    Ok(uid_set_string(&uids))
}

fn date_search_query(window: SyncWindow) -> String {
    let mut parts = Vec::new();
    if let Some(since) = window.since {
        parts.push(format!("SINCE {}", imap_date(since)));
    }
    if let Some(before) = window.before {
        parts.push(format!("BEFORE {}", imap_date(before)));
    }
    parts.join(" ")
}

fn imap_date(date: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS[date.month0() as usize];
    format!("{}-{month}-{}", date.day(), date.year())
}

fn uid_set_string(uids: &[u32]) -> String {
    uids.iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_fetch_items_include_message_id_header() {
        assert!(METADATA_FETCH_ITEMS.contains("UID"));
        assert!(METADATA_FETCH_ITEMS.contains("RFC822.SIZE"));
        assert!(METADATA_FETCH_ITEMS.contains("BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]"));
    }
}
