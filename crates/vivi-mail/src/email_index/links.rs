use std::collections::BTreeSet;

use mail_parser::MessageParser;

use crate::message::normalize_message_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageLink {
    pub(crate) kind: &'static str,
    pub(crate) rfc_message_id: String,
}

pub(crate) fn links_from_raw(data: &[u8]) -> Vec<MessageLink> {
    let Some(parsed) = MessageParser::default().parse(data) else {
        return Vec::new();
    };
    let mut dedupe = BTreeSet::new();
    let mut links = Vec::new();
    if let Some(message_id) = parsed.message_id().and_then(normalize_message_id) {
        push_link(&mut links, &mut dedupe, "message_id", message_id);
    }
    for message_id in message_ids_from_header(parsed.in_reply_to()) {
        push_link(&mut links, &mut dedupe, "in_reply_to", message_id);
    }
    for message_id in message_ids_from_header(parsed.references()) {
        push_link(&mut links, &mut dedupe, "reference", message_id);
    }
    links
}

fn push_link(
    links: &mut Vec<MessageLink>,
    dedupe: &mut BTreeSet<(&'static str, String)>,
    kind: &'static str,
    rfc_message_id: String,
) {
    if dedupe.insert((kind, rfc_message_id.clone())) {
        links.push(MessageLink {
            kind,
            rfc_message_id,
        });
    }
}

fn message_ids_from_header(header: &mail_parser::HeaderValue<'_>) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    if let Some(values) = header.as_text_list() {
        for value in values {
            for token in value.split_whitespace() {
                if let Some(id) = normalize_message_id(token) {
                    ids.insert(id);
                }
            }
        }
    }
    ids
}
