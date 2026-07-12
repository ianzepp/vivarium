use super::*;

#[test]
fn operation_store_replays_same_fingerprint_and_rejects_conflicts() {
    let mut store = OperationStore::default();
    let response = Response::success(1.into(), serde_json::json!({ "ok": true }));
    let fingerprint = serde_json::json!({ "method": "session.stop", "id": "demo" });
    store.insert("op-1".into(), fingerprint.clone(), response.clone());

    assert!(matches!(store.lookup("op-1", &fingerprint), Replay::Hit(_)));
    assert!(matches!(
        store.lookup("op-1", &serde_json::json!({ "different": true })),
        Replay::Conflict
    ));
    assert!(matches!(store.lookup("op-2", &fingerprint), Replay::Miss));
}
