use super::*;
use std::thread;
use std::time::Duration;

#[test]
fn one_session_has_one_exclusive_expiring_lease() {
    let manager = LeaseManager::default();
    let first = manager.acquire("session", "operator-a".into(), 50).unwrap();
    assert_eq!(first.holder, "operator-a");
    assert!(matches!(
        manager.acquire("session", "operator-b".into(), 50),
        Err(LeaseError::Busy(_))
    ));
    manager.validate("session", &first.lease_id).unwrap();
    thread::sleep(Duration::from_millis(60));
    assert!(matches!(
        manager.validate("session", &first.lease_id),
        Err(LeaseError::Expired(_))
    ));
    let second = manager.acquire("session", "operator-b".into(), 50).unwrap();
    assert_ne!(first.lease_id, second.lease_id);
}

#[test]
fn release_requires_the_current_lease_token() {
    let manager = LeaseManager::default();
    let lease = manager
        .acquire("session", "operator".into(), 1_000)
        .unwrap();
    assert!(matches!(
        manager.release("session", "wrong"),
        Err(LeaseError::NotFound(_))
    ));
    manager.release("session", &lease.lease_id).unwrap();
    assert!(matches!(
        manager.validate("session", &lease.lease_id),
        Err(LeaseError::NotFound(_))
    ));
}

#[test]
fn lease_input_and_duration_are_bounded() {
    let manager = LeaseManager::default();
    assert!(matches!(
        manager.acquire("session", " ".into(), 1_000),
        Err(LeaseError::InvalidInput(_))
    ));
    assert!(matches!(
        manager.acquire("session", "operator".into(), 0),
        Err(LeaseError::InvalidInput(_))
    ));
    assert!(matches!(
        manager.acquire("session", "operator".into(), MAX_LEASE_TTL_MS + 1),
        Err(LeaseError::InvalidInput(_))
    ));
}
