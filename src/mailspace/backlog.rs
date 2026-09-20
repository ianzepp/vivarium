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

    /// Complete the backlog node for `handle` via the lifecycle hook, then
    /// cascade the join rule. Returns whether a transition occurred.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on storage failure.
    pub fn backlog_complete_item(&self, handle: &str) -> Result<bool, VivariumError> {
        self.backlog_complete_item_via(handle, "via=lifecycle")
    }

    /// Complete the backlog node for `handle`, recording the decision
    /// (`via=<origin>`) atomically with the transition, then cascade the join
    /// rule. Returns whether a transition occurred; no-op when the backlog
    /// graph or the node does not exist (pre-feature items) or the node is
    /// not completable.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on storage failure.
    pub fn backlog_complete_item_via(
        &self,
        handle: &str,
        via: &str,
    ) -> Result<bool, VivariumError> {
        let mut storage = self.storage()?;
        let Some(graph) = storage.work_graph_by_code(BACKLOG_GRAPH_CODE)? else {
            return Ok(false);
        };
        let nodes = storage.work_graph_nodes(&graph.handle)?;
        let edges = storage.work_graph_edges(&graph.handle)?;
        let Some(target) = nodes.iter().find(|n| n.source_id == handle) else {
            return Ok(false);
        };
        if !matches!(target.state.as_str(), "open" | "active") {
            return Ok(false);
        }
        let ready_before = ready_handles(&nodes, &edges);
        let newly_ready = newly_ready_after_done(&nodes, &edges, &target.handle, &ready_before);
        storage.complete_work_graph_node(
            &graph.handle,
            &target.handle,
            None,
            &newly_ready,
            Some(via),
        )?;
        drop(storage);
        if let Some(parent) = target.subgraph.clone() {
            self.backlog_complete_if_join_complete(&parent)?;
        }
        Ok(true)
    }

    /// Bind unit tasks to a parent need: each unit node's `subgraph` becomes
    /// the need's source id. Binding already-done units may complete the need
    /// immediately via the join rule.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] naming the first missing node, or on
    /// storage failure.
    pub fn backlog_bind_units(&self, parent: &str, units: &[String]) -> Result<(), VivariumError> {
        validate_source_id(parent)?;
        if units.is_empty() {
            return Err(VivariumError::Message(
                "bind needs at least one unit handle".into(),
            ));
        }
        let mut storage = self.storage()?;
        let graph = storage
            .work_graph_by_code(BACKLOG_GRAPH_CODE)?
            .ok_or_else(|| VivariumError::Message(format!("need not found: {parent}")))?;
        let nodes = storage.work_graph_nodes(&graph.handle)?;
        let parent_node = nodes
            .iter()
            .find(|n| n.source_id == parent)
            .ok_or_else(|| VivariumError::Message(format!("need not found: {parent}")))?;
        if parent_node.state != "open" {
            return Err(VivariumError::Message(format!(
                "cannot bind units to need '{parent}' in state '{}'",
                parent_node.state
            )));
        }
        let mut unit_handles = Vec::with_capacity(units.len());
        for unit in units {
            let unit_node = nodes.iter().find(|n| n.source_id == *unit).ok_or_else(|| {
                VivariumError::Message(format!(
                    "unit '{unit}' has no graph node; send it before binding"
                ))
            })?;
            unit_handles.push(unit_node.handle.clone());
        }
        storage.bind_backlog_units(&graph.handle, parent, &unit_handles)?;
        drop(storage);
        self.backlog_complete_if_join_complete(parent)
    }

    /// Complete `parent` when every unit bound to it is done.
    ///
    /// # Errors
    /// Returns a [`VivariumError`] on storage failure.
    fn backlog_complete_if_join_complete(&self, parent: &str) -> Result<(), VivariumError> {
        let storage = self.storage()?;
        let Some(graph) = storage.work_graph_by_code(BACKLOG_GRAPH_CODE)? else {
            return Ok(());
        };
        let units = storage.work_graph_nodes_by_subgraph(&graph.handle, parent)?;
        if units.is_empty() || !units.iter().all(|n| n.state == "done") {
            return Ok(());
        }
        drop(storage);
        self.backlog_complete_item(parent).map(|_| ())
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

    /// Keep the backlog node in step with a lifecycle folder move. Work-item
    /// closes (tasks/needs/wants → done) and their reversals touch graph
    /// state; promotion (wants→needs) intentionally leaves the node open.
    pub(super) fn sync_backlog_node(
        &self,
        from_role: &str,
        to_role: &str,
        handle: &str,
    ) -> Result<(), VivariumError> {
        match (from_role, to_role) {
            ("tasks" | "needs" | "wants", "done") => self.backlog_complete_item(handle).map(|_| ()),
            ("done", "tasks" | "needs" | "wants") => self.backlog_reopen_item(handle),
            _ => Ok(()),
        }
    }
}

/// Resolve one dependency token to a canonical backlog dep. Accepts open
/// tasks, needs, and wants plus done items of any kind.
fn resolve_backlog_dep(storage: &Storage, token: &str) -> Result<BacklogDep, VivariumError> {
    let message_id = storage
        .resolve_message_token(token)
        .map_err(|e| VivariumError::Message(format!("dependency '{token}': {e}")))?;
    let view = storage
        .message_by_id(&message_id)?
        .ok_or_else(|| VivariumError::Message(format!("dependency not found: {token}")))?;
    let (kind, state) = match view.local_role.as_str() {
        "tasks" => ("task", "open"),
        "needs" => ("need", "open"),
        "wants" => ("want", "open"),
        "done" => ("item", "done"),
        other => {
            return Err(VivariumError::Message(format!(
                "dependency '{token}' has role '{other}'; expected a task, need, or want handle"
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
