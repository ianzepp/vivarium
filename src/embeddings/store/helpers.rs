use super::{EmailChunk, StoredEmbedding};
use crate::error::VivariumError;

pub(super) fn validate_dimensions(
    chunks: &[EmailChunk],
    vectors: &[Vec<f32>],
) -> Result<(), VivariumError> {
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

pub(super) fn validate_row_dimensions(
    rows: &[(EmailChunk, Vec<f32>)],
) -> Result<(), VivariumError> {
    let Some(dimensions) = rows.first().map(|(_, vector)| vector.len()) else {
        return Ok(());
    };
    if dimensions == 0 || rows.iter().any(|(_, vector)| vector.len() != dimensions) {
        return Err(VivariumError::Other(
            "embedding provider returned inconsistent dimensions".into(),
        ));
    }
    Ok(())
}

pub(super) fn encode_vector(vector: &[f32]) -> Vec<u8> {
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

pub(super) fn stored_embedding_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredEmbedding> {
    let ordinal = row.get::<_, i64>(4)?;
    let vector = decode_vector(row.get::<_, Vec<u8>>(6)?)?;
    Ok(StoredEmbedding {
        chunk_id: row.get(0)?,
        account: row.get(1)?,
        message_id: row.get(2)?,
        content_id: row.get(3)?,
        chunk_ordinal: usize::try_from(ordinal).unwrap_or_default(),
        text_hash: row.get(5)?,
        vector,
    })
}

pub(super) fn safe_name(value: &str) -> String {
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
