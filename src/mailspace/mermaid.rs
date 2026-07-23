//! Narrow Mermaid flowchart profile for work-graph import.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::error::VivariumError;

/// Parsed flowchart under the supported work-graph profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MermaidFlowchart {
    pub direction: String,
    pub nodes: Vec<MermaidNode>,
    pub edges: Vec<MermaidEdge>,
}

/// One node declaration (explicit or inferred from an edge endpoint).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MermaidNode {
    pub source_id: String,
    pub label: String,
    pub subgraph: Option<String>,
}

/// Directed edge: `to` requires `from`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MermaidEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

/// Parse a supported Mermaid flowchart / graph document.
///
/// # Errors
/// Returns [`VivariumError::Parse`] for unsupported syntax or structural errors.
pub fn parse_flowchart(source: &str) -> Result<MermaidFlowchart, VivariumError> {
    let lines: Vec<&str> = source.lines().collect();
    let (direction, start) = find_header(&lines)?;
    let mut nodes: HashMap<String, MermaidNode> = HashMap::new();
    let mut edges: Vec<MermaidEdge> = Vec::new();
    let mut subgraph_stack: Vec<String> = Vec::new();
    let mut edge_set: HashSet<(String, String)> = HashSet::new();

    for (idx, raw) in lines.iter().enumerate().skip(start) {
        let line_no = idx + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("subgraph") {
            let (id, label) = parse_subgraph_header(rest.trim(), line_no)?;
            subgraph_stack.push(id.clone());
            let _ = label;
            continue;
        }
        if line == "end" {
            if subgraph_stack.pop().is_none() {
                return Err(parse_err(line_no, "unexpected 'end' without open subgraph"));
            }
            continue;
        }
        if line.contains("-->") {
            parse_edge_line(
                line,
                line_no,
                &subgraph_stack,
                &mut nodes,
                &mut edges,
                &mut edge_set,
            )?;
            continue;
        }
        parse_node_line(line, line_no, &subgraph_stack, &mut nodes)?;
    }

    if !subgraph_stack.is_empty() {
        return Err(parse_err(0, "unclosed subgraph block"));
    }
    if nodes.is_empty() {
        return Err(parse_err(0, "flowchart has no nodes"));
    }

    let mut node_list: Vec<MermaidNode> = nodes.into_values().collect();
    node_list.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    validate_endpoints(&node_list, &edges)?;
    detect_cycle(&node_list, &edges)?;

    Ok(MermaidFlowchart {
        direction,
        nodes: node_list,
        edges,
    })
}

fn find_header(lines: &[&str]) -> Result<(String, usize), VivariumError> {
    for (idx, raw) in lines.iter().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let kind = parts.next().unwrap_or("");
        if kind != "flowchart" && kind != "graph" {
            return Err(parse_err(
                idx + 1,
                &format!("unsupported Mermaid start '{kind}'; expected flowchart or graph"),
            ));
        }
        let direction = parts
            .next()
            .ok_or_else(|| parse_err(idx + 1, "missing flowchart direction"))?;
        if !matches!(direction, "TB" | "TD" | "BT" | "RL" | "LR") {
            return Err(parse_err(
                idx + 1,
                &format!("unsupported direction '{direction}'"),
            ));
        }
        if parts.next().is_some() {
            return Err(parse_err(idx + 1, "unexpected tokens after direction"));
        }
        return Ok((direction.to_string(), idx + 1));
    }
    Err(parse_err(0, "empty Mermaid source"))
}

fn parse_subgraph_header(
    rest: &str,
    line_no: usize,
) -> Result<(String, Option<String>), VivariumError> {
    if rest.is_empty() {
        return Err(parse_err(line_no, "subgraph requires an id"));
    }
    if let Some((id, after)) = rest.split_once('[') {
        let id = id.trim();
        validate_id(id, line_no)?;
        let label = parse_bracket_label(after, line_no)?;
        return Ok((id.to_string(), Some(label)));
    }
    let id = rest.trim();
    validate_id(id, line_no)?;
    Ok((id.to_string(), None))
}

