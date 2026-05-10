use emergence::config::ConfigManager;
use emergence::permissions::{PermissionStore, RiskLevel};
use emergence::session::store::{JsonFileStore, SessionStore};
use emergence::session::SessionKey;
use emergence::session::{Session, SessionManager};
use emergence::tools::ToolRegistry;

// ── Session persistence ──

/// Verifies that a session with messages can be saved to JsonFileStore and loaded back intact.
#[tokio::test]
async fn test_session_save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let mut sm = SessionManager::new("roundtrip-test".into());
    sm.begin_turn(emergence::llm::ChatMessage {
        role: emergence::llm::Role::User,
        content: emergence::llm::Content::Text("hello world".into()),
        name: None,
        tool_call_id: None,
    });
    sm.complete_turn().unwrap();

    store.save(sm.session()).await.unwrap();

    let loaded = store
        .load(&SessionKey::Id("roundtrip-test".into()))
        .await
        .unwrap();
    assert!(loaded.is_some());
    let session = loaded.unwrap();
    assert_eq!(session.turns.len(), 1);
    assert_eq!(session.turns[0].messages.len(), 1);
}

/// Verifies that a session alias can be set and used to look up the session.
#[tokio::test]
async fn test_session_alias_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let session = Session::new("alias-test".into());
    store.save(&session).await.unwrap();
    store.set_alias("alias-test", "my-feature").await.unwrap();

    let loaded_by_alias = store
        .load(&SessionKey::Alias("my-feature".into()))
        .await
        .unwrap();
    assert!(loaded_by_alias.is_some());
    assert_eq!(loaded_by_alias.unwrap().id, "alias-test");
}

/// Verifies that delete removes a session and it is no longer loadable.
#[tokio::test]
async fn test_delete_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let session = Session::new("delete-test".into());
    store.save(&session).await.unwrap();
    assert!(store
        .load(&SessionKey::Id("delete-test".into()))
        .await
        .unwrap()
        .is_some());

    store
        .delete(&SessionKey::Id("delete-test".into()))
        .await
        .unwrap();
    assert!(store
        .load(&SessionKey::Id("delete-test".into()))
        .await
        .unwrap()
        .is_none());
}

// ── Config loading ──

/// Verifies that settings are loaded from a JSON file via ConfigManager.
#[test]
fn test_load_settings_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let emergence_dir = dir.path().join(".emergence");
    std::fs::create_dir_all(&emergence_dir).unwrap();

    let settings = serde_json::json!({
        "version": 1,
        "model": "test/test-model",
        "generation": { "max_tokens": 8000, "temperature": 0.5, "top_p": 0.9, "stop_sequences": [], "thinking": null },
        "providers": {},
        "permissions": { "auto_approve": ["read"], "deny_patterns": [] },
        "tools": { "disabled": [] },
        "session": { "store_dir": "~/.emergence/sessions", "auto_save": true, "compaction_threshold_tokens": 80000 }
    });

    std::fs::write(
        emergence_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    let config =
        ConfigManager::load(dir.path().to_path_buf(), dir.path().to_path_buf(), None).unwrap();
    assert_eq!(config.settings.model, "test/test-model");
    assert_eq!(config.settings.generation.max_tokens, 8000);
}

/// Verifies that CLI model flag overrides settings file.
#[test]
fn test_cli_model_overrides_settings() {
    let dir = tempfile::tempdir().unwrap();
    let emergence_dir = dir.path().join(".emergence");
    std::fs::create_dir_all(&emergence_dir).unwrap();
    std::fs::write(emergence_dir.join("settings.json"), "{}").unwrap();

    let config = ConfigManager::load(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        Some("cli-override-model".into()),
    )
    .unwrap();
    assert_eq!(config.settings.model, "cli-override-model");
}

// ── Agent loop simulation ──

/// Verifies that all 8 tools can be registered and produce definitions with name/description.
#[test]
fn test_tool_registry_all_tools_registered() {
    let mut registry = ToolRegistry::new();
    registry.register(emergence::tools::file::ReadTool);
    registry.register(emergence::tools::file::WriteTool);
    registry.register(emergence::tools::file::EditTool);
    registry.register(emergence::tools::search::GrepTool);
    registry.register(emergence::tools::search::GlobTool);
    registry.register(emergence::tools::bash::BashTool);
    registry.register(emergence::tools::web::WebFetchTool);
    registry.register(emergence::tools::web::WebSearchTool);

    let definitions = registry.definitions();
    assert_eq!(definitions.len(), 8);
    for def in &definitions {
        assert!(!def.name.is_empty());
        assert!(!def.description.is_empty());
    }
}

/// Verifies the full permission workflow: default deny, approve_always, clear.
#[test]
fn test_permission_store_workflow() {
    let mut store = PermissionStore::new();
    assert!(!store.is_allowed("bash", RiskLevel::Write));
    store.approve_always("bash", RiskLevel::Write);
    assert!(store.is_allowed("bash", RiskLevel::Write));
    assert!(!store.is_allowed("bash", RiskLevel::System));
    store.clear();
    assert!(!store.is_allowed("bash", RiskLevel::Write));
}

/// Verifies that build_context produces a system message first and includes user messages.
#[test]
fn test_session_manager_context_building() {
    let mut sm = SessionManager::new("ctx-test".into());
    sm.begin_turn(emergence::llm::ChatMessage {
        role: emergence::llm::Role::User,
        content: emergence::llm::Content::Text("write a function".into()),
        name: None,
        tool_call_id: None,
    });
    sm.push(emergence::llm::ChatMessage {
        role: emergence::llm::Role::Assistant,
        content: emergence::llm::Content::Text("here is the function...".into()),
        name: None,
        tool_call_id: None,
    })
    .unwrap();
    sm.complete_turn().unwrap();

    let tools = vec![emergence::llm::ToolDefinition {
        name: "read".into(),
        description: "read files".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    }];
    let ctx = sm.build_context("You are helpful.", &tools, "", &[], None);

    let system_msg = ctx.first().unwrap();
    assert_eq!(system_msg.role, emergence::llm::Role::System);
    let user_msgs: Vec<_> = ctx
        .iter()
        .filter(|m| m.role == emergence::llm::Role::User)
        .collect();
    assert_eq!(user_msgs.len(), 1);
}

/// Verifies that BashTool correctly classifies read-only, write, and dangerous commands.
#[test]
fn test_bash_risk_classification() {
    use emergence::tools::bash::BashTool;
    use emergence::tools::Tool;

    assert_eq!(
        BashTool.risk_level(&serde_json::json!({"command": "ls -la"})),
        RiskLevel::ReadOnly
    );
    assert_eq!(
        BashTool.risk_level(&serde_json::json!({"command": "cargo build"})),
        RiskLevel::Write
    );
    assert_eq!(
        BashTool.risk_level(&serde_json::json!({"command": "sudo rm -rf /"})),
        RiskLevel::System
    );
}
