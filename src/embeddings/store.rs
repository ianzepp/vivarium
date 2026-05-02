use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

use super::chunk::EmailChunk;
use crate::error::VivariumError;
use crate::store::secure_create_dir_all;

pub(crate) struct EmbeddingStore {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredEmbedding {
    pub(crate) chunk_id: String,
    pub(crate) account: String,
    pub(crate) handle: String,
    pub(crate) fingerprint: String,
    pub(crate) chunk_ordinal: usize,
    pub(crate) text_hash: String,
    pub(crate) vector: Vec<f32>,
}

impl EmbeddingStore {
    pub(crate) fn open(
        mail_root: &Path,
        provider: &str,
        model: &str,
    ) -> Result<Self, VivariumError> {
        let dir = mail_root.join(".vivarium").join("embeddings");
        secure_create_dir_all(&dir)?;
        let path = dir.join(format!(
            "{}-{}.sqlite",
            safe_name(provider),
            safe_name(model)
        ));
        let conn = Connection::open(&path)
            .map_err(|e| VivariumError::Other(format!("failed to open embedding DB: {e}")))?;
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let store = Self { conn };
        store.ensure_schema()?;
        store.write_metadata(provider, model)?;
        Ok(store)
    }

    pub(crate) fn clear_account(&mut self, account: &str) -> Result<(), VivariumError> {
        self.conn
            .execute("DELETE FROM chunks WHERE account = ?1", params![account])
            .map_err(|e| VivariumError::Other(format!("failed to clear embedding chunks: {e}")))?;
        Ok(())
    }

    pub(crate) fn pending_chunks(
        &self,
        chunks: &[EmailChunk],
    ) -> Result<Vec<EmailChunk>, VivariumError> {
        let mut pending = Vec::new();
        for chunk in chunks {
            if !self.has_embedding(chunk)? {
                pending.push(chunk.clone());
            }
        }
        Ok(pending)
    }

    pub(crate) fn store_embeddings(
        &mut self,
        chunks: &[EmailChunk],
        provider: &str,
        model: &str,
        vectors: Vec<Vec<f32>>,
    ) -> Result<(), VivariumError> {
        validate_dimensions(chunks, &vectors)?;
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction().map_err(|e| {
            VivariumError::Other(format!("failed to start embedding transaction: {e}"))
        })?;
        for (chunk, vector) in chunks.iter().zip(vectors) {
            upsert_chunk(&tx, chunk, &now)?;
            tx.execute(
                "INSERT OR REPLACE INTO embeddings
                 (chunk_id, provider, model, dimensions, vector, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    chunk.chunk_id,
                    provider,
                    model,
                    i64::try_from(vector.len()).unwrap_or(i64::MAX),
                    encode_vector(&vector),
                    now,
                ],
            )
            .map_err(|e| VivariumError::Other(format!("failed to store embedding row: {e}")))?;
        }
        tx.commit().map_err(|e| {
            VivariumError::Other(format!("failed to commit embedding transaction: {e}"))
        })?;
        Ok(())
    }

