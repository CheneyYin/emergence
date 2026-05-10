use emergence::permissions::RiskLevel;
use emergence::tools::bash::BashTool;
use emergence::tools::file::{EditTool, ReadTool, WriteTool};
use emergence::tools::search::{GlobTool, GrepTool};
use emergence::tools::web::{WebFetchTool, WebSearchTool};
use emergence::tools::*;

/// Verifies that ToolRegistry can register all real tool types and each can be retrieved by its canonical name.
#[test]
fn test_registry_with_real_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadTool);
    registry.register(WriteTool);
    registry.register(EditTool);
    registry.register(GrepTool);
    registry.register(GlobTool);
    registry.register(BashTool);
    registry.register(WebFetchTool);
    registry.register(WebSearchTool);

    assert_eq!(registry.definitions().len(), 8);
    assert!(registry.get("read").is_some());
    assert!(registry.get("write").is_some());
    assert!(registry.get("edit").is_some());
    assert!(registry.get("grep").is_some());
    assert!(registry.get("glob").is_some());
    assert!(registry.get("bash").is_some());
    assert!(registry.get("web_fetch").is_some());
    assert!(registry.get("web_search").is_some());
}

/// Verifies that ToolRegistry.definitions() returns tool definitions containing descriptions and parameters from real tool implementations.
#[test]
fn test_registry_definitions_output() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadTool);
    registry.register(BashTool);

    let defs = registry.definitions();
    let read_def = defs.iter().find(|d| d.name == "read").unwrap();
    assert!(read_def.description.contains("读取"));
    assert_eq!(read_def.parameters["type"], "object");
}

/// Verifies that ToolRegistry.risk_level() returns appropriate RiskLevel per tool and that BashTool risk depends on command content.
#[test]
fn test_registry_risk_levels() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadTool);
    registry.register(WriteTool);
    registry.register(BashTool);

    assert_eq!(
        registry.risk_level("read", &serde_json::json!({})),
        Some(RiskLevel::ReadOnly)
    );
    assert_eq!(
        registry.risk_level("write", &serde_json::json!({})),
        Some(RiskLevel::Write)
    );
    // BashTool risk depends on command — "ls" is safe
    assert_eq!(
        registry.risk_level("bash", &serde_json::json!({"command": "ls"})),
        Some(RiskLevel::ReadOnly)
    );
}

/// Verifies that ToolRegistry.execute() can run a ReadTool on an actual file and returns its content.
#[tokio::test]
async fn test_registry_execute_read_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadTool);

    let output = registry
        .execute("read", serde_json::json!({"file_path": "src/main.rs"}))
        .await
        .unwrap();
    assert!(output.content.contains("main.rs"));
    assert!(output.content.contains("行"));
}

/// Verifies that ToolRegistry.execute() returns an error for an unregistered tool name.
#[tokio::test]
async fn test_registry_execute_unknown_tool_errors() {
    let registry = ToolRegistry::new();
    let result = registry.execute("nonexistent", serde_json::json!({})).await;
    assert!(result.is_err());
}
