//! Backlog citizenship: needs and wants as work-graph nodes in the
//! per-mailspace `backlog` graph, so readiness spans the whole backlog
//! without a separate graph-management step.

use super::Mailspace;
use super::graph_mutate::{newly_ready_after_done, ready_handles, validate_source_id};
use crate::error::VivariumError;
use crate::storage::{
    BACKLOG_GRAPH_CODE, BacklogMintInput, BacklogNodeInput, Storage, WorkGraphEdgeInput,
};

/// A resolved `--depends-on` reference to an existing need or want.
struct BacklogDep {
    handle: String,
    subject: String,
    state: &'static str,
    kind: &'static str,
}

impl Mailspace {
    /// Validate `--depends-on` handles before a send so a typo fails before
    /// the message exists.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] naming the first dependency that does not
    /// resolve to an open or done need/want.
    pub fn backlog_validate_deps(&self, depends_on: &[String]) -> Result<(), VivariumError> {
        let storage = self.storage()?;
        for dep in depends_on {
            resolve_backlog_dep(&storage, dep)?;
        }
        Ok(())
    }

    /// Mint one backlog node for `handle` (label = subject) plus one edge per
    /// dependency, minting missing dependency nodes retroactively. Idempotent.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on invalid handles, unresolvable
    /// dependencies, or storage failure.
    pub fn backlog_attach(
        &self,
        kind: &str,
        handle: &str,
        subject: &str,
        depends_on: &[String],
    ) -> Result<(), VivariumError> {
        validate_source_id(handle)?;
        let mut storage = self.storage()?;
        let mut nodes = vec![BacklogNodeInput {
            source_id: handle.to_string(),
            label: subject.to_string(),
            state: "open".into(),
            kind: kind.to_string(),
        }];
        let mut edges = Vec::new();
        for dep in depends_on {
            let resolved = resolve_backlog_dep(&storage, dep)?;
            edges.push(WorkGraphEdgeInput {
                from_source_id: resolved.handle.clone(),
                to_source_id: handle.to_string(),
                label: None,
            });
            nodes.push(BacklogNodeInput {
                source_id: resolved.handle,
                label: resolved.subject,
                state: resolved.state.into(),
                kind: resolved.kind.into(),
            });
        }
        storage.mint_backlog(&BacklogMintInput { nodes, edges })?;
        Ok(())
    }

    /// Complete the backlog node for `handle`, unlocking dependents. No-op
    /// when the backlog graph or the node does not exist (pre-feature items)
    /// or the node is not completable.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on storage failure.
    pub fn backlog_complete_item(&self, handle: &str) -> Result<(), VivariumError> {
        let mut storage = self.storage()?;
        let Some(graph) = storage.work_graph_by_code(BACKLOG_GRAPH_CODE)? else {
            return Ok(());
        };
        let nodes = storage.work_graph_nodes(&graph.handle)?;
        let edges = storage.work_graph_edges(&graph.handle)?;
        let Some(target) = nodes.iter().find(|n| n.source_id == handle) else {
            return Ok(());
        };
        if !matches!(target.state.as_str(), "open" | "active") {
            return Ok(());
        }
        let ready_before = ready_handles(&nodes, &edges);
        let newly_ready = newly_ready_after_done(&nodes, &edges, &target.handle, &ready_before);
        storage.complete_work_graph_node(&graph.handle, &target.handle, None, &newly_ready)?;
        Ok(())
    }

    /// Reopen the backlog node for `handle` after a done→open lifecycle move.
    /// No-op when the graph, the node, or a non-done state says so.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on storage failure.
    pub fn backlog_reopen_item(&self, handle: &str) -> Result<(), VivariumError> {
        let mut storage = self.storage()?;
        let Some(graph) = storage.work_graph_by_code(BACKLOG_GRAPH_CODE)? else {
            return Ok(());
        };
        let nodes = storage.work_graph_nodes(&graph.handle)?;
        let Some(target) = nodes.iter().find(|n| n.source_id == handle) else {
            return Ok(());
        };
        if target.state != "done" {
            return Ok(());
        }
        storage.set_work_graph_node_state(&graph.handle, &target.handle, "open", None)?;
        Ok(())
    }

    /// Keep the backlog node in step with a lifecycle folder move. Only
    /// need/want closes and their reversals touch graph state; promotion
    /// (wants→needs) intentionally leaves the node open.
    pub(super) fn sync_backlog_node(
        &self,
        from_role: &str,
        to_role: &str,
        handle: &str,
    ) -> Result<(), VivariumError> {
        match (from_role, to_role) {
            ("needs" | "wants", "done") => self.backlog_complete_item(handle),
            ("done", "needs" | "wants") => self.backlog_reopen_item(handle),
            _ => Ok(()),
        }
    }
}

/// Resolve one dependency token to a canonical backlog dep. Accepts needs,
/// wants, and done items; refuses tasks until graph unification lands.
fn resolve_backlog_dep(storage: &Storage, token: &str) -> Result<BacklogDep, VivariumError> {
    let message_id = storage
        .resolve_message_token(token)
        .map_err(|e| VivariumError::Message(format!("dependency '{token}': {e}")))?;
    let view = storage
        .message_by_id(&message_id)?
        .ok_or_else(|| VivariumError::Message(format!("dependency not found: {token}")))?;
    let (kind, state) = match view.local_role.as_str() {
        "needs" => ("need", "open"),
        "wants" => ("want", "open"),
        "done" => ("item", "done"),
        "tasks" => {
            return Err(VivariumError::Message(format!(
                "dependency '{token}' is a task; task dependencies join the graph with unification (Phase 2)"
            )));
        }
        other => {
            return Err(VivariumError::Message(format!(
                "dependency '{token}' has role '{other}'; expected a need or want handle"
            )));
        }
    };
    let handle = storage.display_handle(&message_id)?;
    validate_source_id(&handle)
        .map_err(|e| VivariumError::Message(format!("dependency '{token}': {e}")))?;
    Ok(BacklogDep {
        handle,
        subject: view.subject,
        state,
        kind,
    })
}

#[cfg(test)]
#[path = "backlog_test.rs"]
mod tests;
