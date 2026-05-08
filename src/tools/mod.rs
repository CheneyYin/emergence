use std::collections::HashMap;
use crate::permissions::RiskLevel;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub metadata: Option<serde_json::Value>,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    fn risk_level(&self, params: &serde_json::Value) -> RiskLevel;
    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn definitions(&self) -> Vec<crate::llm::ToolDefinition> {
        self.tools.values().map(|t| crate::llm::ToolDefinition {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters: t.parameters(),
        }).collect()
    }

    pub fn risk_level(&self, name: &str, params: &serde_json::Value) -> Option<RiskLevel> {
        self.tools.get(name).map(|t| t.risk_level(params))
    }

    pub async fn execute(&self, name: &str, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let tool = self.tools.get(name)
            .ok_or_else(|| anyhow::anyhow!("未知工具: {}", name))?;
        tool.execute(params).await
    }

    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::RiskLevel;

    struct TestTool;

    #[async_trait::async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str { "test" }
        fn description(&self) -> &str { "测试工具" }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::ReadOnly }
        async fn execute(&self, _params: serde_json::Value) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput { content: "ok".into(), metadata: None })
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        assert!(registry.get("test").is_some());
        assert!(registry.get("unknown").is_none());
        assert_eq!(registry.definitions().len(), 1);
    }

    #[test]
    fn test_risk_level() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        assert_eq!(registry.risk_level("test", &serde_json::json!({})), Some(RiskLevel::ReadOnly));
    }

    #[tokio::test]
    async fn test_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        let output = registry.execute("test", serde_json::json!({})).await.unwrap();
        assert_eq!(output.content, "ok");
    }
}

pub mod bash;
pub mod file;
pub mod search;