fn parse_node_line(
    line: &str,
    line_no: usize,
    subgraph_stack: &[String],
    nodes: &mut HashMap<String, MermaidNode>,
) -> Result<(), VivariumError> {
    let (id, label) = if let Some((id, after)) = line.split_once('[') {
        let id = id.trim();
        validate_id(id, line_no)?;
        let label = parse_bracket_label(after, line_no)?;
        (id.to_string(), label)
    } else {
        let id = line.trim();
        validate_id(id, line_no)?;
        (id.to_string(), id.to_string())
    };
    ensure_node(nodes, &id, &label, current_subgraph(subgraph_stack));
    Ok(())
}

fn parse_edge_line(
    line: &str,
    line_no: usize,
    subgraph_stack: &[String],
    nodes: &mut HashMap<String, MermaidNode>,
    edges: &mut Vec<MermaidEdge>,
    edge_set: &mut HashSet<(String, String)>,
) -> Result<(), VivariumError> {
    let parts = split_edge_chain(line, line_no)?;
    if parts.len() < 2 {
        return Err(parse_err(
            line_no,
            "edge chain needs at least two endpoints",
        ));
    }
    for window in parts.windows(2) {
        let (from_raw, edge_label) = &window[0];
        let (to_raw, _) = &window[1];
        let (from_id, from_label) = parse_endpoint(from_raw, line_no)?;
        let (to_id, to_label) = parse_endpoint(to_raw, line_no)?;
        ensure_node(
            nodes,
            &from_id,
            from_label.as_deref().unwrap_or(from_id.as_str()),
            current_subgraph(subgraph_stack),
        );
        ensure_node(
            nodes,
            &to_id,
            to_label.as_deref().unwrap_or(to_id.as_str()),
            current_subgraph(subgraph_stack),
        );
        let key = (from_id.clone(), to_id.clone());
        if edge_set.insert(key) {
            edges.push(MermaidEdge {
                from: from_id,
                to: to_id,
                label: edge_label.clone(),
            });
        }
    }
    Ok(())
}

/// Split `a --> b -->|lbl| c` into endpoint tokens with the outbound edge label.
fn split_edge_chain(
    line: &str,
    line_no: usize,
) -> Result<Vec<(String, Option<String>)>, VivariumError> {
    let mut out = Vec::new();
    let mut rest = line;
    loop {
        if let Some(idx) = find_arrow(rest) {
            let left = rest[..idx].trim().to_string();
            if left.is_empty() {
                return Err(parse_err(line_no, "empty edge endpoint"));
            }
            let after_arrow = &rest[idx + 3..];
            let (edge_label, next) = parse_optional_edge_label(after_arrow, line_no)?;
            out.push((left, edge_label));
            rest = next;
        } else {
            let right = rest.trim().to_string();
            if right.is_empty() {
                return Err(parse_err(line_no, "edge chain ends without endpoint"));
            }
            out.push((right, None));
            break;
        }
    }
    Ok(out)
}

fn find_arrow(s: &str) -> Option<usize> {
    s.find("-->")
}

fn parse_optional_edge_label(
    after_arrow: &str,
    line_no: usize,
) -> Result<(Option<String>, &str), VivariumError> {
    let trimmed = after_arrow.trim_start();
    if let Some(rest) = trimmed.strip_prefix('|') {
        let end = rest
            .find('|')
            .ok_or_else(|| parse_err(line_no, "unclosed edge label"))?;
        let label = rest[..end].trim().to_string();
        Ok((Some(label), rest[end + 1..].trim_start()))
    } else {
        Ok((None, trimmed))
    }
}

fn parse_endpoint(raw: &str, line_no: usize) -> Result<(String, Option<String>), VivariumError> {
    let raw = raw.trim();
    if let Some((id, after)) = raw.split_once('[') {
        let id = id.trim();
        validate_id(id, line_no)?;
        let label = parse_bracket_label(after, line_no)?;
        return Ok((id.to_string(), Some(label)));
    }
    validate_id(raw, line_no)?;
    Ok((raw.to_string(), None))
}

