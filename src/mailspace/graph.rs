//! Work-graph domain: import, show, ready/blocked projections.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::Mailspace;
use super::mermaid::{MermaidFlowchart, parse_flowchart};
use crate::error::VivariumError;
use crate::storage::{
    Storage, WorkGraphEdgeInput, WorkGraphEdgeRow, WorkGraphImportCommit, WorkGraphImportInput,
    WorkGraphNodeInput, WorkGraphNodeRow, WorkGraphRow, sha256_hex,
};

/// Import report returned by check and commit paths.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphImportReport {
    pub check_only: bool,
    pub created: bool,
    pub idempotent: bool,
    pub graph_handle: String,
    pub code: String,
    pub revision: i64,
    pub content_hash: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub roots: Vec<String>,
    pub ready: Vec<GraphNodeView>,
    pub nodes: Vec<GraphNodeView>,
    pub edges: Vec<GraphEdgeView>,
}

/// Show / projection view of a stored graph.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphShow {
    pub graph: WorkGraphRow,
    pub content_hash: Option<String>,
    pub nodes: Vec<GraphNodeView>,
    pub edges: Vec<GraphEdgeView>,
    pub ready: Vec<GraphNodeView>,
    pub blocked: Vec<GraphNodeView>,
}

/// Node with derived readiness fields.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphNodeView {
    pub handle: String,
    pub source_id: String,
    pub label: String,
    pub state: String,
    pub subgraph: Option<String>,
    pub readiness: String,
    pub blocked_by: Vec<String>,
    pub successors: Vec<String>,
}

/// Edge view using source IDs for agent readability.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphEdgeView {
    pub handle: String,
    pub from_handle: String,
    pub to_handle: String,
    pub from_source_id: String,
    pub to_source_id: String,
    pub label: Option<String>,
}

impl Mailspace {
    /// Parse, validate, and optionally commit a Mermaid work graph import.
    ///
    /// # Errors
    /// Returns parse, validation, conflict, or storage errors.
    pub fn graph_import(
        &self,
        code: &str,
        mermaid_source: &str,
        check_only: bool,
    ) -> Result<GraphImportReport, VivariumError> {
        validate_code(code)?;
        let flowchart = parse_flowchart(mermaid_source)?;
        let content_hash = sha256_hex(mermaid_source.as_bytes());
        let mut storage = self.storage()?;
        if let Some(existing) = storage.work_graph_by_code(code)? {
            return idempotent_or_conflict(
                &storage,
                &existing,
                &content_hash,
                &flowchart,
                check_only,
            );
        }
        let input = compile_import(code, mermaid_source, &content_hash, &flowchart);
        if check_only {
            return Ok(preview_import(&input, &flowchart, true));
        }
        let commit = storage.import_work_graph(&input)?;
        Ok(report_from_commit(&commit, &content_hash, false, false))
    }

    /// Import Mermaid from a file path.
    ///
    /// # Errors
    /// Returns IO, parse, or storage errors.
    pub fn graph_import_file(
        &self,
        code: &str,
        path: &Path,
        check_only: bool,
    ) -> Result<GraphImportReport, VivariumError> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            VivariumError::Other(format!("failed to read graph file {}: {e}", path.display()))
        })?;
        self.graph_import(code, &source, check_only)
    }

    /// Load a graph by code or handle and project readiness.
    ///
    /// # Errors
    /// Returns not-found or storage errors.
    pub fn graph_show(&self, code_or_handle: &str) -> Result<GraphShow, VivariumError> {
        let storage = self.storage()?;
        let graph = resolve_graph(&storage, code_or_handle)?;
        let nodes = storage.work_graph_nodes(&graph.handle)?;
        let edges = storage.work_graph_edges(&graph.handle)?;
        let hash = storage.work_graph_revision_hash(&graph.handle, graph.current_revision)?;
        let (node_views, ready, blocked) = project_nodes(&nodes, &edges);
        let edge_views = project_edges(&nodes, &edges);
        Ok(GraphShow {
            graph,
            content_hash: hash,
            nodes: node_views,
            edges: edge_views,
            ready,
            blocked,
        })
    }
}

