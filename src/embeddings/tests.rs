use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use rusqlite::Connection;

use super::chunk::chunks_for_message;
use super::provider::EmbeddingProvider;
use super::{EmbeddingOptions, index_embeddings_with_provider};
use crate::catalog::{Catalog, CatalogEntry};
use crate::email_index::{self, IndexedMessage};
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

#[tokio::test]
async fn changed_raw_fingerprint_is_stale_and_not_embedded() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider::new(vec![vec![0.1, 0.2, 0.3]; 2]);
    let path = build_indexed_message(tmp.path(), "original body");
    fs::write(
        &path,
        "Message-ID: <one@example.com>\r\nSubject: hi\r\n\r\nchanged",
    )
    .unwrap();

    let stats =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
            .await
            .unwrap();

    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.stale, 1);
    assert_eq!(stats.embedded, 0);
}

#[tokio::test]
async fn changed_embedding_model_uses_separate_embedding_db() {
    let tmp = tempfile::tempdir().unwrap();
    build_indexed_message(tmp.path(), "body text");

    let first = MockProvider::with_model("first", vec![vec![0.1, 0.2, 0.3]; 2]);
    let second = MockProvider::with_model("second", vec![vec![0.4, 0.5, 0.6]; 2]);

    let first_stats =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &first)
            .await
            .unwrap();
    let second_stats =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &second)
            .await
            .unwrap();

    assert_eq!(first_stats.embedded, 2);
    assert_eq!(second_stats.embedded, 2);
    assert_eq!(embedding_count(tmp.path(), "mock-first"), 2);
    assert_eq!(embedding_count(tmp.path(), "mock-second"), 2);
}

#[tokio::test]
async fn rebuild_failure_leaves_existing_embeddings_intact() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider::new(vec![vec![0.1, 0.2, 0.3]; 2]);
    build_indexed_message(tmp.path(), "body text");

    index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
        .await
        .unwrap();
    assert_eq!(embedding_count(tmp.path(), "mock-model"), 2);

    let err = index_embeddings_with_provider(
        tmp.path(),
        "acct",
        EmbeddingOptions {
            rebuild: true,
            ..EmbeddingOptions::default()
        },
        &FailingProvider,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("provider unavailable"));
    assert_eq!(embedding_count(tmp.path(), "mock-model"), 2);
}

#[tokio::test]
async fn rebuild_failure_after_partial_provider_success_leaves_embeddings_intact() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider::new(vec![vec![0.1, 0.2, 0.3]; 2]);
    build_indexed_message_named(tmp.path(), "inbox-1", "one", "first body");
    build_indexed_message_named(tmp.path(), "inbox-2", "two", "second body");

    index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
        .await
        .unwrap();
    assert_eq!(
        embedding_first_values(tmp.path(), "mock-model"),
        vec![0.1; 4]
    );

    let err = index_embeddings_with_provider(
        tmp.path(),
        "acct",
        EmbeddingOptions {
            rebuild: true,
            ..EmbeddingOptions::default()
        },
        &FailingAfterFirstProvider::default(),
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("provider unavailable"));
    assert_eq!(
        embedding_first_values(tmp.path(), "mock-model"),
        vec![0.1; 4]
    );
}

#[tokio::test]
async fn rebuild_is_scoped_to_selected_provider_model_db() {
    let tmp = tempfile::tempdir().unwrap();
    build_indexed_message(tmp.path(), "body text");

    let first = MockProvider::with_model("first", vec![vec![0.1, 0.2, 0.3]; 2]);
    let second = MockProvider::with_model("second", vec![vec![0.4, 0.5, 0.6]; 2]);
    index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &first)
        .await
        .unwrap();
    index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &second)
        .await
        .unwrap();

    index_embeddings_with_provider(
        tmp.path(),
        "acct",
        EmbeddingOptions {
            rebuild: true,
            ..EmbeddingOptions::default()
        },
        &first,
    )
    .await
    .unwrap();

    assert_eq!(embedding_count(tmp.path(), "mock-first"), 2);
    assert_eq!(embedding_count(tmp.path(), "mock-second"), 2);
}

#[tokio::test]
async fn deleted_local_message_counts_error_without_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = MockProvider::new(vec![vec![0.1, 0.2, 0.3]; 2]);
    let path = build_indexed_message(tmp.path(), "body text");
    fs::remove_file(path).unwrap();

    let stats =
        index_embeddings_with_provider(tmp.path(), "acct", EmbeddingOptions::default(), &provider)
            .await
            .unwrap();

    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.errors, 1);
    assert_eq!(stats.embedded, 0);
}

fn build_indexed_message(mail_root: &Path, body: &str) -> PathBuf {
    build_indexed_message_named(mail_root, "inbox-1", "one", body)
}

fn build_indexed_message_named(
    mail_root: &Path,
    file_id: &str,
    message_id: &str,
    body: &str,
) -> PathBuf {
    let store = MailStore::new(mail_root);
    let eml = format!("Message-ID: <{message_id}@example.com>\r\nSubject: hi\r\n\r\n{body}");
    let path = store
        .store_message("inbox", file_id, eml.as_bytes())
        .unwrap();
    catalog(mail_root, "acct", &path);
    email_index::rebuild(mail_root, "acct").unwrap();
    path
}

fn catalog(mail_root: &Path, account: &str, path: &Path) {
    let data = fs::read(path).unwrap();
    let mut catalog = Catalog::open(mail_root).unwrap();
    let handle = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("cat-1")
        .to_string();
    catalog
        .upsert(&CatalogEntry {
            handle,
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
    model: String,
    vectors: Vec<Vec<f32>>,
}

impl MockProvider {
    fn new(vectors: Vec<Vec<f32>>) -> Self {
        Self::with_model("model", vectors)
    }

    fn with_model(model: &str, vectors: Vec<Vec<f32>>) -> Self {
        Self {
            model: model.into(),
            vectors,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for MockProvider {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError> {
        Ok(self.vectors.iter().take(inputs.len()).cloned().collect())
    }
}

struct FailingProvider;

#[async_trait]
impl EmbeddingProvider for FailingProvider {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "model"
    }

    async fn embed(&self, _inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError> {
        Err(VivariumError::Other("provider unavailable".into()))
    }
}

#[derive(Default)]
struct FailingAfterFirstProvider {
    calls: AtomicUsize,
}

#[async_trait]
impl EmbeddingProvider for FailingAfterFirstProvider {
    fn provider(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "model"
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(vec![vec![0.9, 0.8, 0.7]; inputs.len()]);
        }
        Err(VivariumError::Other("provider unavailable".into()))
    }
}

fn embedding_count(mail_root: &Path, db_name: &str) -> i64 {
    let db_path = mail_root
        .join(".vivarium")
        .join("embeddings")
        .join(format!("{db_name}.sqlite"));
    let conn = Connection::open(db_path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))
        .unwrap()
}

fn embedding_first_values(mail_root: &Path, db_name: &str) -> Vec<f32> {
    let db_path = mail_root
        .join(".vivarium")
        .join("embeddings")
        .join(format!("{db_name}.sqlite"));
    let conn = Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT vector FROM embeddings ORDER BY chunk_id")
        .unwrap();
    stmt.query_map([], |row| {
        let vector = row.get::<_, Vec<u8>>(0)?;
        Ok(f32::from_le_bytes([
            vector[0], vector[1], vector[2], vector[3],
        ]))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}
