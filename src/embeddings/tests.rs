use std::fs;
use std::path::Path;

use async_trait::async_trait;

use super::chunk::chunks_for_message;
use super::provider::EmbeddingProvider;
use super::{EmbeddingOptions, index_embeddings_with_provider};
use crate::catalog::{Catalog, CatalogEntry};
use crate::email_index::{self, EmailIndex, IndexedMessage};
use crate::error::VivariumError;
use crate::store::MailStore;

#[test]
fn chunk_ids_are_stable_and_oversized_words_split() {
    let message = indexed_message("acct", "inbox-1", "f1", "Subject");
    let long_word = "a".repeat(9000);
    let eml = format!("Subject: hi\r\n\r\nhello {long_word} world");

    let first = chunks_for_message(&message, eml.as_bytes()).unwrap();
    let second = chunks_for_message(&message, eml.as_bytes()).unwrap();

    assert_eq!(first[0].chunk_id, second[0].chunk_id);
    assert!(first.iter().any(|chunk| chunk.chunk_kind == "message"));
    assert!(first.iter().all(|chunk| chunk.text.len() < 20_000));
}

#[tokio::test]
async fn embedding_index_stores_vectors_without_text_and_reuses() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider::new(vec![vec![0.1, 0.2, 0.3]; 2]);
    let body = "secret body should not be stored";
    build_indexed_message(tmp.path(), body);

    let stats =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
            .await
            .unwrap();
    let reused =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
            .await
            .unwrap();

    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.embedded, 2);
    assert_eq!(reused.reused, 2);
    let db = fs::read(tmp.path().join(".vivarium/embeddings/mock-model.sqlite")).unwrap();
    assert!(!String::from_utf8_lossy(&db).contains(body));
}

#[tokio::test]
async fn inconsistent_provider_dimensions_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider::new(vec![vec![0.1], vec![0.1, 0.2]]);
    build_indexed_message(tmp.path(), "body text");

    let err =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
            .await
            .unwrap_err();

    assert!(err.to_string().contains("inconsistent dimensions"));
}

fn build_indexed_message(mail_root: &Path, body: &str) {
    let store = MailStore::new(mail_root);
    let eml = format!("Message-ID: <one@example.com>\r\nSubject: hi\r\n\r\n{body}");
    let path = store
        .store_message("inbox", "inbox-1", eml.as_bytes())
        .unwrap();
    catalog(mail_root, "acct", &path);
    email_index::rebuild(mail_root, "acct").unwrap();
    let index = EmailIndex::open(mail_root).unwrap();
    assert_eq!(index.count_messages("acct").unwrap(), 1);
}

fn catalog(mail_root: &Path, account: &str, path: &Path) {
    let data = fs::read(path).unwrap();
    let mut catalog = Catalog::open(mail_root).unwrap();
    catalog
        .upsert(&CatalogEntry {
            handle: "cat-1".into(),
            raw_path: path.to_string_lossy().to_string(),
            fingerprint: crate::catalog::fingerprint(&data),
            account: account.into(),
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

fn indexed_message(
    account: &str,
    handle: &str,
    fingerprint: &str,
    subject: &str,
) -> IndexedMessage {
    IndexedMessage {
        account: account.into(),
        handle: handle.into(),
        catalog_handle: "cat".into(),
        fingerprint: fingerprint.into(),
        raw_path: "unused".into(),
        folder: "INBOX".into(),
        maildir_subdir: "new".into(),
        date: "2026-05-02 12:00".into(),
        from_addr: String::new(),
        to_addr: String::new(),
        cc_addr: String::new(),
        bcc_addr: String::new(),
        subject: subject.into(),
        rfc_message_id: Some("one@example.com".into()),
    }
}

struct MockProvider {
    vectors: Vec<Vec<f32>>,
}

impl MockProvider {
    fn new(vectors: Vec<Vec<f32>>) -> Self {
        Self { vectors }
    }
}

#[async_trait]
impl EmbeddingProvider for MockProvider {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "model"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError> {
        Ok(self.vectors.iter().take(inputs.len()).cloned().collect())
    }
}
