use sha2::{Digest, Sha256};

use crate::email_index::IndexedMessage;
use crate::error::VivariumError;
use crate::extract;

pub(crate) const EXTRACTOR_VERSION: &str = "extract-v1";
pub(crate) const CHUNKER_VERSION: &str = "email-chunker-v1";
const MESSAGE_PREFIX_CHARS: usize = 2048;
const BODY_CHUNK_WORDS: usize = 600;
const BODY_CHUNK_OVERLAP: usize = 75;
pub(crate) const MAX_EMBED_INPUT_CHARS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmailChunk {
    pub(crate) chunk_id: String,
    pub(crate) account: String,
    pub(crate) handle: String,
    pub(crate) fingerprint: String,
    pub(crate) extractor_version: String,
    pub(crate) chunker_version: String,
    pub(crate) chunk_kind: String,
    pub(crate) chunk_ordinal: usize,
    pub(crate) text_hash: String,
    pub(crate) token_count: usize,
    pub(crate) text: String,
}

pub(crate) fn chunks_for_message(
    message: &IndexedMessage,
    data: &[u8],
) -> Result<Vec<EmailChunk>, VivariumError> {
    let extracted = extract::extract_text(data)?;
    let body = split_oversized_words(&extracted.body_text);
    let mut chunks = Vec::new();
    let message_text = truncate_to_chars(message_level_text(message, &body), MAX_EMBED_INPUT_CHARS);
    chunks.push(chunk(message, "message", 0, message_text));

    for (ordinal, text) in body_chunks(&body).into_iter().enumerate() {
        if !text.trim().is_empty() {
            chunks.push(chunk(message, "body", ordinal, text));
        }
    }
    Ok(chunks)
}

fn message_level_text(message: &IndexedMessage, body: &str) -> String {
    let prefix = body.chars().take(MESSAGE_PREFIX_CHARS).collect::<String>();
    format!(
        "Subject: {}\nFrom: {}\nTo: {}\nCc: {}\nDate: {}\n\n{}",
        message.subject, message.from_addr, message.to_addr, message.cc_addr, message.date, prefix
    )
}

fn body_chunks(body: &str) -> Vec<String> {
    let words = body.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = usize::min(start + BODY_CHUNK_WORDS, words.len());
        chunks.extend(split_long_text(&words[start..end].join(" ")));
        if end == words.len() {
            break;
        }
        start = end.saturating_sub(BODY_CHUNK_OVERLAP);
    }
    chunks
}

fn split_oversized_words(text: &str) -> String {
    text.split_whitespace()
        .flat_map(split_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_word(word: &str) -> Vec<String> {
    if word.chars().count() <= MAX_EMBED_INPUT_CHARS {
        return vec![word.to_string()];
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    for ch in word.chars() {
        current.push(ch);
        if current.chars().count() >= MAX_EMBED_INPUT_CHARS {
            parts.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn split_long_text(text: &str) -> Vec<String> {
    if text.chars().count() <= MAX_EMBED_INPUT_CHARS {
        return vec![text.to_string()];
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if word_len > MAX_EMBED_INPUT_CHARS {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            parts.extend(split_word(word));
            continue;
        }
        let separator = usize::from(!current.is_empty());
        if !current.is_empty()
            && current.chars().count() + separator + word_len > MAX_EMBED_INPUT_CHARS
        {
            parts.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn truncate_to_chars(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    text.chars().take(max_chars).collect()
}

fn chunk(message: &IndexedMessage, kind: &str, ordinal: usize, text: String) -> EmailChunk {
    let text_hash = hash_hex(text.as_bytes());
    let chunk_id = hash_hex(
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            message.account,
            message.handle,
            message.fingerprint,
            EXTRACTOR_VERSION,
            CHUNKER_VERSION,
            kind,
            ordinal
        )
        .as_bytes(),
    );
    EmailChunk {
        chunk_id,
        account: message.account.clone(),
        handle: message.handle.clone(),
        fingerprint: message.fingerprint.clone(),
        extractor_version: EXTRACTOR_VERSION.to_string(),
        chunker_version: CHUNKER_VERSION.to_string(),
        chunk_kind: kind.to_string(),
        chunk_ordinal: ordinal,
        text_hash,
        token_count: text.split_whitespace().count(),
        text,
    }
}

fn hash_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}
