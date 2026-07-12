use crate::protocol::ControlLease;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

pub const DEFAULT_LEASE_TTL_MS: u64 = 30_000;
pub const MAX_LEASE_TTL_MS: u64 = 300_000;
const MAX_LEASE_FIELD_BYTES: usize = 128;

#[derive(Debug)]
pub enum LeaseError {
    InvalidInput(String),
    Busy(String),
    NotFound(String),
    Expired(String),
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::Busy(message)
            | Self::NotFound(message)
            | Self::Expired(message) => formatter.write_str(message),
        }
    }
}

struct ActiveLease {
    grant: ControlLease,
    expires_at: Instant,
}

#[derive(Default)]
pub struct LeaseManager {
    next_id: AtomicU64,
    active: Mutex<HashMap<String, ActiveLease>>,
    session_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl LeaseManager {
    pub fn acquire(
        &self,
        session_id: &str,
        holder: String,
        ttl_ms: u64,
    ) -> Result<ControlLease, LeaseError> {
        validate_field("holder", &holder)?;
        if !(1..=MAX_LEASE_TTL_MS).contains(&ttl_ms) {
            return Err(LeaseError::InvalidInput(format!(
                "lease ttl must be within 1..={MAX_LEASE_TTL_MS} milliseconds"
            )));
        }
        let lock = self.session_lock(session_id);
        let _lock = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = active.get(session_id)
            && existing.expires_at > Instant::now()
        {
            return Err(LeaseError::Busy(format!(
                "session is controlled by {}",
                existing.grant.holder
            )));
        }
        active.remove(session_id);
        let lease_id = format!("lease-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let grant = ControlLease {
            session_id: session_id.into(),
            lease_id,
            holder,
            expires_in_ms: ttl_ms,
        };
        active.insert(
            session_id.into(),
            ActiveLease {
                grant: grant.clone(),
                expires_at: Instant::now() + Duration::from_millis(ttl_ms),
            },
        );
        Ok(grant)
    }

    pub fn validate(&self, session_id: &str, lease_id: &str) -> Result<(), LeaseError> {
        validate_field("lease_id", lease_id)?;
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(lease) = active.get(session_id) else {
            return Err(LeaseError::NotFound(format!(
                "no active control lease for session {session_id}"
            )));
        };
        if lease.expires_at <= Instant::now() {
            active.remove(session_id);
            return Err(LeaseError::Expired(format!(
                "control lease expired for session {session_id}"
            )));
        }
        if lease.grant.lease_id != lease_id {
            return Err(LeaseError::NotFound("control lease does not match".into()));
        }
        Ok(())
    }

    pub fn release(&self, session_id: &str, lease_id: &str) -> Result<(), LeaseError> {
        self.with_lease(session_id, lease_id, || {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            active.remove(session_id);
            Ok(())
        })
    }

    pub fn release_session(&self, session_id: &str) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        active.remove(session_id);
    }

    fn session_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self
            .session_locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks
            .entry(session_id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub fn with_lease<F, T, E>(&self, session_id: &str, lease_id: &str, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
        E: From<LeaseError>,
    {
        validate_field("lease_id", lease_id)?;
        let lock = self.session_lock(session_id);
        let _lock = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(lease) = active.get(session_id) else {
                return Err(LeaseError::NotFound(format!(
                    "no active control lease for session {session_id}"
                ))
                .into());
            };
            if lease.expires_at <= Instant::now() {
                active.remove(session_id);
                return Err(LeaseError::Expired(format!(
                    "control lease expired for session {session_id}"
                ))
                .into());
            }
            if lease.grant.lease_id != lease_id {
                return Err(LeaseError::NotFound("control lease does not match".into()).into());
            }
        }
        f()
    }
}

fn validate_field(name: &str, value: &str) -> Result<(), LeaseError> {
    if value.trim().is_empty() || value.len() > MAX_LEASE_FIELD_BYTES {
        return Err(LeaseError::InvalidInput(format!(
            "{name} must be 1..={MAX_LEASE_FIELD_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "lease_test.rs"]
mod tests;
