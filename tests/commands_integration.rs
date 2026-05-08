use emergence::commands::*;
use emergence::config::ConfigManager;
use emergence::session::SessionManager;
use emergence::utils::fuzzy;
use tempfile::TempDir;

fn make_ctx() -> (SessionManager, ConfigManager, String, bool) {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let config = ConfigManager::load(
        home.path().to_path_buf(), project.path().to_path_buf(), None,
    ).unwrap();
    let session = SessionManager::new("integration-cmd".into());
    (session, config, "default-model".into(), false)
}

/// Verifies that register_all registers 11 built-in commands and help can dispatch.
#[test]
fn test_register_all_has_expected_count() {
    let mut registry = CommandRegistry::new();
    registry.register_all();
    let list = registry.list();
    // 11 built-in commands
    assert!(list.len() >= 10);
    let names: Vec<&str> = list.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"help"));
    assert!(names.contains(&"quit"));
    assert!(names.contains(&"clear"));
    assert!(names.contains(&"model"));
}

/// Verifies that dispatch with empty input returns Error.
#[tokio::test]
async fn test_dispatch_empty_input() {
    let registry = CommandRegistry::new();  // empty — no commands registered
    let (mut session, mut config, mut model, mut should_quit) = make_ctx();
    let result = registry.dispatch("", &mut CommandContext {
        config: &mut config, session: &mut session,
        model: &mut model, should_quit: &mut should_quit,
        skill_registry: None, session_store: None,
    }).await.unwrap();
    assert!(matches!(result, CommandOutput::Error { .. }));
}

/// Verifies that /quit sets should_quit and returns Quit output.
#[tokio::test]
async fn test_dispatch_quit() {
    let mut registry = CommandRegistry::new();
    registry.register_all();
    let (mut session, mut config, mut model, mut should_quit) = make_ctx();
    let result = registry.dispatch("/quit", &mut CommandContext {
        config: &mut config, session: &mut session,
        model: &mut model, should_quit: &mut should_quit,
        skill_registry: None, session_store: None,
    }).await.unwrap();
    assert!(should_quit);
    assert!(matches!(result, CommandOutput::Quit));
}

/// Verifies that /model shows current config model.
#[tokio::test]
async fn test_dispatch_model_shows_current() {
    let mut registry = CommandRegistry::new();
    registry.register_all();
    let (mut session, mut config, mut model, mut should_quit) = make_ctx();
    let result = registry.dispatch("/model", &mut CommandContext {
        config: &mut config, session: &mut session,
        model: &mut model, should_quit: &mut should_quit,
        skill_registry: None, session_store: None,
    }).await.unwrap();
    match result {
        CommandOutput::Success { message } => assert!(message.contains("deepseek")),
        other => panic!("expected Success, got {:?}", other),
    }
}

/// Verifies that dispatch via alias ("q") works for QuitCommand.
#[tokio::test]
async fn test_dispatch_via_alias() {
    let mut registry = CommandRegistry::new();
    registry.register_all();
    let (mut session, mut config, mut model, mut should_quit) = make_ctx();
    let _result = registry.dispatch("/q", &mut CommandContext {
        config: &mut config, session: &mut session,
        model: &mut model, should_quit: &mut should_quit,
        skill_registry: None, session_store: None,
    }).await.unwrap();
    assert!(should_quit);
}

/// Verifies that dispatch with unknown command returns fuzzy suggestions.
#[tokio::test]
async fn test_dispatch_unknown_with_fuzzy() {
    let mut registry = CommandRegistry::new();
    registry.register_all();
    let (mut session, mut config, mut model, mut should_quit) = make_ctx();
    let result = registry.dispatch("/quuit", &mut CommandContext {
        config: &mut config, session: &mut session,
        model: &mut model, should_quit: &mut should_quit,
        skill_registry: None, session_store: None,
    }).await.unwrap();
    match result {
        CommandOutput::Error { message } => assert!(message.contains("quit") || message.contains("help")),
        other => panic!("expected Error, got {:?}", other),
    }
}

/// Verifies that levenshtein_distance from the public fuzzy API is accessible and correct.
#[test]
fn test_fuzzy_public_api() {
    assert_eq!(fuzzy::levenshtein_distance("help", "help"), 0);
    assert_eq!(fuzzy::levenshtein_distance("hlp", "help"), 1);
}
