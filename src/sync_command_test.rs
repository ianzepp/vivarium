use super::*;
use serde_json::Value;

#[test]
fn sync_json_renders_single_account_as_object() {
    let report = test_report("agent-proton");
    let json: Value = serde_json::from_str(&render_json_reports(&[report])).unwrap();

    assert_eq!(json["account"], "agent-proton");
    assert_eq!(json["sync"]["new"], 2);
    assert_eq!(json["sync"]["decryption_errors"], 0);
    assert_eq!(json["index"], Value::Null);
    assert_eq!(json["embeddings"], Value::Null);
}

#[test]
fn sync_json_renders_multiple_accounts_as_array() {
    let json: Value = serde_json::from_str(&render_json_reports(&[
        test_report("first"),
        test_report("second"),
    ]))
    .unwrap();

    assert_eq!(json.as_array().unwrap().len(), 2);
    assert_eq!(json[0]["account"], "first");
    assert_eq!(json[1]["account"], "second");
}

#[test]
fn sync_json_includes_post_processing_reports() {
    let mut report = test_report("semantic");
    report.index = Some(IndexCountReport {
        scanned: 3,
        updated: 2,
        reused: 1,
        stale: 0,
        errors: 0,
    });
    report.embeddings = Some(EmbeddingCountReport {
        scanned: 3,
        reused: 1,
        embedded: 2,
        stale: 0,
        errors: 0,
    });

    let json: Value = serde_json::from_str(&render_json_reports(&[report])).unwrap();

    assert_eq!(json["index"]["updated"], 2);
    assert_eq!(json["embeddings"]["embedded"], 2);
}

fn test_report(account: &str) -> SyncReport {
    SyncReport {
        account: account.into(),
        sync: SyncCountReport {
            new: 2,
            archived: 0,
            cataloged: 2,
            extracted: 2,
            extraction_errors: 0,
            decryption_errors: 0,
        },
        index: None,
        embeddings: None,
    }
}
