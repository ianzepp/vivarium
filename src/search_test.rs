use std::path::Path;

use super::*;
use crate::catalog::{Catalog, CatalogEntry};
use crate::email_index;
use crate::store::MailStore;

#[allow(clippy::cast_precision_loss)]
fn score_query(query: &str, data: &[u8]) -> f64 {
    let text = std::str::from_utf8(data).ok().unwrap_or("");
    let text_lower = text.to_ascii_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();

    if words.is_empty() {
        return 0.0;
    }

    let query_len = words.len();
    let mut total_score = 0.0f64;
    let mut found = 0;

    for word in words {
        if text_lower.contains(word) {
            let weight = if word.len() > 3 { 2.0 } else { 1.0 };
            total_score += weight;
            found += 1;
        }
    }

    if found == 0 {
        0.0
    } else {
        total_score / query_len as f64
    }
}

#[test]
fn scores_contained_words() {
    let data = b"From: a@b\r\nTo: c@d\r\nSubject: Hello World\r\n\r\nBody text";
    let score = score_query("hello", data);
    assert!(score > 0.0_f64);
}

#[test]
#[allow(clippy::float_cmp)]
fn scores_zero_for_nonmatching() {
    let data = b"Subject: Hello World\r\n\r\nBody";
    let score = score_query("zxczxczxczxczxczxczxc", data);
    assert_eq!(score, 0.0);
}

#[test]
fn snippet_respects_max_len() {
    let data = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nHello world body text";
    let snippet = snippet_from_bytes(data, 5);
    assert!(snippet.len() <= 5);
}

#[test]
fn keyword_search_matches_indexed_eml_files_with_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let path = store
        .store_message(
            "inbox",
            "inbox-1",
            b"From: Agent <agent@example.com>\r\nTo: me@example.com\r\nDate: Sat, 2 May 2026 13:35:00 +0000\r\nSubject: Release notice\r\n\r\nRelease body",
        )
        .unwrap();
    catalog(tmp.path(), "acct", &path, "INBOX");
    email_index::rebuild(tmp.path(), "acct").unwrap();

    let (results, total) = keyword_search(tmp.path(), "acct", "release", 10, 0, None).unwrap();

    assert_eq!(total, 1);
    assert_eq!(results[0].message_id, "inbox-1");
    assert_eq!(results[0].account, "acct");
    assert_eq!(results[0].local_role, "inbox");
    assert_eq!(results[0].from, "Agent <agent@example.com>");
    assert_eq!(results[0].subject, "Release notice");
    assert!(!results[0].content_id.is_empty());
}

#[test]
fn keyword_search_filters_by_sender_and_sender_domain() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let agent = store
        .store_message(
            "inbox",
            "inbox-1",
            b"From: Agent <agent@example.com>\r\nTo: me@example.com\r\nDate: Sat, 2 May 2026 13:35:00 +0000\r\nSubject: Release notice\r\n\r\nRelease body",
        )
        .unwrap();
    let other = store
        .store_message(
            "inbox",
            "inbox-2",
            b"From: Other <other@test.example>\r\nTo: me@example.com\r\nDate: Sat, 2 May 2026 13:36:00 +0000\r\nSubject: Release notice\r\n\r\nRelease body",
        )
        .unwrap();
    catalog(tmp.path(), "acct", &agent, "INBOX");
    catalog(tmp.path(), "acct", &other, "INBOX");
    email_index::rebuild(tmp.path(), "acct").unwrap();

    let (sender_results, sender_total) = keyword_search(
        tmp.path(),
        "acct",
        "release",
        10,
        0,
        SearchFilters::new(None, Some("agent@example.com"), None),
    )
    .unwrap();
    let (domain_results, domain_total) = keyword_search(
        tmp.path(),
        "acct",
        "release",
        10,
        0,
        SearchFilters::new(None, None, Some("test.example")),
    )
    .unwrap();

    assert_eq!(sender_total, 1);
    assert_eq!(sender_results[0].message_id, "inbox-1");
    assert_eq!(domain_total, 1);
    assert_eq!(domain_results[0].message_id, "inbox-2");
}

#[test]
fn keyword_search_ignores_unindexed_maildir_files() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    store
        .store_message(
            "inbox",
            "inbox-1",
            b"Subject: Release notice\r\n\r\nRelease body",
        )
        .unwrap();

    let (results, total) = keyword_search(tmp.path(), "acct", "release", 10, 0, None).unwrap();

    assert_eq!(total, 0);
    assert!(results.is_empty());
}

#[test]
fn keyword_search_errors_when_catalog_exists_but_index_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let path = store
        .store_message(
            "inbox",
            "inbox-1",
            b"Subject: Release notice\r\n\r\nRelease body",
        )
        .unwrap();
    catalog(tmp.path(), "acct", &path, "INBOX");

    let err = keyword_search(tmp.path(), "acct", "release", 10, 0, None).unwrap_err();

    assert!(err.to_string().contains("email index is empty"));
}

#[test]
fn indexed_lexical_results_use_indexed_blob_content() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let path = store
        .store_message(
            "inbox",
            "inbox-1",
            b"Subject: Release notice\r\n\r\nRelease body",
        )
        .unwrap();
    catalog(tmp.path(), "acct", &path, "INBOX");
    email_index::rebuild(tmp.path(), "acct").unwrap();
    std::fs::write(&path, b"Subject: Release notice\r\n\r\nchanged").unwrap();

    let results = indexed_lexical_results(tmp.path(), "acct", "release").unwrap();

    assert_eq!(results.len(), 1);
}

#[test]
fn json_result_includes_citation() {
    let result = SearchResult {
        handle: "inbox-1".into(),
        message_id: "inbox-1".into(),
        account: "acct".into(),
        content_id: "content-1".into(),
        local_role: "inbox".into(),
        date: "2026-05-02 12:00".into(),
        from: "Agent".into(),
        subject: "Subject".into(),
        score: 1.0,
        lexical_score: Some(1.0),
        semantic_score: None,
        chunk_id: None,
        snippet: "body".into(),
    };

    let json = to_json_result(&result);

    assert_eq!(json["citation"]["handle"], "inbox-1");
    assert_eq!(json["citation"]["account"], "acct");
    assert_eq!(json["citation"]["source_type"], "rfc5322");
}

fn catalog(mail_root: &Path, account: &str, path: &Path, folder: &str) {
    let data = std::fs::read(path).unwrap();
    let handle = path.file_stem().unwrap().to_string_lossy().to_string();
    let mut catalog = Catalog::open(mail_root).unwrap();
    catalog
        .upsert(&CatalogEntry {
            handle,
            account: account.into(),
            content_id: crate::catalog::fingerprint(&data),
            blob_path: path.to_string_lossy().to_string(),
            local_role: storage_role(folder),
            read_state: false,
            starred: false,
            date: "2026-05-02 13:35".into(),
            from: "Agent".into(),
            to: "me@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Release notice".into(),
            rfc_message_id: crate::message::message_id_from_bytes(&data).unwrap_or_default(),
            remote: None,
        })
        .unwrap();
}

fn storage_role(folder: &str) -> String {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" => "inbox".into(),
        "archive" => "archive".into(),
        "trash" => "trash".into(),
        "sent" => "sent".into(),
        "draft" | "drafts" => "drafts".into(),
        other => other.into(),
    }
}
