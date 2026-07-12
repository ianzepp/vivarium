use crate::protocol::{EventBatch, SessionEvent, SessionEventKind};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub(crate) const MAX_EVENT_HISTORY: usize = 256;

#[derive(Default)]
pub(crate) struct EventHub {
    histories: Mutex<HashMap<String, EventHistory>>,
}

#[derive(Default)]
struct EventHistory {
    next_sequence: u64,
    events: VecDeque<SessionEvent>,
}

impl EventHistory {
    fn next_sequence(&mut self) -> u64 {
        self.next_sequence = self.next_sequence.saturating_add(1).max(1);
        self.next_sequence
    }
}

impl EventHub {
    pub(crate) fn publish(&self, session_id: impl Into<String>, kind: SessionEventKind) -> u64 {
        let session_id = session_id.into();
        let mut histories = self
            .histories
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let history = histories.entry(session_id.clone()).or_default();
        let sequence = history.next_sequence();
        history.events.push_back(SessionEvent {
            session_id,
            sequence,
            kind,
        });
        while history.events.len() > MAX_EVENT_HISTORY {
            history.events.pop_front();
        }
        sequence
    }

    pub(crate) fn latest_sequence(&self, session_id: &str) -> u64 {
        let histories = self
            .histories
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        histories
            .get(session_id)
            .and_then(|history| history.events.back().map(|event| event.sequence))
            .unwrap_or(0)
    }

    pub(crate) fn batch(&self, session_id: &str, after_sequence: u64) -> EventBatch {
        let histories = self
            .histories
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(history) = histories.get(session_id) else {
            return EventBatch {
                session_id: session_id.into(),
                events: Vec::new(),
                latest_sequence: 0,
                lagged: false,
                snapshot: None,
            };
        };
        let latest_sequence = history
            .events
            .back()
            .map(|event| event.sequence)
            .unwrap_or(0);
        let oldest_sequence = history
            .events
            .front()
            .map(|event| event.sequence)
            .unwrap_or(latest_sequence.saturating_add(1));
        let lagged = after_sequence < oldest_sequence.saturating_sub(1);
        let events = if lagged {
            Vec::new()
        } else {
            history
                .events
                .iter()
                .filter(|event| event.sequence > after_sequence)
                .cloned()
                .collect()
        };
        EventBatch {
            session_id: session_id.into(),
            events,
            latest_sequence,
            lagged,
            snapshot: None,
        }
    }
}

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;
