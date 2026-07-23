use std::fs;

use crate::email_index::{self, IndexedMessage};
use crate::error::VivariumError;
use crate::message;
use crate::retrieve::citation_json;
use crate::store::MailStore;

/// Print thread messages as JSON to stdout.
///
/// # Errors
/// Returns an error if resolving the seed, reading the index, or serializing
/// the output fails.
pub fn print_thread_json(
    store: &MailStore,
    account: &str,
    seed_handle: &str,
    limit: usize,
) -> Result<(), VivariumError> {
    let output = thread_json(store, account, seed_handle, limit)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string())
    );
    Ok(())
}

/// Build a JSON representation of a thread.
///
/// # Errors
/// Returns an error if resolving the seed, reading the index, reading message
/// data, or parsing an email fails.
pub fn thread_json(
    store: &MailStore,
    account: &str,
    seed_handle: &str,
    limit: usize,
) -> Result<serde_json::Value, VivariumError> {
    let resolved_seed = store.resolve_message_id(seed_handle)?;
    let index = email_index::ensure_for_thread(store.root(), account, &resolved_seed)?;
    let messages = index.thread_messages(account, &resolved_seed, limit)?;
    let total = messages.len();
    let messages = messages
        .iter()
        .take(limit)
        .map(|message| indexed_message_json(message, account))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(serde_json::json!({
        "seed": store.display_handle(&resolved_seed)?,
        "total": total,
        "limit": limit,
        "messages": messages,
    }))
}

fn indexed_message_json(
    indexed: &IndexedMessage,
    account: &str,
) -> Result<serde_json::Value, VivariumError> {
    let data = fs::read(&indexed.blob_path)?;
    let mut json = message::to_json_message(&indexed.handle, &data)?;
    json["citation"] = citation_json(&indexed.handle, account, &indexed.location());
    Ok(json)
}

#[cfg(test)]
#[path = "thread_test.rs"]
mod tests;
