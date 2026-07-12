use super::*;
use serde_json::json;
use std::path::PathBuf;

fn legacy_config() -> Value {
    json!({
        "hands": {
            "hand-1": {
                "mail_identity": "hand-1",
                "tmux_target": "fleet:hand-1.1",
                "cwd": "/tmp",
                "agent": "codex"
            }
        }
    })
}

#[test]
fn legacy_roles_resolve_to_tmux_without_mutation() {
    let config = legacy_config();
    let binding = resolve_role(&config, "hand-1", "/project").unwrap();
    assert_eq!(binding.runtime, RuntimeKind::Tmux);
    assert_eq!(binding.session_id, "hand-1");
    assert_eq!(binding.tmux_target.as_deref(), Some("fleet:hand-1.1"));
    assert_eq!(config["hands"]["hand-1"]["tmux_target"], "fleet:hand-1.1");
}

#[test]
fn vivi_pty_roles_use_canonical_identity_and_project_socket() {
    let config = json!({
        "hands": {
            "hand-1": {
                "mail_identity": "hand-1",
                "cwd": "/tmp",
                "runtime": {
                    "kind": "vivi_pty",
                    "driver": "codex",
                    "command": ["codex", "--no-sandbox"]
                }
            }
        }
    });
    let binding = resolve_role(&config, "hand-1", "/project").unwrap();
    assert_eq!(binding.runtime, RuntimeKind::ViviPty);
    assert_eq!(binding.session_id, "hand-1");
    assert_eq!(binding.driver.as_deref(), Some("codex"));
    assert_eq!(
        binding.socket,
        Some(PathBuf::from("/project/.vivi/vivi-pty.sock"))
    );
    assert_eq!(binding.command, ["codex", "--no-sandbox"]);
    assert!(binding.tmux_target.is_none());
}

#[test]
fn dual_runtime_ownership_and_malformed_roles_are_rejected() {
    let dual = json!({
        "hands": { "hand-1": {
            "tmux_target": "fleet:hand-1.1",
            "runtime": { "kind": "vivi_pty", "command": ["codex"] }
        }}
    });
    assert!(matches!(
        resolve_role(&dual, "hand-1", "/project"),
        Err(BindingError::Invalid(message)) if message.contains("tmux_target")
    ));

    let missing_target = json!({ "hands": { "hand-1": {} } });
    assert!(matches!(
        resolve_role(&missing_target, "hand-1", "/project"),
        Err(BindingError::Invalid(message)) if message.contains("tmux_target")
    ));

    let invalid_role = json!({ "hands": { "bad/role": {
        "tmux_target": "fleet:bad.1"
    }}});
    assert!(matches!(
        resolve_role(&invalid_role, "bad/role", "/project"),
        Err(BindingError::Invalid(_))
    ));
}

#[test]
fn missing_and_unknown_runtime_entries_are_typed() {
    let config = json!({ "hands": {} });
    assert_eq!(
        resolve_role(&config, "hand-9", "/project"),
        Err(BindingError::MissingRole("hand-9".into()))
    );

    let unsupported = json!({
        "hands": { "hand-1": {
            "runtime": { "kind": "screen", "command": ["codex"] }
        }}
    });
    assert!(matches!(
        resolve_role(&unsupported, "hand-1", "/project"),
        Err(BindingError::Invalid(message)) if message.contains("unsupported runtime")
    ));
}
