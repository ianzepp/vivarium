use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use serde::Serialize;

use super::{Mailspace, kind::effective_kind};
use crate::error::VivariumError;
use crate::storage::{MailspaceEvent, MailspaceLink, Storage, StoredMessageView};

/// Print a trace graph for the given handle.
///
/// # Errors
/// Returns an error if the handle cannot be resolved, the graph cannot be
/// assembled, or JSON serialization fails when `--json` is used.
pub fn print_trace(
    mailspace: &Mailspace,
    handle: &str,
    max_depth: usize,
    limit: usize,
    json: bool,
) -> Result<(), VivariumError> {
    let graph = mailspace.trace(handle, max_depth, limit)?;
    if json {
        print_trace_json(&graph)?;
    } else {
        print_trace_text(&graph);
    }
    Ok(())
}

fn print_trace_text(graph: &TraceGraph) {
    let seed = graph.nodes.first();
    let seed_handle = seed.map_or(graph.seed.as_str(), |node| node.handle.as_str());
    println!("trace {} ({} node(s))", seed_handle, graph.nodes.len());

    let handle_by_content: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|node| (node.content_id.as_str(), node.handle.as_str()))
        .collect();

    for node in &graph.nodes {
        let kind = kind_for_node(node);
        println!("\n## {} - {} [{}]", node.date, node.handle, kind);
        println!("subject: {}", node.subject);
        if node.messages.len() > 1 {
            println!("copies:");
            for message in &node.messages {
                println!(
                    "  {} {} {}",
                    message.account, message.role, message.message_id
                );
            }
        }
        if !node.edges.is_empty() {
            println!("edges:");
            for edge in &node.edges {
                let target_handle = handle_by_content
                    .get(edge.target.as_str())
                    .copied()
                    .unwrap_or(edge.target.as_str());
                println!(
                    "  {} -> {} ({})",
                    edge.direction, target_handle, edge.source
                );
            }
        }
    }
}

fn kind_for_node(node: &TraceNode) -> String {
    node.messages
        .iter()
        .find_map(|message| message.kind.as_deref())
        .unwrap_or("mail")
        .to_string()
}

