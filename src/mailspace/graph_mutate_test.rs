use super::*;
use tempfile::tempdir;

fn base() -> &'static str {
    "flowchart LR\na[\"A\"]\nb[\"B\"]\na --> b\n"
}

#[test]
fn complete_unlocks_successor() {
    let dir = tempdir().unwrap();
    let ms = Mailspace::init(Some(dir.path())).unwrap();
    ms.graph_import("demo", base(), false).unwrap();
    let before = ms.graph_show("demo").unwrap();
    assert_eq!(before.ready.len(), 1);
    assert_eq!(before.ready[0].source_id, "a");
    let after = ms.graph_complete("demo", "a", Some("done")).unwrap();
    assert!(after.ready.iter().any(|n| n.source_id == "b"));
    let a = after.nodes.iter().find(|n| n.source_id == "a").unwrap();
    assert_eq!(a.state, "done");
}

#[test]
fn apply_adds_successor_to_done() {
    let dir = tempdir().unwrap();
    let ms = Mailspace::init(Some(dir.path())).unwrap();
    ms.graph_import("demo", base(), false).unwrap();
    ms.graph_complete("demo", "a", None).unwrap();
    ms.graph_complete("demo", "b", None).unwrap();
    let expanded = "flowchart LR\na[\"A\"]\nb[\"B\"]\nc[\"C\"]\na --> b\nb --> c\n";
    let report = ms.graph_apply("demo", expanded, false).unwrap();
    assert!(report.nodes_added.contains(&"c".to_string()));
    assert_eq!(report.revision, 2);
    let show = ms.graph_show("demo").unwrap();
    assert!(show.ready.iter().any(|n| n.source_id == "c"));
}

#[test]
fn apply_rejects_prereq_change_on_done() {
    let dir = tempdir().unwrap();
    let ms = Mailspace::init(Some(dir.path())).unwrap();
    ms.graph_import("demo", base(), false).unwrap();
    ms.graph_complete("demo", "a", None).unwrap();
    ms.graph_complete("demo", "b", None).unwrap();
    let bad = "flowchart LR\na[\"A\"]\nb[\"B\"]\nx[\"X\"]\nx --> b\n";
    let err = ms.graph_apply("demo", bad, false).unwrap_err().to_string();
    assert!(
        err.contains("prerequisite") || err.contains("remove"),
        "{err}"
    );
}

#[test]
fn export_round_trip_topology() {
    let dir = tempdir().unwrap();
    let ms = Mailspace::init(Some(dir.path())).unwrap();
    ms.graph_import("demo", base(), false).unwrap();
    let mermaid = ms.graph_export_mermaid("demo", false).unwrap();
    let again = ms.graph_apply("demo", &mermaid, false).unwrap();
    assert!(again.idempotent || again.revision >= 1);
    let show = ms.graph_show("demo").unwrap();
    assert_eq!(show.nodes.len(), 2);
    assert_eq!(show.edges.len(), 1);
}

#[test]
fn activate_refuses_blocked_node() {
    let dir = tempdir().unwrap();
    let ms = Mailspace::init(Some(dir.path())).unwrap();
    ms.graph_import("demo", base(), false).unwrap();
    let err = ms
        .graph_activate("demo", "b", "deadbeef", None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("blocked"), "{err}");
}
