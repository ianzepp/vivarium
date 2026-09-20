//! Backlog citizenship: transactional minting of needs/wants into the
//! `backlog` work graph without Mermaid re-export round-trips.

use std::collections::HashSet;

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};

use super::Storage;
use super::graph::{insert_edges, insert_work_graph, node_handle_for};
use super::{VivariumError, WorkGraphEdgeInput, WorkGraphImportInput, sha256_hex};

/// Project-unique code of the per-mailspace backlog graph.
pub const BACKLOG_GRAPH_CODE: &str = "backlog";

/// One backlog node to mint, carrying an explicit initial state so
/// retroactive dependencies of already-done items unlock immediately.
#[derive(Debug, Clone)]
pub struct BacklogNodeInput {
    pub source_id: String,
    pub label: String,
    pub state: String,
    pub kind: String,
}

/// Mint request: nodes plus prerequisite→dependent edges between them.
#[derive(Debug, Clone)]
pub struct BacklogMintInput {
    pub nodes: Vec<BacklogNodeInput>,
    pub edges: Vec<WorkGraphEdgeInput>,
}

/// Result of a backlog mint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogMintCommit {
    pub graph_handle: String,
    pub created_graph: bool,
    pub minted_nodes: Vec<String>,
    pub minted_edges: usize,
}

impl Storage {
    /// Mint nodes and edges into the `backlog` graph in one transaction,
    /// creating the graph (revision 1 with synthesized Mermaid evidence) on
    /// first use. Existing nodes and edges are skipped, so the call is
    /// idempotent.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the transaction fails.
    pub fn mint_backlog(
        &mut self,
        input: &BacklogMintInput,
    ) -> Result<BacklogMintCommit, VivariumError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| VivariumError::Other(format!("failed to begin backlog mint: {e}")))?;
        let commit = mint_backlog_tx(&tx, input)?;
        tx.commit()
            .map_err(|e| VivariumError::Other(format!("failed to commit backlog mint: {e}")))?;
        Ok(commit)
    }
}

fn mint_backlog_tx(
    tx: &Transaction<'_>,
    input: &BacklogMintInput,
) -> Result<BacklogMintCommit, VivariumError> {
    let now = Utc::now().to_rfc3339();
    let (graph_handle, created_graph) = ensure_backlog_graph(tx, input)?;
    let existing_nodes = existing_node_ids(tx, &graph_handle)?;
    let mut minted_nodes = Vec::new();
    for node in &input.nodes {
        if existing_nodes.contains(&node.source_id) {
            continue;
        }
        insert_backlog_node(tx, &graph_handle, node, &now)?;
        minted_nodes.push(node_handle_for(&graph_handle, &node.source_id));
    }
    let existing_edges = existing_edge_pairs(tx, &graph_handle)?;
    let new_edges: Vec<WorkGraphEdgeInput> = input
        .edges
        .iter()
        .filter(|edge| {
            let from = node_handle_for(&graph_handle, &edge.from_source_id);
            let to = node_handle_for(&graph_handle, &edge.to_source_id);
            !existing_edges.contains(&(from, to))
        })
        .cloned()
        .collect();
    let minted_edges = new_edges.len();
    insert_edges(tx, &graph_handle, &new_edges, &now)?;
    if created_graph || !minted_nodes.is_empty() || minted_edges > 0 {
        tx.execute(
            "UPDATE work_graphs SET updated_at = ?1 WHERE handle = ?2",
            params![now, graph_handle],
        )
        .map_err(|e| VivariumError::Other(format!("failed to touch backlog graph: {e}")))?;
    }
    Ok(BacklogMintCommit {
        graph_handle,
        created_graph,
        minted_nodes,
        minted_edges,
    })
}

