use std::io::{self, Write};

use crate::error::VivariumError;
use crate::message;
use crate::store::MailStore;

pub fn print_json_messages(store: &MailStore, message_ids: &[String]) -> Result<(), VivariumError> {
    let mut messages = Vec::new();
    for message_id in message_ids {
        let data = store.read_message(message_id)?;
        messages.push(message::to_json_message(message_id, &data)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&messages).unwrap_or_else(|_| "[]".to_string())
    );
    Ok(())
}

pub fn export_raw_message(store: &MailStore, message_id: &str) -> Result<(), VivariumError> {
    let data = store.read_message(message_id)?;
    io::stdout().write_all(&data)?;
    Ok(())
}
