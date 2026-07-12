use super::*;
use std::path::Path;

#[test]
fn builtin_capabilities_describe_drivers_and_control_boundaries() {
    let bridge = McpBridge::default();
    let capabilities = bridge.capabilities();
    assert_eq!(capabilities.protocol_version, PROTOCOL_VERSION);
    assert!(capabilities.drivers.contains(&"codex".into()));
    assert!(capabilities.features.contains(&"control_leases".into()));
    assert!(capabilities.methods.contains(&"daemon.capabilities".into()));
}

#[test]
fn tool_translation_preserves_operation_ids_and_rejects_unknown_names() {
    let bridge = McpBridge::default();
    let request = bridge
        .request(
            "vivi_pty.terminal_control_write",
            serde_json::json!({ "session_id": "demo" }),
            Some("operation-7"),
        )
        .unwrap();
    assert_eq!(request.method, "terminal.control_write");
    assert_eq!(request.operation_id.as_deref(), Some("operation-7"));
    assert!(matches!(
        bridge.request("terminal.write", serde_json::json!({}), None),
        Err(McpError::UnknownTool(_))
    ));
}

#[test]
fn tool_metadata_marks_observation_and_lease_requirements() {
    let bridge = McpBridge::default();
    let attach = bridge
        .tools()
        .iter()
        .find(|tool| tool.name == "vivi_pty.session_attach")
        .unwrap();
    assert!(attach.read_only);
    assert!(!attach.requires_lease);

    let write = bridge
        .tools()
        .iter()
        .find(|tool| tool.name == "vivi_pty.terminal_control_write")
        .unwrap();
    assert!(!write.read_only);
    assert!(write.requires_lease);
}

#[test]
fn unknown_tool_is_rejected_before_socket_access() {
    let bridge = McpBridge::default();
    let error = bridge
        .call(
            Path::new("/definitely/missing/socket"),
            "vivi_pty.unknown",
            serde_json::json!({}),
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("unknown MCP tool"));
}
