use super::*;

const FANOUT: &str = r#"
flowchart LR
  subgraph mir["mir-swarm"]
    verify["G-P-10/U1/reverify"]
    accept["G-P-10/U1/accept"]
    u2["G-P-10/U2"]
    u3["G-P-10/U3"]
    verify --> accept
    accept --> u2
    accept --> u3
  end
"#;

#[test]
fn parses_fanout_fixture() {
    let g = parse_flowchart(FANOUT).unwrap();
    assert_eq!(g.direction, "LR");
    assert_eq!(g.nodes.len(), 4);
    assert_eq!(g.edges.len(), 3);
    let verify = g.nodes.iter().find(|n| n.source_id == "verify").unwrap();
    assert_eq!(verify.label, "G-P-10/U1/reverify");
    assert_eq!(verify.subgraph.as_deref(), Some("mir"));
}

#[test]
fn rejects_cycle() {
    let src = "flowchart TD\na --> b\nb --> a\n";
    let err = parse_flowchart(src).unwrap_err().to_string();
    assert!(err.contains("cycle"), "{err}");
}

#[test]
fn rejects_unsupported_diagram() {
    let err = parse_flowchart("sequenceDiagram\nA->>B: hi\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains("unsupported"), "{err}");
}

#[test]
fn parses_chained_edges() {
    let g = parse_flowchart("flowchart TD\na --> b --> c\n").unwrap();
    assert_eq!(g.edges.len(), 2);
    assert!(g.edges.iter().any(|e| e.from == "a" && e.to == "b"));
    assert!(g.edges.iter().any(|e| e.from == "b" && e.to == "c"));
}

#[test]
fn parses_edge_label() {
    let g = parse_flowchart("flowchart LR\na -->|gate| b\n").unwrap();
    assert_eq!(g.edges[0].label.as_deref(), Some("gate"));
}
