//! Work-graph tables: durable topology, revisions, and graph events.

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;

use super::{Storage, VivariumError, sha256_hex};

/// Row stored for one work graph.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkGraphRow {
    pub handle: String,
    pub code: String,
    pub status: String,
    pub current_revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One node in a work graph.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkGraphNodeRow {
    pub handle: String,
    pub graph_handle: String,
    pub source_id: String,
    pub label: String,
    pub state: String,
    pub subgraph: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Directed dependency edge: `to_node` requires `from_node`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkGraphEdgeRow {
    pub handle: String,
    pub graph_handle: String,
    pub from_node: String,
    pub to_node: String,
    pub label: Option<String>,
    pub created_at: String,
}

/// Inputs for an atomic first import of a work graph.
#[derive(Debug, Clone)]
pub struct WorkGraphImportInput {
    pub code: String,
    pub mermaid_source: String,
    pub content_hash: String,
    pub nodes: Vec<WorkGraphNodeInput>,
    pub edges: Vec<WorkGraphEdgeInput>,
}

/// Node fields supplied by the Mermaid compiler before handles are assigned.
#[derive(Debug, Clone)]
pub struct WorkGraphNodeInput {
    pub source_id: String,
    pub label: String,
    pub subgraph: Option<String>,
}

/// Edge fields using Mermaid source IDs (resolved to handles on write).
#[derive(Debug, Clone)]
pub struct WorkGraphEdgeInput {
    pub from_source_id: String,
    pub to_source_id: String,
    pub label: Option<String>,
}

/// Result of committing a graph import.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkGraphImportCommit {
    pub graph: WorkGraphRow,
    pub nodes: Vec<WorkGraphNodeRow>,
    pub edges: Vec<WorkGraphEdgeRow>,
    pub created: bool,
}

impl Storage {
    /// Look up a work graph by project-unique code.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the query fails.
    pub fn work_graph_by_code(&self, code: &str) -> Result<Option<WorkGraphRow>, VivariumError> {
        self.conn
            .query_row(
                "SELECT handle, code, status, current_revision, created_at, updated_at
                 FROM work_graphs WHERE code = ?1",
                params![code],
                map_graph_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to load work graph by code: {e}")))
    }

    /// Look up a work graph by immutable handle.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the query fails.
    pub fn work_graph_by_handle(
        &self,
        handle: &str,
    ) -> Result<Option<WorkGraphRow>, VivariumError> {
        self.conn
            .query_row(
                "SELECT handle, code, status, current_revision, created_at, updated_at
                 FROM work_graphs WHERE handle = ?1",
                params![handle],
                map_graph_row,
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to load work graph by handle: {e}")))
    }

    /// Content hash of the graph's current revision, if any.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the query fails.
    pub fn work_graph_revision_hash(
        &self,
        graph_handle: &str,
        revision: i64,
    ) -> Result<Option<String>, VivariumError> {
        self.conn
            .query_row(
                "SELECT content_hash FROM work_graph_revisions
                 WHERE graph_handle = ?1 AND revision = ?2",
                params![graph_handle, revision],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to load graph revision hash: {e}")))
    }

    /// Load all nodes for a graph.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the query fails.
    pub fn work_graph_nodes(
        &self,
        graph_handle: &str,
    ) -> Result<Vec<WorkGraphNodeRow>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT handle, graph_handle, source_id, label, state, subgraph,
                        created_at, updated_at
                 FROM work_graph_nodes WHERE graph_handle = ?1
                 ORDER BY source_id",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare graph nodes: {e}")))?;
        let rows = stmt
            .query_map(params![graph_handle], map_node_row)
            .map_err(|e| VivariumError::Other(format!("failed to query graph nodes: {e}")))?;
        collect_rows(rows, "graph nodes")
    }

    /// Load all edges for a graph.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the query fails.
    pub fn work_graph_edges(
        &self,
        graph_handle: &str,
    ) -> Result<Vec<WorkGraphEdgeRow>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT handle, graph_handle, from_node, to_node, label, created_at
                 FROM work_graph_edges WHERE graph_handle = ?1
                 ORDER BY from_node, to_node",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare graph edges: {e}")))?;
        let rows = stmt
            .query_map(params![graph_handle], map_edge_row)
            .map_err(|e| VivariumError::Other(format!("failed to query graph edges: {e}")))?;
        collect_rows(rows, "graph edges")
    }

    /// Atomically insert a new work graph at revision 1.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] if the transaction fails.
    pub fn import_work_graph(
        &mut self,
        input: &WorkGraphImportInput,
    ) -> Result<WorkGraphImportCommit, VivariumError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| VivariumError::Other(format!("failed to begin graph import: {e}")))?;
        let commit = insert_work_graph(&tx, input)?;
        tx.commit()
            .map_err(|e| VivariumError::Other(format!("failed to commit graph import: {e}")))?;
        Ok(commit)
    }
}

/// Deterministic graph handle from project-unique code.
#[must_use]
pub fn graph_handle_for_code(code: &str) -> String {
    let digest = sha256_hex(format!("graph\0{code}").as_bytes());
    format!("gph_{}", &digest[..16])
}

/// Deterministic node handle from graph handle + Mermaid source id.
#[must_use]
pub fn node_handle_for(graph_handle: &str, source_id: &str) -> String {
    let digest = sha256_hex(format!("node\0{graph_handle}\0{source_id}").as_bytes());
    format!("nod_{}", &digest[..16])
}

