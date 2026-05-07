use super::Provider;
use std::collections::HashMap;

pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, provider: Box<dyn Provider>) {
        self.providers.insert(name, provider);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Provider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    pub fn list_providers(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::*;
    use async_trait::async_trait;

    struct StubProvider {
        models: Vec<ModelInfo>,
    }

    #[async_trait]
    impl Provider for StubProvider {
        async fn chat(
            &self,
            _model: &str,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _config: &GenerationConfig,
        ) -> anyhow::Result<ChatStream> {
            unimplemented!()
        }

        fn models(&self) -> &[ModelInfo] {
            &self.models
        }
    }

    fn make_stub(name: &str) -> Box<dyn Provider> {
        Box::new(StubProvider {
            models: vec![ModelInfo {
                id: name.to_string(),
                name: name.to_string(),
                max_tokens: 4096,
            }],
        })
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = ProviderRegistry::new();
        registry.register("test".to_string(), make_stub("test-model"));

        let provider = registry.get("test");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().models()[0].id, "test-model");
    }

    #[test]
    fn test_get_nonexistent() {
        let registry = ProviderRegistry::new();
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn test_list_providers_sorted() {
        let mut registry = ProviderRegistry::new();
        registry.register("c".to_string(), make_stub("c"));
        registry.register("a".to_string(), make_stub("a"));
        registry.register("b".to_string(), make_stub("b"));

        assert_eq!(registry.list_providers(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_register_overwrites() {
        let mut registry = ProviderRegistry::new();
        registry.register("p".to_string(), make_stub("old"));
        registry.register("p".to_string(), make_stub("new"));

        let provider = registry.get("p").unwrap();
        assert_eq!(provider.models()[0].id, "new");
    }
}