/// Print a trace graph as JSON.
///
/// # Errors
/// Returns an error if JSON serialization fails.
pub fn print_trace_json(graph: &TraceGraph) -> Result<(), VivariumError> {
    println!(
        "{}",
        serde_json::to_string_pretty(graph)
            .map_err(|e| VivariumError::Other(format!("failed to encode trace JSON: {e}")))?
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceMessageRef {
    pub message_id: String,
    pub handle: String,
    pub account: String,
    pub role: String,
    pub kind: Option<String>,
    pub date: String,
    pub from: String,
    pub to: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceEdge {
    pub target: String,
    pub source: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceNode {
    pub content_id: String,
    pub handle: String,
    pub messages: Vec<TraceMessageRef>,
    pub date: String,
    pub subject: String,
    pub body: String,
    pub edges: Vec<TraceEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceGraph {
    pub seed: String,
    pub nodes: Vec<TraceNode>,
}

#[derive(Debug, Clone, Default)]
struct TraceGraphState {
    nodes: HashMap<String, TraceNodeState>,
    adjacency: HashMap<String, Vec<(String, String, String)>>,
}

#[derive(Debug, Clone)]
struct TraceNodeState {
    content_id: String,
    handle: String,
    messages: Vec<TraceMessageRef>,
    date: String,
    subject: String,
    body: String,
}

impl Mailspace {
    /// Build a trace graph rooted at the given handle.
    ///
    /// The graph walks captured reply links, inferred body-citation links, and
    /// lifecycle-event links (e.g. `tasked`) up to `max_depth` and `limit`.
    /// Multiple copies of the same logical message (same `content_id`) are
    /// collapsed into a single node.
    ///
    /// # Errors
    /// Returns an error if the handle cannot be resolved, storage queries fail,
    /// or the graph cannot be assembled.
    pub fn trace(
        &self,
        handle: &str,
        max_depth: usize,
        limit: usize,
    ) -> Result<TraceGraph, VivariumError> {
        let storage = self.storage()?;
        let seed_id = storage.resolve_message_token(handle)?;
        let seed = storage
            .message_by_id(&seed_id)?
            .ok_or_else(|| VivariumError::Message(format!("message not found: {handle}")))?;
        let seed_content_id = seed.content_id.clone();

        let all_messages = storage.list_messages()?;
        let by_content = group_by_content_id(all_messages);
        let events = storage.list_mailspace_events_after(0)?;
        let links = storage.list_mailspace_links()?;

        let mut state = TraceGraphState::default();
        build_nodes(&storage, &by_content, &mut state)?;
        add_link_edges(&links, &mut state);
        add_event_edges(&events, &storage, &mut state);
        add_inferred_edges(&by_content, &storage, &mut state)?;

        let included = walk_from_seed(&seed_content_id, max_depth, limit, &state);
        let nodes = assemble_graph(&seed_content_id, &included, &state);
        Ok(TraceGraph {
            seed: seed_content_id,
            nodes,
        })
    }
}

fn group_by_content_id(
    messages: Vec<StoredMessageView>,
) -> HashMap<String, Vec<StoredMessageView>> {
    let mut map: HashMap<String, Vec<StoredMessageView>> = HashMap::new();
    for view in messages {
        map.entry(view.content_id.clone()).or_default().push(view);
    }
    map
}

fn build_nodes(
    storage: &Storage,
    by_content: &HashMap<String, Vec<StoredMessageView>>,
    state: &mut TraceGraphState,
) -> Result<(), VivariumError> {
    let events_by_message = events_by_message(storage)?;
    for (content_id, messages) in by_content {
        let mut messages = messages.clone();
        messages.sort_by(|left, right| {
            left.date
                .cmp(&right.date)
                .then_with(|| left.message_id.cmp(&right.message_id))
        });
        let primary = messages
            .first()
            .cloned()
            .ok_or_else(|| VivariumError::Message("empty message group".into()))?;
        let data = storage.read_message(&primary.message_id)?;
        let events = events_by_message
            .get(&primary.message_id)
            .cloned()
            .unwrap_or_default();
        let body = text_body(&data);

        let refs: Vec<TraceMessageRef> = messages
            .iter()
            .map(|view| TraceMessageRef {
                message_id: view.message_id.clone(),
                handle: view.handle.clone(),
                account: view.account.clone(),
                role: view.local_role.clone(),
                kind: effective_kind(&view.local_role, &data, &events),
                date: view.date.clone(),
                from: view.from_addr.clone(),
                to: view.to_addr.clone(),
                subject: view.subject.clone(),
            })
            .collect();

        state.nodes.insert(
            content_id.clone(),
            TraceNodeState {
                content_id: content_id.clone(),
                handle: primary.handle.clone(),
                messages: refs,
                date: primary.date.clone(),
                subject: primary.subject.clone(),
                body,
            },
        );
    }
    Ok(())
}

fn events_by_message(
    storage: &Storage,
) -> Result<HashMap<String, Vec<MailspaceEvent>>, VivariumError> {
    let all_events = storage.list_mailspace_events_after(0)?;
    let mut map: HashMap<String, Vec<MailspaceEvent>> = HashMap::new();
    for event in all_events {
        map.entry(event.message_id.clone()).or_default().push(event);
    }
    Ok(map)
}

fn add_link_edges(links: &[MailspaceLink], state: &mut TraceGraphState) {
    for link in links {
        add_adjacency(
            state,
            &link.parent_content_id,
            &link.child_content_id,
            &link.source,
            "descendant",
        );
        add_adjacency(
            state,
            &link.child_content_id,
            &link.parent_content_id,
            &link.source,
            "ancestor",
        );
    }
}

fn add_event_edges(events: &[MailspaceEvent], storage: &Storage, state: &mut TraceGraphState) {
    for event in events {
        if event.command != "task from" || event.event_type != "tasked" {
            continue;
        }
        let Some(note) = &event.note else {
            continue;
        };
        for handle in parse_task_handles(note) {
            let task_id = storage.resolve_message_token(&handle).ok();
            let task_content_id = task_id.and_then(|id| message_content_id(storage, &id));
            let Some(task_content_id) = task_content_id else {
                continue;
            };
            add_adjacency(
                state,
                &event.content_id,
                &task_content_id,
                "event",
                "descendant",
            );
            add_adjacency(
                state,
                &task_content_id,
                &event.content_id,
                "event",
                "ancestor",
            );
        }
    }
    // `moved` events intentionally do not add cross-content edges here.
    // A move preserves the same content_id and is represented by copy
    // collapse (multiple role/account copies on one node). Moves that
    // include a note reply create a separate message linked by a captured
    // reply edge, which is already added in `add_link_edges`.
}

fn parse_task_handles(note: &str) -> Vec<String> {
    let Some(prefix) = note.strip_prefix("active_tasks=") else {
        return Vec::new();
    };
    let Some(rest) = prefix.split(';').next() else {
        return Vec::new();
    };
    rest.split(',').map(|s| s.trim().to_string()).collect()
}

fn message_content_id(storage: &Storage, message_id: &str) -> Option<String> {
    storage
        .message_by_id(message_id)
        .ok()
        .flatten()
        .map(|view| view.content_id)
}

fn add_inferred_edges(
    by_content: &HashMap<String, Vec<StoredMessageView>>,
    storage: &Storage,
    state: &mut TraceGraphState,
) -> Result<(), VivariumError> {
    let candidates = build_inference_candidates(by_content, storage)?;
    for child in &candidates {
        let Some(parent) = infer_parent(child, &candidates) else {
            continue;
        };
        add_adjacency(
            state,
            &parent.content_id,
            &child.content_id,
            "inferred",
            "descendant",
        );
        add_adjacency(
            state,
            &child.content_id,
            &parent.content_id,
            "inferred",
            "ancestor",
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct InferenceCandidate {
    content_id: String,
    handle: String,
    date: String,
    subject: String,
    body: String,
    from_addr: String,
    to_addr: String,
    cc_addr: String,
}

fn build_inference_candidates(
    by_content: &HashMap<String, Vec<StoredMessageView>>,
    storage: &Storage,
) -> Result<Vec<InferenceCandidate>, VivariumError> {
    let mut candidates = Vec::new();
    for (content_id, messages) in by_content {
        let primary = messages
            .first()
            .ok_or_else(|| VivariumError::Message("empty message group".into()))?;
        let data = storage.read_message(&primary.message_id)?;
        candidates.push(InferenceCandidate {
            content_id: content_id.clone(),
            handle: primary.handle.clone(),
            date: primary.date.clone(),
            subject: primary.subject.clone(),
            body: text_body(&data),
            from_addr: primary.from_addr.clone(),
            to_addr: primary.to_addr.clone(),
            cc_addr: primary.cc_addr.clone(),
        });
    }
    Ok(candidates)
}

fn infer_parent<'a>(
    child: &'a InferenceCandidate,
    candidates: &'a [InferenceCandidate],
) -> Option<&'a InferenceCandidate> {
    let cited = candidates
        .iter()
        .filter(|candidate| candidate.content_id != child.content_id)
        .filter(|candidate| candidate.date <= child.date)
        .filter(|candidate| child.body.contains(&candidate.handle))
        .max_by(|left, right| left.date.cmp(&right.date));
    if cited.is_some() {
        return cited;
    }
    let subject = strip_reply_prefix(&child.subject);
    let subject_match = candidates
        .iter()
        .filter(|candidate| candidate.content_id != child.content_id)
        .filter(|candidate| strip_reply_prefix(&candidate.subject) == subject)
        .filter(|candidate| candidate.date <= child.date)
        .filter(|candidate| same_participants(candidate, child))
        .max_by(|left, right| left.date.cmp(&right.date));
    if subject_match.is_some() {
        return subject_match;
    }
    None
}

fn same_participants(left: &InferenceCandidate, right: &InferenceCandidate) -> bool {
    participants(left) == participants(right)
}

fn participants(candidate: &InferenceCandidate) -> BTreeSet<String> {
    let mut set = BTreeSet::from([candidate.from_addr.to_ascii_lowercase()]);
    set.extend(candidate.to_addr.split(", ").map(str::to_ascii_lowercase));
    set.extend(candidate.cc_addr.split(", ").map(str::to_ascii_lowercase));
    set
}

fn add_adjacency(state: &mut TraceGraphState, from: &str, to: &str, source: &str, direction: &str) {
    if from == to {
        return;
    }
    state.adjacency.entry(from.into()).or_default().push((
        to.into(),
        source.into(),
        direction.into(),
    ));
}

fn walk_from_seed(
    seed: &str,
    max_depth: usize,
    limit: usize,
    state: &TraceGraphState,
) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((seed.to_string(), 0usize));
    visited.insert(seed.to_string());

    while let Some((content_id, depth)) = queue.pop_front() {
        if depth >= max_depth || visited.len() >= limit {
            continue;
        }
        let neighbors = state
            .adjacency
            .get(&content_id)
            .cloned()
            .unwrap_or_default();
        for (target, _source, _direction) in neighbors {
            if visited.insert(target.clone()) {
                queue.push_back((target, depth + 1));
            }
        }
    }
    visited
}

fn assemble_graph(
    seed: &str,
    included: &HashSet<String>,
    state: &TraceGraphState,
) -> Vec<TraceNode> {
    let mut nodes: Vec<TraceNode> = included
        .iter()
        .filter_map(|content_id| state.nodes.get(content_id))
        .map(|node_state| {
            let edges: Vec<TraceEdge> = state
                .adjacency
                .get(&node_state.content_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|(target, _source, _direction)| included.contains(target))
                .map(|(target, source, direction)| TraceEdge {
                    target,
                    source,
                    direction,
                })
                .collect();
            TraceNode {
                content_id: node_state.content_id.clone(),
                handle: node_state.handle.clone(),
                messages: node_state.messages.clone(),
                date: node_state.date.clone(),
                subject: node_state.subject.clone(),
                body: node_state.body.clone(),
                edges,
            }
        })
        .collect();
    nodes.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    // Ensure seed is first.
    if let Some(seed_index) = nodes.iter().position(|n| n.content_id == seed)
        && seed_index > 0
    {
        nodes.swap(0, seed_index);
    }
    nodes
}

fn strip_reply_prefix(subject: &str) -> String {
    let mut subject = subject.trim();
    loop {
        let lower = subject.to_ascii_lowercase();
        let Some(after_re) = lower.strip_prefix("re") else {
            break;
        };
        let Some(colon_offset) = reply_prefix_colon(after_re) else {
            break;
        };
        subject = subject[2 + colon_offset + 1..].trim_start();
    }
    subject.to_ascii_lowercase()
}

fn reply_prefix_colon(after_re: &str) -> Option<usize> {
    if after_re.starts_with(':') {
        return Some(0);
    }
    let closing = after_re.strip_prefix('[')?.find(']')? + 2;
    after_re.get(closing..)?.starts_with(':').then_some(closing)
}

fn text_body(data: &[u8]) -> String {
    mail_parser::MessageParser::default()
        .parse(data)
        .and_then(|parsed| parsed.body_text(0).map(|body| body.to_string()))
        .unwrap_or_default()
}
