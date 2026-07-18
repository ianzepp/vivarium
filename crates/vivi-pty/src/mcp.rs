use crate::client;
use crate::protocol::{DaemonCapabilities, PROTOCOL_VERSION, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const BRIDGE_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub daemon_method: String,
    pub read_only: bool,
    pub requires_lease: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpError {
    UnknownTool(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool(name) => write!(formatter, "unknown MCP tool: {name}"),
        }
    }
}

impl std::error::Error for McpError {}

#[derive(Clone, Debug)]
pub struct McpBridge {
    tools: Vec<ToolDescriptor>,
}

impl McpBridge {
    pub fn builtin() -> Self {
        Self {
            tools: vec![
                tool("vivi_pty.session_list", "session.list", true, false),
                tool("vivi_pty.session_start", "session.start", false, false),
                tool("vivi_pty.session_inspect", "session.inspect", true, false),
                tool("vivi_pty.session_stop", "session.stop", false, false),
                tool("vivi_pty.session_remove", "session.remove", false, false),
                tool(
                    "vivi_pty.session_diagnostic",
                    "session.diagnostic",
                    true,
                    false,
                ),
                tool("vivi_pty.session_attach", "session.attach", true, false),
                tool(
                    "vivi_pty.session_lease_acquire",
                    "session.lease.acquire",
                    false,
                    false,
                ),
                tool(
                    "vivi_pty.session_lease_release",
                    "session.lease.release",
                    false,
                    true,
                ),
                tool(
                    "vivi_pty.terminal_snapshot",
                    "terminal.snapshot",
                    true,
                    false,
                ),
                tool(
                    "vivi_pty.terminal_control_write",
                    "terminal.control_write",
                    false,
                    true,
                ),
                tool(
                    "vivi_pty.terminal_control_write_bytes",
                    "terminal.control_write_bytes",
                    false,
                    true,
                ),
                tool(
                    "vivi_pty.terminal_control_key",
                    "terminal.control_key",
                    false,
                    true,
                ),
                tool(
                    "vivi_pty.terminal_control_resize",
                    "terminal.control_resize",
                    false,
                    true,
                ),
                tool("vivi_pty.session_submit", "session.submit", false, false),
                tool(
                    "vivi_pty.session_interrupt",
                    "session.interrupt",
                    false,
                    false,
                ),
                tool("vivi_pty.session_restart", "session.restart", false, false),
            ],
        }
    }

    pub fn tools(&self) -> &[ToolDescriptor] {
        &self.tools
    }

    pub fn capabilities(&self) -> DaemonCapabilities {
        DaemonCapabilities {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            drivers: vec![
                "generic".into(),
                "grok".into(),
                "kimi".into(),
                "codex".into(),
                "pi".into(),
                "opencode".into(),
            ],
            methods: self
                .tools
                .iter()
                .map(|tool| tool.daemon_method.clone())
                .chain(["daemon.info".into(), "daemon.capabilities".into()])
                .collect(),
            features: vec![
                "read_only_attachment".into(),
                "control_leases".into(),
                "operation_replay".into(),
                "ordered_events".into(),
                "semantic_actions".into(),
            ],
        }
    }

    pub fn request(
        &self,
        tool_name: &str,
        params: Value,
        operation_id: Option<&str>,
    ) -> Result<Request, McpError> {
        let descriptor = self
            .tools
            .iter()
            .find(|tool| tool.name == tool_name)
            .ok_or_else(|| McpError::UnknownTool(tool_name.into()))?;
        let mut request = Request::new(1, descriptor.daemon_method.clone(), params);
        request.operation_id = operation_id.map(str::to_owned);
        Ok(request)
    }

    pub fn call(
        &self,
        socket: &Path,
        tool_name: &str,
        params: Value,
        operation_id: Option<&str>,
    ) -> anyhow::Result<Value> {
        let request = self.request(tool_name, params, operation_id)?;
        client::call_request(socket, request)
    }
}

fn tool(name: &str, daemon_method: &str, read_only: bool, requires_lease: bool) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        daemon_method: daemon_method.into(),
        read_only,
        requires_lease,
    }
}

impl Default for McpBridge {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
#[path = "mcp_test.rs"]
mod tests;
