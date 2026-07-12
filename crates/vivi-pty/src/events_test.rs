use super::*;

#[test]
fn event_history_is_ordered_bounded_and_reports_lag() {
    let hub = EventHub::default();
    for _ in 0..(MAX_EVENT_HISTORY + 2) {
        hub.publish(
            "session",
            SessionEventKind::Screen {
                screen_revision: 1,
                output_sequence: 1,
            },
        );
    }

    let lagged = hub.batch("session", 0);
    assert!(lagged.lagged);
    assert!(lagged.events.is_empty());
    assert_eq!(lagged.latest_sequence, (MAX_EVENT_HISTORY + 2) as u64);

    let current = hub.batch("session", lagged.latest_sequence - 1);
    assert!(!current.lagged);
    assert_eq!(current.events.len(), 1);
    assert_eq!(current.events[0].sequence, lagged.latest_sequence);
}
