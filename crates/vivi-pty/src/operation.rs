use crate::protocol::Response;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

pub(crate) const MAX_OPERATION_ID_BYTES: usize = 128;
pub(crate) const MAX_OPERATION_RECORDS: usize = 256;

#[derive(Default)]
pub(super) struct OperationStore {
    records: VecDeque<OperationRecord>,
    pending: HashMap<String, Arc<PendingCompletion>>,
}

struct OperationRecord {
    operation_id: String,
    fingerprint: Value,
    response: Response,
}

#[cfg(test)]
#[allow(dead_code)]
pub(super) enum Replay {
    Miss,
    Hit(Response),
    Conflict,
}

pub(super) struct PendingCompletion {
    fingerprint: Value,
    response: Mutex<Option<Response>>,
    done: Arc<(Mutex<bool>, Condvar)>,
}

impl PendingCompletion {
    pub(super) fn wait_for_response(&self) -> Response {
        let (lock, cvar) = &*self.done;
        let mut done = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*done {
            done = cvar
                .wait(done)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        self.response
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("pending response set before signal")
    }
}

pub(super) enum OperationSlot {
    Hit(Response),
    Conflict,
    Coalesce(Arc<PendingCompletion>),
    Own,
}

impl OperationStore {
    #[cfg(test)]
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

    #[cfg(test)]
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

    pub(super) fn reserve(&mut self, operation_id: &str, fingerprint: &Value) -> OperationSlot {
        if let Some(record) = self
            .records
            .iter()
            .find(|record| record.operation_id == operation_id)
        {
            if record.fingerprint == *fingerprint {
                return OperationSlot::Hit(record.response.clone());
            }
            return OperationSlot::Conflict;
        }
        if let Some(pending) = self.pending.get(operation_id) {
            if pending.fingerprint == *fingerprint {
                return OperationSlot::Coalesce(Arc::clone(pending));
            }
            return OperationSlot::Conflict;
        }
        let pending = Arc::new(PendingCompletion {
            fingerprint: fingerprint.clone(),
            response: Mutex::new(None),
            done: Arc::new((Mutex::new(false), Condvar::new())),
        });
        self.pending.insert(operation_id.to_string(), pending);
        OperationSlot::Own
    }

    pub(super) fn complete(&mut self, operation_id: &str, fingerprint: &Value, response: Response) {
        if let Some(pending) = self.pending.remove(operation_id) {
            if pending.fingerprint != *fingerprint {
                eprintln!(
                    "operation {operation_id} fingerprint changed during execution; expected {fingerprint}, got {}",
                    pending.fingerprint
                );
            }
            *pending
                .response
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(response.clone());
            let (lock, cvar) = &*pending.done;
            let mut done = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            *done = true;
            cvar.notify_all();
        }
        self.records
            .retain(|record| record.operation_id != operation_id);
        self.records.push_back(OperationRecord {
            operation_id: operation_id.to_string(),
            fingerprint: fingerprint.clone(),
            response: response.clone(),
        });
        while self.records.len() > MAX_OPERATION_RECORDS {
            self.records.pop_front();
        }
    }
}

pub(crate) fn validate_operation_id(operation_id: &str) -> Result<(), String> {
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
