use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::provider::EmbeddingProvider;
use super::{EmbeddingOptions, index_embeddings_with_provider};
use crate::catalog::{Catalog, CatalogEntry};
use crate::email_index;
use crate::error::VivariumError;
use crate::store::MailStore;

#[tokio::test]
async fn embedding_index_can_scope_to_catalog_handles() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider;
    build_indexed_message(tmp.path(), "one", "first body");
    build_indexed_message(tmp.path(), "two", "second body");

    let stats = index_embeddings_with_provider(
        tmp.path(),
        "acct",
        EmbeddingOptions {
            catalog_handles: Some(BTreeSet::from(["two".to_string()])),
            ..EmbeddingOptions::default()
        },
        &provider,
    )
    .await
    .unwrap();

    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.embedded, 2);
}

fn build_indexed_message(mail_root: &Path, file_id: &str, body: &str) {
    let path = store_message(mail_root, file_id, body);
    catalog(mail_root, file_id, &path);
    email_index::rebuild(mail_root, "acct").unwrap();
}

fn store_message(mail_root: &Path, file_id: &str, body: &str) -> PathBuf {
    let store = MailStore::new(mail_root);
    let eml = format!("Message-ID: <{file_id}@example.com>\r\nSubject: hi\r\n\r\n{body}");
    store
        .store_message("inbox", file_id, eml.as_bytes())
        .unwrap()
}

fn catalog(mail_root: &Path, handle: &str, path: &Path) {
    let data = std::fs::read(path).unwrap();
    Catalog::open(mail_root)
        .unwrap()
        .upsert(&CatalogEntry {
            handle: handle.into(),
            raw_path: path.to_string_lossy().to_string(),
            fingerprint: crate::catalog::fingerprint(&data),
            account: "acct".into(),
            folder: "INBOX".into(),
            maildir_subdir: "new".into(),
            date: "2026-05-02 12:00".into(),
            from: String::new(),
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            rfc_message_id: crate::message::message_id_from_bytes(&data).unwrap_or_default(),
            remote: None,
            is_duplicate: false,
        })
        .unwrap();
}

struct MockProvider;

#[async_trait]
impl EmbeddingProvider for MockProvider {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "model"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError> {
        Ok(vec![vec![0.1, 0.2, 0.3]; inputs.len()])
    }
}