/// Print import report as text or JSON.
///
/// # Errors
/// Returns JSON encode errors.
pub fn print_import_report(report: &GraphImportReport, json: bool) -> Result<(), VivariumError> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    let mode = if report.check_only {
        "check"
    } else if report.idempotent {
        "idempotent"
    } else {
        "imported"
    };
    println!("graph {mode}");
    println!("  handle   {}", report.graph_handle);
    println!("  code     {}", report.code);
    println!("  revision {}", report.revision);
    println!("  hash     {}", report.content_hash);
    println!("  nodes    {}", report.node_count);
    println!("  edges    {}", report.edge_count);
    println!("  roots    {}", report.roots.join(", ").if_empty("(none)"));
    println!(
        "  ready    {}",
        report
            .ready
            .iter()
            .map(|n| n.source_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
            .if_empty("(none)")
    );
    Ok(())
}

/// Print graph show as text or JSON.
///
/// # Errors
/// Returns JSON encode errors.
pub fn print_graph_show(show: &GraphShow, json: bool) -> Result<(), VivariumError> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(show)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    println!("graph {}", show.graph.code);
    println!("  handle   {}", show.graph.handle);
    println!("  status   {}", show.graph.status);
    println!("  revision {}", show.graph.current_revision);
    if let Some(hash) = &show.content_hash {
        println!("  hash     {hash}");
    }
    println!("nodes:");
    for node in &show.nodes {
        println!(
            "  {}  {}  {}  [{}]  blocked_by={}",
            node.source_id,
            node.handle,
            node.state,
            node.readiness,
            node.blocked_by.join(",").if_empty("-")
        );
    }
    println!("edges:");
    for edge in &show.edges {
        println!(
            "  {} --> {}{}",
            edge.from_source_id,
            edge.to_source_id,
            edge.label
                .as_ref()
                .map(|l| format!("  ({l})"))
                .unwrap_or_default()
        );
    }
    println!(
        "ready: {}",
        show.ready
            .iter()
            .map(|n| n.source_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
            .if_empty("(none)")
    );
    Ok(())
}

fn validate_code(code: &str) -> Result<(), VivariumError> {
    if code.is_empty() {
        return Err(VivariumError::Message(
            "graph code must not be empty".into(),
        ));
    }
    if !code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/')
    {
        return Err(VivariumError::Message(format!(
            "invalid graph code '{code}' (use [A-Za-z0-9_/-]+)"
        )));
    }
    Ok(())
}

fn compile_import(
    code: &str,
    mermaid_source: &str,
    content_hash: &str,
    flowchart: &MermaidFlowchart,
) -> WorkGraphImportInput {
    WorkGraphImportInput {
        code: code.to_string(),
        mermaid_source: mermaid_source.to_string(),
        content_hash: content_hash.to_string(),
        nodes: flowchart
            .nodes
            .iter()
            .map(|n| WorkGraphNodeInput {
                source_id: n.source_id.clone(),
                label: n.label.clone(),
                subgraph: n.subgraph.clone(),
            })
            .collect(),
        edges: flowchart
            .edges
            .iter()
            .map(|e| WorkGraphEdgeInput {
                from_source_id: e.from.clone(),
                to_source_id: e.to.clone(),
                label: e.label.clone(),
            })
            .collect(),
    }
}

fn preview_import(
    input: &WorkGraphImportInput,
    flowchart: &MermaidFlowchart,
    check_only: bool,
) -> GraphImportReport {
    let graph_handle = crate::storage::graph_handle_for_code(&input.code);
    let nodes: Vec<WorkGraphNodeRow> = flowchart
        .nodes
        .iter()
        .map(|n| WorkGraphNodeRow {
            handle: crate::storage::node_handle_for(&graph_handle, &n.source_id),
            graph_handle: graph_handle.clone(),
            source_id: n.source_id.clone(),
            label: n.label.clone(),
            state: "open".into(),
            subgraph: n.subgraph.clone(),
            created_at: String::new(),
            updated_at: String::new(),
        })
        .collect();
    let edges: Vec<WorkGraphEdgeRow> = flowchart
        .edges
        .iter()
        .map(|e| {
            let from_node = crate::storage::node_handle_for(&graph_handle, &e.from);
            let to_node = crate::storage::node_handle_for(&graph_handle, &e.to);
            WorkGraphEdgeRow {
                handle: crate::storage::edge_handle_for(&graph_handle, &from_node, &to_node),
                graph_handle: graph_handle.clone(),
                from_node,
                to_node,
                label: e.label.clone(),
                created_at: String::new(),
            }
        })
        .collect();
    let (node_views, ready, _) = project_nodes(&nodes, &edges);
    let edge_views = project_edges(&nodes, &edges);
    let roots: Vec<String> = ready.iter().map(|n| n.source_id.clone()).collect();
    GraphImportReport {
        check_only,
        created: false,
        idempotent: false,
        graph_handle,
        code: input.code.clone(),
        revision: 1,
        content_hash: input.content_hash.clone(),
        node_count: nodes.len(),
        edge_count: edges.len(),
        roots,
        ready,
        nodes: node_views,
        edges: edge_views,
    }
}