    pub(crate) fn embeddings(&self, account: &str) -> Result<Vec<StoredEmbedding>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.chunk_id, c.account, c.handle, c.fingerprint, c.chunk_ordinal,
                    c.text_hash, e.vector
             FROM chunks c
             JOIN embeddings e ON e.chunk_id = c.chunk_id
             WHERE c.account = ?1",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare embedding query: {e}")))?;
        let rows = stmt
            .query_map(params![account], stored_embedding_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to query embeddings: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read embedding row: {e}")))
        })
        .collect()
    }

    fn has_embedding(&self, chunk: &EmailChunk) -> Result<bool, VivariumError> {
        let existing = self
            .conn
            .query_row(
                "SELECT text_hash FROM chunks c
                 JOIN embeddings e ON e.chunk_id = c.chunk_id
                 WHERE c.chunk_id = ?1",
                params![chunk.chunk_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to check embedding row: {e}")))?;
        Ok(existing.as_deref() == Some(chunk.text_hash.as_str()))
    }

    fn ensure_schema(&self) -> Result<(), VivariumError> {
        self.conn
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS embedding_metadata (
                  key TEXT PRIMARY KEY,
                  value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS chunks (
                  chunk_id TEXT PRIMARY KEY,
                  account TEXT NOT NULL,
                  handle TEXT NOT NULL,
                  fingerprint TEXT NOT NULL,
                  extractor_version TEXT NOT NULL,
                  chunker_version TEXT NOT NULL,
                  chunk_kind TEXT NOT NULL,
                  chunk_ordinal INTEGER NOT NULL,
                  text_hash TEXT NOT NULL,
                  token_count INTEGER NOT NULL,
                  indexed_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS embeddings (
                  chunk_id TEXT PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE,
                  provider TEXT NOT NULL,
                  model TEXT NOT NULL,
                  dimensions INTEGER NOT NULL,
                  vector BLOB NOT NULL,
                  indexed_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS chunks_handle_idx ON chunks(account, handle);
                CREATE INDEX IF NOT EXISTS chunks_fingerprint_idx ON chunks(account, fingerprint);
                ",
            )
            .map_err(|e| VivariumError::Other(format!("failed to initialize embedding DB: {e}")))?;
        Ok(())
    }

    fn write_metadata(&self, provider: &str, model: &str) -> Result<(), VivariumError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO embedding_metadata (key, value)
                 VALUES ('provider', ?1), ('model', ?2)",
                params![provider, model],
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to write embedding metadata: {e}"))
            })?;
        Ok(())
    }
}

fn upsert_chunk(
    tx: &rusqlite::Transaction<'_>,
    chunk: &EmailChunk,
    now: &str,
) -> Result<(), VivariumError> {
    tx.execute(
        "INSERT OR REPLACE INTO chunks
         (chunk_id, account, handle, fingerprint, extractor_version, chunker_version,
          chunk_kind, chunk_ordinal, text_hash, token_count, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            chunk.chunk_id,
            chunk.account,
            chunk.handle,
            chunk.fingerprint,
            chunk.extractor_version,
            chunk.chunker_version,
            chunk.chunk_kind,
            i64::try_from(chunk.chunk_ordinal).unwrap_or(i64::MAX),
            chunk.text_hash,
            i64::try_from(chunk.token_count).unwrap_or(i64::MAX),
            now,
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to store chunk row: {e}")))?;
    Ok(())
}

fn validate_dimensions(chunks: &[EmailChunk], vectors: &[Vec<f32>]) -> Result<(), VivariumError> {
    if chunks.len() != vectors.len() {
        return Err(VivariumError::Other(format!(
            "embedding provider returned {} vectors for {} chunks",
            vectors.len(),
            chunks.len()
        )));
    }
    let Some(dimensions) = vectors.first().map(Vec::len) else {
        return Ok(());
    };
    if dimensions == 0 || vectors.iter().any(|vector| vector.len() != dimensions) {
        return Err(VivariumError::Other(
            "embedding provider returned inconsistent dimensions".into(),
        ));
    }
    Ok(())
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>()
}

fn decode_vector(data: Vec<u8>) -> rusqlite::Result<Vec<f32>> {
    if !data.len().is_multiple_of(4) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(data
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect())
}

fn stored_embedding_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredEmbedding> {
    let ordinal = row.get::<_, i64>(4)?;
    let vector = decode_vector(row.get::<_, Vec<u8>>(6)?)?;
    Ok(StoredEmbedding {
        chunk_id: row.get(0)?,
        account: row.get(1)?,
        handle: row.get(2)?,
        fingerprint: row.get(3)?,
        chunk_ordinal: usize::try_from(ordinal).unwrap_or_default(),
        text_hash: row.get(5)?,
        vector,
    })
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}