fn parse_bracket_label(after_open: &str, line_no: usize) -> Result<String, VivariumError> {
    let after_open = after_open.trim();
    if let Some(inner) = after_open.strip_prefix('"') {
        let end = inner
            .find('"')
            .ok_or_else(|| parse_err(line_no, "unclosed quoted node label"))?;
        let label = inner[..end].to_string();
        let rest = inner[end + 1..].trim_start();
        if !rest.starts_with(']') {
            return Err(parse_err(line_no, "expected ] after quoted node label"));
        }
        let trailing = rest[1..].trim();
        if !trailing.is_empty() {
            return Err(parse_err(line_no, "unexpected tokens after node label"));
        }
        return Ok(label);
    }
    let end = after_open
        .find(']')
        .ok_or_else(|| parse_err(line_no, "unclosed node label"))?;
    let label = after_open[..end].trim().to_string();
    let trailing = after_open[end + 1..].trim();
    if !trailing.is_empty() {
        return Err(parse_err(line_no, "unexpected tokens after node label"));
    }
    Ok(label)
}

fn ensure_node(
    nodes: &mut HashMap<String, MermaidNode>,
    source_id: &str,
    label: &str,
    subgraph: Option<String>,
) {
    nodes
        .entry(source_id.to_string())
        .and_modify(|existing| {
            if existing.label == existing.source_id && label != source_id {
                existing.label = label.to_string();
            }
            if existing.subgraph.is_none() {
                existing.subgraph.clone_from(&subgraph);
            }
        })
        .or_insert_with(|| MermaidNode {
            source_id: source_id.to_string(),
            label: label.to_string(),
            subgraph,
        });
}

fn current_subgraph(stack: &[String]) -> Option<String> {
    stack.last().cloned()
}

fn validate_id(id: &str, line_no: usize) -> Result<(), VivariumError> {
    if id.is_empty() {
        return Err(parse_err(line_no, "empty node id"));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(parse_err(
            line_no,
            &format!("invalid node id '{id}' (use [A-Za-z0-9_-]+)"),
        ));
    }
    Ok(())
}

fn validate_endpoints(nodes: &[MermaidNode], edges: &[MermaidEdge]) -> Result<(), VivariumError> {
    let ids: HashSet<&str> = nodes.iter().map(|n| n.source_id.as_str()).collect();
    for edge in edges {
        if !ids.contains(edge.from.as_str()) {
            return Err(parse_err(
                0,
                &format!("edge references missing node '{}'", edge.from),
            ));
        }
        if !ids.contains(edge.to.as_str()) {
            return Err(parse_err(
                0,
                &format!("edge references missing node '{}'", edge.to),
            ));
        }
    }
    Ok(())
}

fn detect_cycle(nodes: &[MermaidNode], edges: &[MermaidEdge]) -> Result<(), VivariumError> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in nodes {
        adj.entry(node.source_id.as_str()).or_default();
    }
    for edge in edges {
        adj.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in nodes {
        if dfs_cycle(node.source_id.as_str(), &adj, &mut visiting, &mut visited)? {
            return Err(parse_err(0, "cycle detected in flowchart"));
        }
    }
    Ok(())
}

fn dfs_cycle<'a>(
    node: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> Result<bool, VivariumError> {
    if visited.contains(node) {
        return Ok(false);
    }
    if !visiting.insert(node) {
        return Ok(true);
    }
    if let Some(nexts) = adj.get(node) {
        for next in nexts {
            if dfs_cycle(next, adj, visiting, visited)? {
                return Ok(true);
            }
        }
    }
    visiting.remove(node);
    visited.insert(node);
    Ok(false)
}

fn strip_comment(line: &str) -> &str {
    if let Some(idx) = line.find("%%") {
        &line[..idx]
    } else {
        line
    }
}

fn parse_err(line_no: usize, msg: &str) -> VivariumError {
    if line_no == 0 {
        VivariumError::Parse(format!("mermaid: {msg}"))
    } else {
        VivariumError::Parse(format!("mermaid line {line_no}: {msg}"))
    }
}

#[cfg(test)]
#[path = "mermaid_test.rs"]
mod tests;