fn idempotent_or_conflict(
    storage: &Storage,
    existing: &WorkGraphRow,
    content_hash: &str,
    flowchart: &MermaidFlowchart,
    check_only: bool,
) -> Result<GraphImportReport, VivariumError> {
    let current_hash =
        storage.work_graph_revision_hash(&existing.handle, existing.current_revision)?;
    if current_hash.as_deref() == Some(content_hash) {
        let nodes = storage.work_graph_nodes(&existing.handle)?;
        let edges = storage.work_graph_edges(&existing.handle)?;
        let (node_views, ready, _) = project_nodes(&nodes, &edges);
        let edge_views = project_edges(&nodes, &edges);
        let roots: Vec<String> = ready.iter().map(|n| n.source_id.clone()).collect();
        return Ok(GraphImportReport {
            check_only,
            created: false,
            idempotent: true,
            graph_handle: existing.handle.clone(),
            code: existing.code.clone(),
            revision: existing.current_revision,
            content_hash: content_hash.to_string(),
            node_count: nodes.len(),
            edge_count: edges.len(),
            roots,
            ready,
            nodes: node_views,
            edges: edge_views,
        });
    }
    let _ = flowchart;
    Err(VivariumError::Message(format!(
        "graph code '{}' already exists (handle {}); use graph apply to revise",
        existing.code, existing.handle
    )))
}

fn report_from_commit(
    commit: &WorkGraphImportCommit,
    content_hash: &str,
    check_only: bool,
    idempotent: bool,
) -> GraphImportReport {
    let (node_views, ready, _) = project_nodes(&commit.nodes, &commit.edges);
    let edge_views = project_edges(&commit.nodes, &commit.edges);
    let roots: Vec<String> = ready.iter().map(|n| n.source_id.clone()).collect();
    GraphImportReport {
        check_only,
        created: commit.created,
        idempotent,
        graph_handle: commit.graph.handle.clone(),
        code: commit.graph.code.clone(),
        revision: commit.graph.current_revision,
        content_hash: content_hash.to_string(),
        node_count: commit.nodes.len(),
        edge_count: commit.edges.len(),
        roots,
        ready,
        nodes: node_views,
        edges: edge_views,
    }
}

fn resolve_graph(storage: &Storage, code_or_handle: &str) -> Result<WorkGraphRow, VivariumError> {
    if let Some(g) = storage.work_graph_by_code(code_or_handle)? {
        return Ok(g);
    }
    if let Some(g) = storage.work_graph_by_handle(code_or_handle)? {
        return Ok(g);
    }
    Err(VivariumError::Message(format!(
        "work graph not found: {code_or_handle}"
    )))
}

fn project_nodes(
    nodes: &[WorkGraphNodeRow],
    edges: &[WorkGraphEdgeRow],
) -> (Vec<GraphNodeView>, Vec<GraphNodeView>, Vec<GraphNodeView>) {
    let by_handle: HashMap<&str, &WorkGraphNodeRow> =
        nodes.iter().map(|n| (n.handle.as_str(), n)).collect();
    let (prereqs, successors) = adjacency(edges);
    let mut views = Vec::with_capacity(nodes.len());
    let mut ready = Vec::new();
    let mut blocked = Vec::new();
    for node in nodes {
        let view = node_view(node, &by_handle, &prereqs, &successors);
        match view.readiness.as_str() {
            "ready" => ready.push(view.clone()),
            "blocked" => blocked.push(view.clone()),
            _ => {}
        }
        views.push(view);
    }
    views.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    ready.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    blocked.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    (views, ready, blocked)
}

type NodeAdj<'a> = HashMap<&'a str, Vec<&'a str>>;

fn adjacency(edges: &[WorkGraphEdgeRow]) -> (NodeAdj<'_>, NodeAdj<'_>) {
    let mut prereqs: NodeAdj<'_> = HashMap::new();
    let mut successors: NodeAdj<'_> = HashMap::new();
    for edge in edges {
        prereqs
            .entry(edge.to_node.as_str())
            .or_default()
            .push(edge.from_node.as_str());
        successors
            .entry(edge.from_node.as_str())
            .or_default()
            .push(edge.to_node.as_str());
    }
    (prereqs, successors)
}

