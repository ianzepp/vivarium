//! `vivi step`: mechanical adjudication of the backlog graph into a
//! dispatch/exception manifest for the coordination host. Shadow form is
//! read-only — no mutation, no network, no provider.

use serde::Serialize;

use super::Mailspace;
use super::graph::GraphNodeView;
use crate::error::VivariumError;
use crate::storage::BACKLOG_GRAPH_CODE;

/// One dispatchable ready node with its mechanical checks.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StepDispatch {
    pub node: String,
    pub item: String,
    pub kind: String,
    pub subject: String,
    pub done_when_clauses: usize,
    pub write_scope: bool,
}

/// One ready node that needs attention instead of a seat.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StepException {
    pub node: String,
    pub item: String,
    pub kind: String,
    pub reason: String,
    pub detail: String,
}

/// The full step manifest. `decisions` is empty in shadow form; apply mode
/// (Phase 05) fills it without changing the shape.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StepManifest {
    pub graph: String,
    pub dispatches: Vec<StepDispatch>,
    pub exceptions: Vec<StepException>,
    pub decisions: Vec<String>,
}

impl Mailspace {
    /// Adjudicate ready backlog nodes mechanically. Read-only.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on storage failure.
    pub fn step_shadow(&self) -> Result<StepManifest, VivariumError> {
        let storage = self.storage()?;
        let Some(graph) = storage.work_graph_by_code(BACKLOG_GRAPH_CODE)? else {
            return Ok(StepManifest {
                graph: BACKLOG_GRAPH_CODE.to_string(),
                dispatches: Vec::new(),
                exceptions: Vec::new(),
                decisions: Vec::new(),
            });
        };
        let show = self.graph_show(BACKLOG_GRAPH_CODE)?;
        let mut dispatches = Vec::new();
        let mut exceptions = Vec::new();
        for node in &show.ready {
            match adjudicate_node(&storage, &graph.handle, node)? {
                StepOutcome::Dispatch(dispatch) => dispatches.push(dispatch),
                StepOutcome::Exception(exc) => exceptions.push(exc),
            }
        }
        Ok(StepManifest {
            graph: graph.code,
            dispatches,
            exceptions,
            decisions: Vec::new(),
        })
    }
}

/// Per-node adjudication result.
enum StepOutcome {
    Dispatch(StepDispatch),
    Exception(StepException),
}

fn adjudicate_node(
    storage: &crate::storage::Storage,
    graph_handle: &str,
    node: &GraphNodeView,
) -> Result<StepOutcome, VivariumError> {
    let message = match storage.resolve_message_token(&node.source_id) {
        Ok(message_id) => storage.message_by_id(&message_id)?,
        Err(_) => None,
    };
    let Some(message) = message else {
        return Ok(StepOutcome::Exception(exception(
            node,
            "unknown",
            "item_missing",
            "no message resolves to this node",
        )));
    };
    let kind = kind_for_role(&message.local_role);
    let body = item_body(storage, &message.message_id)?;
    let clauses = count_labeled_clauses(&body, "done_when");
    if kind == "want" {
        return Ok(StepOutcome::Exception(exception(
            node,
            &kind,
            "want_requires_promotion",
            "wants never dispatch before explicit promotion",
        )));
    }
    if !storage
        .work_graph_nodes_by_subgraph(graph_handle, &node.source_id)?
        .is_empty()
    {
        return Ok(StepOutcome::Exception(exception(
            node,
            &kind,
            "lowered_awaiting_units",
            "completion derives from bound unit tasks",
        )));
    }
    if clauses == 0 {
        return Ok(StepOutcome::Exception(exception(
            node,
            &kind,
            "no_done_when",
            "body declares no done_when clause; completion could not be verified",
        )));
    }
    Ok(StepOutcome::Dispatch(StepDispatch {
        node: node.handle.clone(),
        item: node.source_id.clone(),
        kind,
        subject: message.subject,
        done_when_clauses: clauses,
        write_scope: has_labeled_field(&body, "write_scope"),
    }))
}

fn kind_for_role(role: &str) -> String {
    match role {
        "tasks" => "task".into(),
        "needs" => "need".into(),
        "wants" => "want".into(),
        other => other.into(),
    }
}

fn exception(node: &GraphNodeView, kind: &str, reason: &str, detail: &str) -> StepException {
    StepException {
        node: node.handle.clone(),
        item: node.source_id.clone(),
        kind: kind.to_string(),
        reason: reason.to_string(),
        detail: detail.to_string(),
    }
}

fn item_body(storage: &crate::storage::Storage, message_id: &str) -> Result<String, VivariumError> {
    let data = storage.read_message(message_id)?;
    let extracted = crate::extract::extract_text(&data)?;
    Ok(extracted.body_text)
}

fn count_labeled_clauses(body: &str, label: &str) -> usize {
    body.lines()
        .filter(|line| {
            line.trim_start()
                .to_ascii_lowercase()
                .starts_with(&format!("{label}:"))
        })
        .count()
}

fn has_labeled_field(body: &str, label: &str) -> bool {
    body.lines().any(|line| {
        line.trim_start()
            .to_ascii_lowercase()
            .starts_with(&format!("{label}:"))
    })
}

#[cfg(test)]
#[path = "step_test.rs"]
mod tests;