fn ensure_backlog_graph(
    tx: &Transaction<'_>,
    input: &BacklogMintInput,
) -> Result<(String, bool), VivariumError> {
    if let Some(handle) = existing_backlog_handle(tx)? {
        return Ok((handle, false));
    }
    let source = synthesize_backlog_mermaid(&input.nodes, &input.edges);
    let commit = insert_work_graph(
        tx,
        &WorkGraphImportInput {
            code: BACKLOG_GRAPH_CODE.to_string(),
            mermaid_source: source.clone(),
            content_hash: sha256_hex(source.as_bytes()),
            nodes: Vec::new(),
            edges: Vec::new(),
        },
    )?;
    Ok((commit.graph.handle, true))
}

fn existing_backlog_handle(tx: &Transaction<'_>) -> Result<Option<String>, VivariumError> {
    tx.query_row(
        "SELECT handle FROM work_graphs WHERE code = ?1",
        params![BACKLOG_GRAPH_CODE],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| VivariumError::Other(format!("failed to load backlog graph: {e}")))
}

fn existing_node_ids(
    tx: &Transaction<'_>,
    graph_handle: &str,
) -> Result<HashSet<String>, VivariumError> {
    let mut stmt = tx
        .prepare("SELECT source_id FROM work_graph_nodes WHERE graph_handle = ?1")
        .map_err(|e| VivariumError::Other(format!("failed to prepare backlog nodes: {e}")))?;
    let rows = stmt
        .query_map(params![graph_handle], |row| row.get::<_, String>(0))
        .map_err(|e| VivariumError::Other(format!("failed to query backlog nodes: {e}")))?;
    let mut out = HashSet::new();
    for row in rows {
        out.insert(
            row.map_err(|e| VivariumError::Other(format!("failed to read backlog node: {e}")))?,
        );
    }
    Ok(out)
}

fn existing_edge_pairs(
    tx: &Transaction<'_>,
    graph_handle: &str,
) -> Result<HashSet<(String, String)>, VivariumError> {
    let mut stmt = tx
        .prepare("SELECT from_node, to_node FROM work_graph_edges WHERE graph_handle = ?1")
        .map_err(|e| VivariumError::Other(format!("failed to prepare backlog edges: {e}")))?;
    let rows = stmt
        .query_map(params![graph_handle], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| VivariumError::Other(format!("failed to query backlog edges: {e}")))?;
    let mut out = HashSet::new();
    for row in rows {
        let pair =
            row.map_err(|e| VivariumError::Other(format!("failed to read backlog edge: {e}")))?;
        out.insert(pair);
    }
    Ok(out)
}

fn insert_backlog_node(
    tx: &Transaction<'_>,
    graph_handle: &str,
    node: &BacklogNodeInput,
    now: &str,
) -> Result<(), VivariumError> {
    let handle = node_handle_for(graph_handle, &node.source_id);
    tx.execute(
        "INSERT INTO work_graph_nodes
           (handle, graph_handle, source_id, label, state, subgraph, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)",
        params![
            handle,
            graph_handle,
            node.source_id,
            node.label,
            node.state,
            now
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to insert backlog node: {e}")))?;
    tx.execute(
        "INSERT INTO work_graph_events (graph_handle, occurred_at, event_type, node_handle, note)
         VALUES (?1, ?2, 'backlog_attached', ?3, ?4)",
        params![graph_handle, now, handle, format!("kind={}", node.kind)],
    )
    .map_err(|e| VivariumError::Other(format!("failed to insert backlog event: {e}")))?;
    Ok(())
}

fn synthesize_backlog_mermaid(nodes: &[BacklogNodeInput], edges: &[WorkGraphEdgeInput]) -> String {
    let mut out = String::from("flowchart TD\n");
    for node in nodes {
        let label = node.label.replace('"', "'");
        out.push_str(&format!("  {}[\"{label}\"]\n", node.source_id));
    }
    for edge in edges {
        out.push_str(&format!(
            "  {} --> {}\n",
            edge.from_source_id, edge.to_source_id
        ));
    }
    out
}
