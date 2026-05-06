use super::metadata::ParsedMetadata;
use super::*;
use rusqlite::Transaction;

impl Storage {
    pub fn ingest_message(
        &mut self,
        request: &MessageIngestRequest,
        data: &[u8],
    ) -> Result<StoredMessage, VivariumError> {
        let content_id = sha256_hex(data);
        let blob_relpath = blob_relpath(&content_id);
        let blob_abspath = self.mail_root.join(&blob_relpath);
        let created_blob = write_blob_if_absent(&blob_abspath, data)?;
        let metadata = parse_metadata(data);
        let message_id = ingest_message_id(request, &content_id);
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction().map_err(|e| {
            VivariumError::Other(format!("failed to open storage transaction: {e}"))
        })?;
        upsert_blob_row(&tx, &content_id, &blob_relpath, data.len(), &metadata, &now)?;
        upsert_metadata_row(&tx, &content_id, &metadata)?;
        upsert_message_row(&tx, request, &message_id, &content_id, &now)?;
        upsert_remote_binding(&tx, &message_id, request.remote.as_ref(), &now)?;
        tx.commit().map_err(|e| {
            VivariumError::Other(format!("failed to commit storage transaction: {e}"))
        })?;
        Ok(StoredMessage {
            message_id,
            content_id,
            blob_relpath,
            created_blob,
        })
    }

    #[cfg(test)]
    pub(super) fn store_catalog_entry(
        &mut self,
        entry: &CatalogEntry,
        data: &[u8],
    ) -> Result<StoredMessage, VivariumError> {
        self.ingest_message(&request_from_catalog_entry(entry), data)
    }
}

fn ingest_message_id(request: &MessageIngestRequest, content_id: &str) -> String {
    request.message_id_hint.clone().unwrap_or_else(|| {
        request
            .remote
            .as_ref()
            .map(|remote| {
                remote_bound_message_id(&request.account, &request.local_role, content_id, remote)
            })
            .unwrap_or_else(|| fallback_message_id(request, content_id))
    })
}

fn upsert_blob_row(
    tx: &Transaction<'_>,
    content_id: &str,
    blob_relpath: &str,
    byte_size: usize,
    metadata: &ParsedMetadata,
    now: &str,
) -> Result<(), VivariumError> {
    tx.execute(
        "INSERT INTO blobs (content_id, blob_relpath, byte_size, rfc_message_id, parsed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(content_id) DO UPDATE SET
           blob_relpath = excluded.blob_relpath,
           byte_size = excluded.byte_size,
           rfc_message_id = COALESCE(excluded.rfc_message_id, blobs.rfc_message_id)",
        params![
            content_id,
            blob_relpath,
            i64::try_from(byte_size).unwrap_or(i64::MAX),
            metadata.normalized_message_id,
            now,
        ],
    )
    .map(|_| ())
    .map_err(|e| VivariumError::Other(format!("failed to upsert blob row: {e}")))
}

fn upsert_metadata_row(
    tx: &Transaction<'_>,
    content_id: &str,
    metadata: &ParsedMetadata,
) -> Result<(), VivariumError> {
    tx.execute(
        "INSERT INTO message_metadata (
           content_id, date, from_addr, to_addr, cc_addr, bcc_addr, subject, normalized_message_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(content_id) DO UPDATE SET
           date = excluded.date,
           from_addr = excluded.from_addr,
           to_addr = excluded.to_addr,
           cc_addr = excluded.cc_addr,
           bcc_addr = excluded.bcc_addr,
           subject = excluded.subject,
           normalized_message_id = excluded.normalized_message_id",
        params![
            content_id,
            metadata.date,
            metadata.from_addr,
            metadata.to_addr,
            metadata.cc_addr,
            metadata.bcc_addr,
            metadata.subject,
            metadata.normalized_message_id,
        ],
    )
    .map(|_| ())
    .map_err(|e| VivariumError::Other(format!("failed to upsert message metadata: {e}")))
}

fn upsert_message_row(
    tx: &Transaction<'_>,
    request: &MessageIngestRequest,
    message_id: &str,
    content_id: &str,
    now: &str,
) -> Result<(), VivariumError> {
    tx.execute(
        "INSERT INTO messages (
           message_id, account, content_id, local_role, read_state, starred,
           draft_state, discovered_at, updated_at, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, NULL)
         ON CONFLICT(message_id) DO UPDATE SET
           content_id = excluded.content_id,
           local_role = excluded.local_role,
           read_state = excluded.read_state,
           starred = excluded.starred,
           updated_at = excluded.updated_at,
           deleted_at = NULL",
        params![
            message_id,
            request.account,
            content_id,
            request.local_role,
            if request.read_state { 1 } else { 0 },
            if request.starred { 1 } else { 0 },
            now,
            now,
        ],
    )
    .map(|_| ())
    .map_err(|e| VivariumError::Other(format!("failed to upsert message row: {e}")))
}

fn upsert_remote_binding(
    tx: &Transaction<'_>,
    message_id: &str,
    remote: Option<&RemoteBindingInput>,
    now: &str,
) -> Result<(), VivariumError> {
    let Some(remote) = remote else {
        return Ok(());
    };
    tx.execute(
        "INSERT INTO remote_bindings (
           message_id, account, provider, remote_mailbox, remote_uid,
           remote_uidvalidity, last_verified_at, stale
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
         ON CONFLICT(message_id) DO UPDATE SET
           account = excluded.account,
           provider = excluded.provider,
           remote_mailbox = excluded.remote_mailbox,
           remote_uid = excluded.remote_uid,
           remote_uidvalidity = excluded.remote_uidvalidity,
           last_verified_at = excluded.last_verified_at,
           stale = 0",
        params![
            message_id,
            remote.account,
            remote.provider,
            remote.remote_mailbox,
            remote.remote_uid,
            remote.remote_uidvalidity,
            now,
        ],
    )
    .map(|_| ())
    .map_err(|e| VivariumError::Other(format!("failed to upsert remote binding: {e}")))
}
