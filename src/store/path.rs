use std::path::Path;

pub fn message_id_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(display_message_id)
}

pub(super) fn canonical_folder(folder: &str) -> Option<&'static str> {
    Some(match folder.to_ascii_lowercase().as_str() {
        "inbox" | "new" => "INBOX",
        "archive" | "archives" | "all" => "Archive",
        "sent" => "Sent",
        "draft" | "drafts" => "Drafts",
        "outbox" => "outbox",
        _ => return None,
    })
}

pub(super) fn is_message_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| {
            name.split_once(":2,")
                .map_or(name, |(id, _)| id)
                .ends_with(".eml")
        })
        .unwrap_or(false)
}

pub(super) fn maildir_filename(message_id: &str, subdir: &str) -> String {
    let base = storage_message_id(message_id);
    if subdir == "cur" {
        format!("{base}:2,S")
    } else {
        base
    }
}

fn storage_message_id(message_id: &str) -> String {
    let display = display_message_id(message_id);
    format!("{display}.eml")
}

pub(super) fn display_message_id(message_id: &str) -> String {
    let before_flags = message_id
        .split_once(":2,")
        .map_or(message_id, |(id, _)| id);
    before_flags
        .strip_suffix(".eml")
        .unwrap_or(before_flags)
        .to_string()
}

pub(super) fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