/// Deterministic edge handle from graph + endpoint node handles.
#[must_use]
pub fn edge_handle_for(graph_handle: &str, from_node: &str, to_node: &str) -> String {
    let digest = sha256_hex(format!("edge\0{graph_handle}\0{from_node}\0{to_node}").as_bytes());
    format!("edg_{}", &digest[..16])
}

fn insert_work_graph(
    tx: &Transaction<'_>,
    input: &WorkGraphImportInput,
) -> Result<WorkGraphImportCommit, VivariumError> {
    let now = Utc::now().to_rfc3339();
    let graph_handle = graph_handle_for_code(&input.code);
    let graph = WorkGraphRow {
        handle: graph_handle.clone(),
        code: input.code.clone(),
        status: "open".into(),
        current_revision: 1,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    tx.execute(
        "INSERT INTO work_graphs (handle, code, status, current_revision, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            graph.handle,
            graph.code,
            graph.status,
            graph.current_revision,
            graph.created_at,
            graph.updated_at
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to insert work graph: {e}")))?;
    tx.execute(
        "INSERT INTO work_graph_revisions
           (graph_handle, revision, mermaid_source, content_hash, created_at)
         VALUES (?1, 1, ?2, ?3, ?4)",
        params![graph.handle, input.mermaid_source, input.content_hash, now],
    )
    .map_err(|e| VivariumError::Other(format!("failed to insert graph revision: {e}")))?;
    let nodes = insert_nodes(tx, &graph.handle, &input.nodes, &now)?;
    let edges = insert_edges(tx, &graph.handle, &input.edges, &now)?;
    tx.execute(
        "INSERT INTO work_graph_events (graph_handle, occurred_at, event_type, node_handle, note)
         VALUES (?1, ?2, 'revision_imported', NULL, ?3)",
        params![
            graph.handle,
            now,
            format!("revision=1 nodes={} edges={}", nodes.len(), edges.len())
        ],
    )
    .map_err(|e| VivariumError::Other(format!("failed to insert graph event: {e}")))?;
    Ok(WorkGraphImportCommit {
        graph,
        nodes,
        edges,
        created: true,
    })
}

fn insert_nodes(
    tx: &Transaction<'_>,
    graph_handle: &str,
    nodes: &[WorkGraphNodeInput],
    now: &str,
) -> Result<Vec<WorkGraphNodeRow>, VivariumError> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        let row = WorkGraphNodeRow {
            handle: node_handle_for(graph_handle, &node.source_id),
            graph_handle: graph_handle.to_string(),
            source_id: node.source_id.clone(),
            label: node.label.clone(),
            state: "open".into(),
            subgraph: node.subgraph.clone(),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        };
        tx.execute(
            "INSERT INTO work_graph_nodes
               (handle, graph_handle, source_id, label, state, subgraph, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.handle,
                row.graph_handle,
                row.source_id,
                row.label,
                row.state,
                row.subgraph,
                row.created_at,
                row.updated_at
            ],
        )
        .map_err(|e| VivariumError::Other(format!("failed to insert graph node: {e}")))?;
        out.push(row);
    }
    Ok(out)
}

fn insert_edges(
    tx: &Transaction<'_>,
    graph_handle: &str,
    edges: &[WorkGraphEdgeInput],
    now: &str,
) -> Result<Vec<WorkGraphEdgeRow>, VivariumError> {
    let mut out = Vec::with_capacity(edges.len());
    for edge in edges {
        let from_node = node_handle_for(graph_handle, &edge.from_source_id);
        let to_node = node_handle_for(graph_handle, &edge.to_source_id);
        let row = WorkGraphEdgeRow {
            handle: edge_handle_for(graph_handle, &from_node, &to_node),
            graph_handle: graph_handle.to_string(),
            from_node,
            to_node,
            label: edge.label.clone(),
            created_at: now.to_string(),
        };
        tx.execute(
            "INSERT INTO work_graph_edges
               (handle, graph_handle, from_node, to_node, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.handle,
                row.graph_handle,
                row.from_node,
                row.to_node,
                row.label,
                row.created_at
            ],
        )
        .map_err(|e| VivariumError::Other(format!("failed to insert graph edge: {e}")))?;
        out.push(row);
    }
    Ok(out)
}

fn map_graph_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkGraphRow> {
    Ok(WorkGraphRow {
        handle: row.get(0)?,
        code: row.get(1)?,
        status: row.get(2)?,
        current_revision: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn map_node_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkGraphNodeRow> {
    Ok(WorkGraphNodeRow {
        handle: row.get(0)?,
        graph_handle: row.get(1)?,
        source_id: row.get(2)?,
        label: row.get(3)?,
        state: row.get(4)?,
        subgraph: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn map_edge_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkGraphEdgeRow> {
    Ok(WorkGraphEdgeRow {
        handle: row.get(0)?,
        graph_handle: row.get(1)?,
        from_node: row.get(2)?,
        to_node: row.get(3)?,
        label: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn collect_rows<T, I>(rows: I, label: &str) -> Result<Vec<T>, VivariumError>
where
    I: Iterator<Item = Result<T, rusqlite::Error>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| VivariumError::Other(format!("failed to read {label}: {e}")))?);
    }
    Ok(out)
}
