use emergence::config::{ConfigManager, Settings};
use emergence::utils::env;
use std::fs;
use tempfile::TempDir;

// ── Helpers ──

fn home_dir() -> TempDir {
    TempDir::new().unwrap()
}

fn project_dir() -> TempDir {
    TempDir::new().unwrap()
}

fn write_settings(dir: &TempDir, content: &str) {
    let emergence_dir = dir.path().join(".emergence");
    fs::create_dir_all(&emergence_dir).unwrap();
    fs::write(emergence_dir.join("settings.json"), content).unwrap();
}

// ── env var expansion ──

/// Verifies that `${VAR}` placeholders in settings JSON are expanded from environment variables when ConfigManager loads.
#[test]
fn test_expand_env_vars_in_settings_file() {
    std::env::set_var("EMERGENCE_INTEGRATION_TEST_MODEL", "env-model");
    let home = home_dir();
    let project = project_dir();
    write_settings(&home, r#"{"model":"${EMERGENCE_INTEGRATION_TEST_MODEL}","version":3}"#);

    let cm = ConfigManager::load(home.path().to_path_buf(), project.path().to_path_buf(), None)
        .unwrap();
    assert_eq!(cm.settings.model, "env-model");
    assert_eq!(cm.settings.version, 3);
    std::env::remove_var("EMERGENCE_INTEGRATION_TEST_MODEL");
}

/// Verifies that `env::expand_env_vars` replaces `${HOME}` with the actual home directory path.
#[test]
fn test_expand_env_vars_direct_call() {
    let result = env::expand_env_vars("hello_${HOME}_world");
    assert!(!result.contains("${HOME}"));
    assert!(result.starts_with("hello_"));
    assert!(result.ends_with("_world"));
}

// ── ConfigManager lifecycle ──

/// Verifies that ConfigManager loads with default settings when no config file is present, and that reload() succeeds.
#[test]
fn test_config_manager_key_behavior() {
    let home = home_dir();
    let project = project_dir();
    let mut cm =
        ConfigManager::load(home.path().to_path_buf(), project.path().to_path_buf(), None)
            .unwrap();

    assert_eq!(cm.settings.version, 1);
    assert_eq!(cm.settings.generation.max_tokens, 32000);
    assert!(cm.agents_md_content.is_none());
    assert!(cm.reload().is_ok());
}

// ── Settings merging ──

/// Verifies that project-level settings override user-level settings on key overlap, while non-overlapping user keys survive.
#[test]
fn test_project_settings_override_user_settings() {
    let home = home_dir();
    let project = project_dir();

    write_settings(&home, r#"{"model":"user-model","version":2}"#);
    write_settings(&project, r#"{"model":"project-model"}"#);

    let cm =
        ConfigManager::load(home.path().to_path_buf(), project.path().to_path_buf(), None)
            .unwrap();
    assert_eq!(cm.settings.model, "project-model");
    // version from user settings should survive (not overridden by project)
    assert_eq!(cm.settings.version, 2);
}

/// Verifies that when no project settings exist, ConfigManager preserves all user-level settings unchanged.
#[test]
fn test_user_settings_preserved_when_no_project_settings() {
    let home = home_dir();
    let project = project_dir();

    write_settings(&home, r#"{"model":"only-user-model","version":5}"#);

    let cm =
        ConfigManager::load(home.path().to_path_buf(), project.path().to_path_buf(), None)
            .unwrap();
    assert_eq!(cm.settings.model, "only-user-model");
    assert_eq!(cm.settings.version, 5);
}

/// Verifies that a CLI-provided model flag takes precedence over both user-level and project-level settings.
#[test]
fn test_cli_model_overrides_both() {
    let home = home_dir();
    let project = project_dir();

    write_settings(&home, r#"{"model":"user-model"}"#);
    write_settings(&project, r#"{"model":"project-model"}"#);

    let cm = ConfigManager::load(
        home.path().to_path_buf(),
        project.path().to_path_buf(),
        Some("cli-model".into()),
    )
    .unwrap();
    assert_eq!(cm.settings.model, "cli-model");
}

// ── GenerationConfig pipeline ──

/// Verifies that `generation_config()` flows from a Settings object through ConfigManager and reflects all sub-fields.
#[test]
fn test_generation_config_from_settings() {
    let home = home_dir();
    let project = project_dir();

    write_settings(
        &home,
        r#"{"generation":{"max_tokens":4096,"temperature":0.2,"top_p":0.95,"thinking":4000}}"#,
    );

    let cm =
        ConfigManager::load(home.path().to_path_buf(), project.path().to_path_buf(), None)
            .unwrap();
    let gc = cm.generation_config();

    assert_eq!(gc.max_tokens, 4096);
    assert_eq!(gc.temperature, 0.2);
    assert_eq!(gc.top_p, 0.95);
    assert_eq!(gc.thinking, Some(4000));
    assert!(gc.tools.is_none());
}

// ── Session store dir ──

/// Verifies that `session_store_dir()` expands a tilde-prefixed default to an absolute path.
#[test]
fn test_session_store_dir_expands_tilde() {
    let home = home_dir();
    let project = project_dir();
    let cm =
        ConfigManager::load(home.path().to_path_buf(), project.path().to_path_buf(), None)
            .unwrap();
    let dir = cm.session_store_dir();
    // Default is "~/.emergence/sessions", should expand to an absolute path
    assert!(!dir.to_string_lossy().starts_with('~'));
}

// ── Settings serde through public API ──

/// Verifies that a full Settings JSON roundtrips through serde without data loss, including providers, permissions, tools, and session config.
#[test]
fn test_settings_roundtrip() {
    let json = r#"{
        "version": 1,
        "model": "test-model",
        "generation": {"max_tokens":100,"temperature":0.5,"top_p":0.9,"stop_sequences":[],"thinking":null},
        "providers": {
            "openai": {"api_key":"sk-key","base_url":"https://api.example.com/v1"}
        },
        "permissions": {"auto_approve":["read"],"deny_patterns":[]},
        "tools": {"disabled":[]},
        "session": {"store_dir":"/tmp/sessions","auto_save":true,"compaction_threshold_tokens":50000}
    }"#;

    let settings: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.model, "test-model");
    assert_eq!(settings.providers.len(), 1);
    assert_eq!(settings.providers["openai"].api_key, "sk-key");
    assert_eq!(settings.providers["openai"].base_url, "https://api.example.com/v1");
    assert_eq!(settings.session.store_dir, "/tmp/sessions");
    assert_eq!(settings.session.compaction_threshold_tokens, 50000);

    // Re-serialization should produce valid JSON that roundtrips
    let re_json = serde_json::to_string_pretty(&settings).unwrap();
    let re_settings: Settings = serde_json::from_str(&re_json).unwrap();
    assert_eq!(re_settings.model, settings.model);
    assert_eq!(re_settings.providers.len(), settings.providers.len());
}

// ── Defaults with partial JSON ──

/// Verifies that a minimal Settings JSON deserializes with all unspecified fields taking documented defaults.
#[test]
fn test_settings_partial_json_fills_defaults() {
    let json = r#"{"model":"partial"}"#;

    let settings: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.model, "partial");
    assert_eq!(settings.generation.max_tokens, 32000); // default
    assert_eq!(settings.generation.temperature, 0.7); // default
    assert!(settings.providers.is_empty());
    assert!(settings.permissions.auto_approve.contains(&"read".to_string()));
    assert!(settings.session.auto_save);
}
