use std::io::{self, Write};

use crate::error::VivariumError;
use crate::extract;
use crate::message;
use crate::store::{MailStore, MessageLocation};

/// Print JSON representations of one or more messages to stdout.
///
/// # Errors
/// Returns an error if resolving a message, reading data, or parsing fails.
pub fn print_json_messages(
    store: &MailStore,
    account: &str,
    message_ids: &[String],
) -> Result<(), VivariumError> {
    let mut messages = Vec::new();
    for message_id in message_ids {
        messages.push(json_message(store, account, message_id)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&messages).unwrap_or_else(|_| "[]".to_string())
    );
    Ok(())
}

/// Export a raw message to stdout.
///
/// # Errors
/// Returns an error if reading the message or writing to stdout fails.
pub fn export_raw_message(store: &MailStore, message_id: &str) -> Result<(), VivariumError> {
    let data = store.read_message(message_id)?;
    io::stdout().write_all(&data)?;
    Ok(())
}

/// Export a message's extracted text to stdout.
///
/// # Errors
/// Returns an error if reading the message, extracting text, or writing to
/// stdout fails.
pub fn export_text_message(store: &MailStore, message_id: &str) -> Result<(), VivariumError> {
    let data = store.read_message(message_id)?;
    let extracted = extract::extract_text(&data)?;
    io::stdout().write_all(extracted.body_text.as_bytes())?;
    Ok(())
}

/// Fetch a message as a JSON value with citation metadata.
///
/// # Errors
/// Returns an error if resolving the message ID, reading the message data,
/// or parsing the email fails.
pub fn json_message(
    store: &MailStore,
    account: &str,
    message_id: &str,
) -> Result<serde_json::Value, VivariumError> {
    let resolved_message_id = store.resolve_message_id(message_id)?;
    let handle = store.display_handle(&resolved_message_id)?;
    let location = store.locate_message(message_id)?;
    let data = std::fs::read(&location.path)?;
    let mut value = message::to_json_message(&handle, &data)?;
    value["citation"] = citation_json(&handle, account, &location);
    Ok(value)
}

#[must_use]
pub fn citation_json(handle: &str, account: &str, location: &MessageLocation) -> serde_json::Value {
    let mut citation = serde_json::json!({
        "handle": handle,
        "account": account,
        "local_role": location.local_role,
        "source_type": "rfc5322",
    });
    if let Some(content_id) = &location.content_id {
        citation["content_id"] = serde_json::Value::String(content_id.clone());
    }
    if let Some(message_id) = &location.message_id {
        citation["message_id"] = serde_json::Value::String(message_id.clone());
    }
    citation
}

#[cfg(test)]
#[path = "retrieve_test.rs"]
mod tests;
