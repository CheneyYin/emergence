use emergence::hooks::*;
use emergence::permissions::RiskLevel;
use std::fs;
use tempfile::TempDir;

/// Verifies that HookRegistry::load() returns empty registry when file doesn't exist.
#[test]
fn test_load_missing_file_returns_empty() {
    let registry = HookRegistry::load(std::path::Path::new("/nonexistent/hooks.json")).unwrap();
    // dispatch with no listeners returns empty
    let rt = tokio::runtime::Runtime::new().unwrap();
    let outcomes = rt.block_on(registry.dispatch(&HookEvent::SessionStart));
    assert!(outcomes.is_empty());
}

/// Verifies that HookRegistry::load from JSON with a Shell hook creates a working executor.
#[tokio::test]
async fn test_load_from_json_with_shell_hook() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("hooks.json");
    fs::write(&path, r#"{"hooks": [{"event": "SessionStart", "type": "shell", "command": "echo ok", "timeout_ms": 5000}]}"#).unwrap();

    let registry = HookRegistry::load(&path).unwrap();
    let outcomes = registry.dispatch(&HookEvent::SessionStart).await;
    assert_eq!(outcomes.len(), 1);
}

/// Verifies that HookRegistry::load from JSON with a builtin log listener returns a working executor.
#[tokio::test]
async fn test_load_builtin_log_listener() {
    let dir = TempDir::new().unwrap();
    let log_path = dir.path().join("test.log");
    let hooks_path = dir.path().join("hooks.json");
    fs::write(&hooks_path, format!(r#"{{"hooks": [{{"event": "SessionEnd", "type": "builtin", "listener": "log", "config": {{"path": "{}"}}}}]}}"#, log_path.display())).unwrap();

    let registry = HookRegistry::load(&hooks_path).unwrap();
    let outcomes = registry.dispatch(&HookEvent::SessionEnd).await;
    assert_eq!(outcomes.len(), 1);
    // Log file should exist after dispatch
    assert!(log_path.exists());
}

/// Verifies that HookEvent serializes all variants without panicking.
#[test]
fn test_all_event_variants_serialize() {
    let events = vec![
        HookEvent::SessionStart,
        HookEvent::SessionEnd,
        HookEvent::PreToolExecute { tool: "read".into(), params: serde_json::json!({"file_path": "/x"}) },
        HookEvent::PostToolExecute { tool: "read".into(), result: emergence::tools::ToolOutput { content: "ok".into(), metadata: None } },
        HookEvent::UserInput { text: "hello".into() },
        HookEvent::PreLLMCall { messages: vec![] },
        HookEvent::PostLLMCall { response: "ok".into(), usage: Default::default() },
        HookEvent::PermissionRequested { tool: "bash".into(), risk: RiskLevel::System },
    ];
    for event in &events {
        let json = serde_json::to_string(event).unwrap();
        assert!(!json.is_empty());
    }
}

/// Verifies that HookRegistry::merge combines listeners.
#[tokio::test]
async fn test_merge_dispatches_to_both() {
    let mut reg1;
    let reg2;

    // Load shell hooks into both
    let dir = TempDir::new().unwrap();
    let p1 = dir.path().join("h1.json");
    let p2 = dir.path().join("h2.json");
    fs::write(&p1, r#"{"hooks": [{"event": "SessionStart", "type": "shell", "command": "echo 1"}]}"#).unwrap();
    fs::write(&p2, r#"{"hooks": [{"event": "SessionStart", "type": "shell", "command": "echo 2"}]}"#).unwrap();

    reg1 = HookRegistry::load(&p1).unwrap();
    reg2 = HookRegistry::load(&p2).unwrap();
    reg1.merge(reg2);

    let outcomes = reg1.dispatch(&HookEvent::SessionStart).await;
    assert_eq!(outcomes.len(), 2);
}