fn node_view(
    node: &WorkGraphNodeRow,
    by_handle: &HashMap<&str, &WorkGraphNodeRow>,
    prereqs: &NodeAdj<'_>,
    successors: &NodeAdj<'_>,
) -> GraphNodeView {
    let unfinished: Vec<String> = prereqs
        .get(node.handle.as_str())
        .into_iter()
        .flatten()
        .filter_map(|h| by_handle.get(h).copied())
        .filter(|p| p.state != "done")
        .map(|p| p.source_id.clone())
        .collect();
    let readiness = if node.state != "open" {
        "n/a".to_string()
    } else if unfinished.is_empty() {
        "ready".to_string()
    } else {
        "blocked".to_string()
    };
    let succ: Vec<String> = successors
        .get(node.handle.as_str())
        .into_iter()
        .flatten()
        .filter_map(|h| by_handle.get(h).map(|n| n.source_id.clone()))
        .collect();
    GraphNodeView {
        handle: node.handle.clone(),
        source_id: node.source_id.clone(),
        label: node.label.clone(),
        state: node.state.clone(),
        subgraph: node.subgraph.clone(),
        readiness,
        blocked_by: unfinished,
        successors: succ,
    }
}

fn project_edges(nodes: &[WorkGraphNodeRow], edges: &[WorkGraphEdgeRow]) -> Vec<GraphEdgeView> {
    let by_handle: HashMap<&str, &WorkGraphNodeRow> =
        nodes.iter().map(|n| (n.handle.as_str(), n)).collect();
    let mut out: Vec<GraphEdgeView> = edges
        .iter()
        .filter_map(|e| {
            let from = by_handle.get(e.from_node.as_str())?;
            let to = by_handle.get(e.to_node.as_str())?;
            Some(GraphEdgeView {
                handle: e.handle.clone(),
                from_handle: e.from_node.clone(),
                to_handle: e.to_node.clone(),
                from_source_id: from.source_id.clone(),
                to_source_id: to.source_id.clone(),
                label: e.label.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (&a.from_source_id, &a.to_source_id).cmp(&(&b.from_source_id, &b.to_source_id))
    });
    out
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample() -> &'static str {
        r#"
flowchart LR
  verify["reverify"]
  accept["accept"]
  u2["U2"]
  u3["U3"]
  verify --> accept
  accept --> u2
  accept --> u3
"#
    }

    #[test]
    fn import_and_show_ready_roots() {
        let dir = tempdir().unwrap();
        let ms = Mailspace::init(Some(dir.path())).unwrap();
        let report = ms.graph_import("demo", sample(), false).unwrap();
        assert!(report.created);
        assert_eq!(report.node_count, 4);
        assert_eq!(report.edge_count, 3);
        assert_eq!(report.ready.len(), 1);
        assert_eq!(report.ready[0].source_id, "verify");

        let show = ms.graph_show("demo").unwrap();
        assert_eq!(show.ready.len(), 1);
        assert_eq!(show.blocked.len(), 3);
        let accept = show.nodes.iter().find(|n| n.source_id == "accept").unwrap();
        assert_eq!(accept.blocked_by, vec!["verify".to_string()]);
    }

    #[test]
    fn check_only_writes_nothing() {
        let dir = tempdir().unwrap();
        let ms = Mailspace::init(Some(dir.path())).unwrap();
        let report = ms.graph_import("demo", sample(), true).unwrap();
        assert!(report.check_only);
        assert!(ms.graph_show("demo").is_err());
    }

    #[test]
    fn idempotent_reimport() {
        let dir = tempdir().unwrap();
        let ms = Mailspace::init(Some(dir.path())).unwrap();
        let first = ms.graph_import("demo", sample(), false).unwrap();
        let second = ms.graph_import("demo", sample(), false).unwrap();
        assert!(second.idempotent);
        assert_eq!(first.graph_handle, second.graph_handle);
        assert_eq!(second.revision, 1);
    }

    #[test]
    fn conflict_on_different_source() {
        let dir = tempdir().unwrap();
        let ms = Mailspace::init(Some(dir.path())).unwrap();
        ms.graph_import("demo", sample(), false).unwrap();
        let err = ms
            .graph_import("demo", "flowchart TD\na --> b\n", false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "{err}");
    }
}
