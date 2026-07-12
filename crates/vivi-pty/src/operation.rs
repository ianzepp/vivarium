use crate::protocol::Response;
use serde_json::Value;
use std::collections::VecDeque;

pub(crate) const MAX_OPERATION_ID_BYTES: usize = 128;
pub(crate) const MAX_OPERATION_RECORDS: usize = 256;

#[derive(Default)]
pub(super) struct OperationStore {
    records: VecDeque<OperationRecord>,
}

struct OperationRecord {
    operation_id: String,
    fingerprint: Value,
    response: Response,
}

pub(super) enum Replay {
    Miss,
    Hit(Response),
    Conflict,
}

impl OperationStore {
    pub(super) fn lookup(&self, operation_id: &str, fingerprint: &Value) -> Replay {
        let Some(record) = self
            .records
            .iter()
            .find(|record| record.operation_id == operation_id)
        else {
            return Replay::Miss;
        };
        if record.fingerprint == *fingerprint {
            Replay::Hit(record.response.clone())
        } else {
            Replay::Conflict
        }
    }

    pub(super) fn insert(&mut self, operation_id: String, fingerprint: Value, response: Response) {
        self.records
            .retain(|record| record.operation_id != operation_id);
        self.records.push_back(OperationRecord {
            operation_id,
            fingerprint,
            response,
        });
        while self.records.len() > MAX_OPERATION_RECORDS {
            self.records.pop_front();
        }
    }
}

pub(super) fn validate_operation_id(operation_id: &str) -> Result<(), String> {
    if operation_id.is_empty() || operation_id.len() > MAX_OPERATION_ID_BYTES {
        return Err(format!(
            "operation_id must be 1..={MAX_OPERATION_ID_BYTES} bytes"
        ));
    }
    if !operation_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("operation_id contains an unsupported character".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "operation_test.rs"]
mod tests;
