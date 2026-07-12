use super::*;
use crate::operation::OperationSlot;
use crate::protocol::{Response, SessionState, error_codes};
use serde_json::{Value, json};

impl SessionRegistry {
    pub(in crate::daemon) fn attach(
        &self,
        request: SessionAttach,
    ) -> std::result::Result<AttachmentAck, SessionError> {
        let ack = self.subscribe(SessionSubscribe {
            session_id: request.session_id,
            after_sequence: request.after_sequence,
        })?;
        Ok(AttachmentAck {
            session_id: ack.session_id,
            next_sequence: ack.next_sequence,
            read_only: true,
        })
    }

    pub(in crate::daemon) fn acquire_lease(
        &self,
        request: SessionLeaseAcquire,
    ) -> std::result::Result<ControlLease, SessionError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state.sessions.get(&request.session_id).ok_or_else(|| {
            SessionError::NotFound(format!("unknown session: {}", request.session_id))
        })?;
        if !matches!(session.info.state, SessionState::Running) {
            return Err(SessionError::InvalidState(format!(
                "session is not running: {}",
                request.session_id
            )));
        }
        drop(state);
        self.leases
            .acquire(&request.session_id, request.holder, request.ttl_ms)
            .map_err(SessionError::from)
    }

    pub(in crate::daemon) fn release_lease(
        &self,
        request: SessionLeaseRelease,
    ) -> std::result::Result<Value, SessionError> {
        self.leases
            .release(&request.session_id, &request.lease_id)
            .map_err(SessionError::from)?;
        Ok(json!({ "released": true }))
    }

    pub(in crate::daemon) fn control_write(
        &self,
        request: LeasedTerminalWrite,
    ) -> std::result::Result<usize, SessionError> {
        let session_id = request.session_id.clone();
        self.leases.with_lease(&session_id, &request.lease_id, || {
            self.write(TerminalWrite {
                session_id: request.session_id,
                data: request.data,
            })
        })
    }

    pub(in crate::daemon) fn control_write_bytes(
        &self,
        request: LeasedTerminalWriteBytes,
    ) -> std::result::Result<usize, SessionError> {
        let session_id = request.session_id.clone();
        self.leases.with_lease(&session_id, &request.lease_id, || {
            self.write_bytes(TerminalWriteBytes {
                session_id: request.session_id,
                data: request.data,
            })
        })
    }

    pub(in crate::daemon) fn control_key(
        &self,
        request: LeasedTerminalKey,
    ) -> std::result::Result<usize, SessionError> {
        let session_id = request.session_id.clone();
        self.leases.with_lease(&session_id, &request.lease_id, || {
            self.key(TerminalKey {
                session_id: request.session_id,
                key: request.key,
                modifiers: request.modifiers,
            })
        })
    }

    pub(in crate::daemon) fn control_resize(
        &self,
        request: LeasedTerminalResize,
    ) -> std::result::Result<TerminalSnapshot, SessionError> {
        let session_id = request.session_id.clone();
        self.leases.with_lease(&session_id, &request.lease_id, || {
            self.resize(TerminalResize {
                session_id: request.session_id,
                columns: request.columns,
                rows: request.rows,
            })
        })
    }

    pub(in crate::daemon) fn snapshot(
        &self,
        selector: SessionSelector,
    ) -> std::result::Result<TerminalSnapshot, SessionError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state.sessions.get(&selector.session_id).ok_or_else(|| {
            SessionError::NotFound(format!("unknown session: {}", selector.session_id))
        })?;
        Ok(session.snapshot())
    }

    pub(in crate::daemon) fn diagnostic(
        &self,
        selector: SessionSelector,
    ) -> std::result::Result<DiagnosticSnapshot, SessionError> {
        let mut state = self.state.lock().expect("session registry poisoned");
        let (session, transitioned) = {
            let session = state
                .sessions
                .get_mut(&selector.session_id)
                .ok_or_else(|| {
                    SessionError::NotFound(format!("unknown session: {}", selector.session_id))
                })?;
            let transitioned = session.refresh()?;
            (session.info.clone(), transitioned)
        };
        if transitioned {
            publish_lifecycle(&self.events, &session);
            record_tombstone(&mut state, selector.session_id.clone());
            self.leases.release_session(&selector.session_id);
        }
        let terminal = state
            .sessions
            .get(&selector.session_id)
            .ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", selector.session_id))
            })?
            .snapshot();
        Ok(DiagnosticSnapshot {
            protocol: DaemonInfo {
                name: "vivi-ptyd".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                protocol_version: PROTOCOL_VERSION,
            },
            session,
            terminal,
        })
    }

    pub(in crate::daemon) fn subscribe(
        &self,
        request: SessionSubscribe,
    ) -> std::result::Result<SubscriptionAck, SessionError> {
        let state = self.state.lock().expect("session registry poisoned");
        if !state.sessions.contains_key(&request.session_id) {
            return Err(SessionError::NotFound(format!(
                "unknown session: {}",
                request.session_id
            )));
        }
        let latest = self.events.latest_sequence(&request.session_id);
        Ok(SubscriptionAck {
            session_id: request.session_id,
            next_sequence: request.after_sequence.min(latest),
        })
    }

    pub(in crate::daemon) fn event_batch(
        &self,
        selector: SessionSelector,
        after_sequence: u64,
    ) -> std::result::Result<EventBatch, SessionError> {
        {
            let state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !state.sessions.contains_key(&selector.session_id) {
                return Err(SessionError::NotFound(format!(
                    "unknown session: {}",
                    selector.session_id
                )));
            }
        }
        let mut batch = self.events.batch(&selector.session_id, after_sequence);
        if batch.lagged {
            batch.snapshot = Some(self.diagnostic(selector)?);
        }
        Ok(batch)
    }

    pub(in crate::daemon) fn wait(
        &self,
        request: SessionWait,
    ) -> std::result::Result<DiagnosticSnapshot, SessionError> {
        let predicates = usize::from(request.state.is_some())
            + usize::from(request.screen_revision.is_some())
            + usize::from(request.event_sequence.is_some());
        if predicates != 1 {
            return Err(SessionError::InvalidInput(
                "wait requires exactly one predicate".into(),
            ));
        }
        if request.timeout_ms > 30_000 {
            return Err(SessionError::InvalidInput(
                "wait timeout cannot exceed 30000 milliseconds".into(),
            ));
        }
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(request.timeout_ms))
            .ok_or_else(|| SessionError::InvalidInput("wait timeout is too large".into()))?;
        let selector = SessionSelector {
            session_id: request.session_id,
        };
        loop {
            let snapshot = self.diagnostic(selector.clone())?;
            let matches = request
                .state
                .as_ref()
                .is_some_and(|state| snapshot.session.state == *state)
                || request
                    .screen_revision
                    .is_some_and(|revision| snapshot.terminal.screen_revision >= revision)
                || request.event_sequence.is_some_and(|sequence| {
                    self.events.latest_sequence(&selector.session_id) >= sequence
                });
            if matches {
                return Ok(snapshot);
            }
            if std::time::Instant::now() >= deadline {
                return Err(SessionError::Timeout(format!(
                    "wait timed out for session {}",
                    selector.session_id
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    pub(in crate::daemon) fn with_operation<F>(
        &self,
        operation_id: &str,
        fingerprint: &serde_json::Value,
        request_id: serde_json::Value,
        session_id: Option<&str>,
        method: &str,
        execute: F,
    ) -> Response
    where
        F: FnOnce() -> Response,
    {
        if let Err(message) = crate::operation::validate_operation_id(operation_id) {
            return Response::error(request_id, error_codes::INVALID_PARAMS, message);
        }
        let slot = {
            let mut operations = self
                .operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            operations.reserve(operation_id, fingerprint)
        };
        match slot {
            OperationSlot::Hit(response) => Response {
                id: request_id,
                ..response
            },
            OperationSlot::Conflict => {
                let mut response = Response::error(
                    request_id,
                    error_codes::OPERATION_CONFLICT,
                    "operation_id was already used for a different request",
                );
                response.operation_id = Some(operation_id.to_string());
                response
            }
            OperationSlot::Coalesce(pending) => {
                let response = pending.wait_for_response();
                Response {
                    id: request_id,
                    ..response
                }
            }
            OperationSlot::Own => {
                let mut response = execute();
                response.operation_id = Some(operation_id.to_string());
                let mut operations = self
                    .operations
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                operations.complete(operation_id, fingerprint, response.clone());
                drop(operations);
                self.publish_operation(session_id, operation_id, method, &response);
                response
            }
        }
    }

    pub(in crate::daemon) fn publish_operation(
        &self,
        session_id: Option<&str>,
        operation_id: &str,
        method: &str,
        response: &crate::protocol::Response,
    ) {
        let Some(session_id) = session_id else {
            return;
        };
        self.events.publish(
            session_id,
            SessionEventKind::Operation {
                operation_id: operation_id.into(),
                method: method.into(),
                success: response.error.is_none(),
                error_code: response.error.as_ref().map(|error| error.code),
            },
        );
    }
}
