use super::*;

impl SessionRegistry {
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

    pub(in crate::daemon) fn replay_operation(
        &self,
        operation_id: &str,
        fingerprint: &serde_json::Value,
    ) -> std::result::Result<Replay, SessionError> {
        validate_operation_id(operation_id).map_err(SessionError::InvalidInput)?;
        let operations = self
            .operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(operations.lookup(operation_id, fingerprint))
    }

    pub(in crate::daemon) fn record_operation(
        &self,
        operation_id: String,
        fingerprint: serde_json::Value,
        response: crate::protocol::Response,
    ) {
        let mut operations = self
            .operations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operations.insert(operation_id, fingerprint, response);
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
