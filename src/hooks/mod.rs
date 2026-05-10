use crate::permissions::RiskLevel;
use async_trait::async_trait;
use std::collections::HashMap;

pub mod builtin;
pub mod shell;

/// Hook 事件类型（无 payload，用于注册/查找 — HashMap key）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookEventType {
    SessionStart,
    SessionEnd,
    PreToolExecute,
    PostToolExecute,
    UserInput,
    PreLLMCall,
    PostLLMCall,
    PermissionRequested,
}

/// Hook 事件（含完整 payload，对齐设计 §10）
#[derive(Debug, Clone, serde::Serialize)]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    PreToolExecute {
        tool: String,
        params: serde_json::Value,
    },
    PostToolExecute {
        tool: String,
        result: crate::tools::ToolOutput,
    },
    UserInput {
        text: String,
    },
    PreLLMCall {
        messages: Vec<crate::llm::ChatMessage>,
    },
    PostLLMCall {
        response: String,
        usage: crate::llm::Usage,
    },
    PermissionRequested {
        tool: String,
        risk: RiskLevel,
    },
}

impl HookEvent {
    /// 提取无数据的 event_type，用于 HashMap 查找
    pub fn event_type(&self) -> HookEventType {
        match self {
            HookEvent::SessionStart => HookEventType::SessionStart,
            HookEvent::SessionEnd => HookEventType::SessionEnd,
            HookEvent::PreToolExecute { .. } => HookEventType::PreToolExecute,
            HookEvent::PostToolExecute { .. } => HookEventType::PostToolExecute,
            HookEvent::UserInput { .. } => HookEventType::UserInput,
            HookEvent::PreLLMCall { .. } => HookEventType::PreLLMCall,
            HookEvent::PostLLMCall { .. } => HookEventType::PostLLMCall,
            HookEvent::PermissionRequested { .. } => HookEventType::PermissionRequested,
        }
    }
}

/// Hook 执行结果
#[derive(Debug, Clone)]
pub enum HookOutcome {
    Continue,
    Abort { reason: String },
}

/// Hook 配置
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookConfig {
    Shell {
        command: String,
        #[serde(default = "default_timeout")]
        timeout_ms: u64,
        #[serde(default)]
        abort_on_error: bool,
    },
    Builtin {
        listener: String,
        config: serde_json::Value,
    },
}

fn default_timeout() -> u64 {
    30000
}

/// Hook 条目
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HookEntry {
    pub event: String,
    #[serde(flatten)]
    pub config: HookConfig,
}

/// Hook 执行器 trait
#[async_trait]
pub trait HookExecutor: Send + Sync {
    fn hook_type(&self) -> &str;
    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome>;
}

/// Hook 注册表
pub struct HookRegistry {
    listeners: HashMap<HookEventType, Vec<Box<dyn HookExecutor>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
        }
    }

    /// 从 hooks.json 加载注册
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let mut registry = Self::new();
        if !path.exists() {
            return Ok(registry);
        }

        let content = std::fs::read_to_string(path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(hooks) = parsed["hooks"].as_array() {
            for hook in hooks {
                let entry: HookEntry = serde_json::from_value(hook.clone())?;
                let event_type = parse_event_type(&entry.event);
                let executor: Box<dyn HookExecutor> = match &entry.config {
                    HookConfig::Shell {
                        command,
                        timeout_ms,
                        abort_on_error,
                    } => Box::new(shell::ShellExecutor::new(
                        command.clone(),
                        *timeout_ms,
                        *abort_on_error,
                    )),
                    HookConfig::Builtin { listener, config } => {
                        builtin::create_builtin(listener, config.clone())?
                    }
                };

                if let Some(et) = event_type {
                    registry.listeners.entry(et).or_default().push(executor);
                }
            }
        }

        Ok(registry)
    }

    /// 注册 hook（通过 event_type）
    pub fn register(&mut self, event_type: HookEventType, executor: Box<dyn HookExecutor>) {
        self.listeners.entry(event_type).or_default().push(executor);
    }

    /// 合并另一个 HookRegistry 的内容（用于两级配置合并）
    pub fn merge(&mut self, other: HookRegistry) {
        for (event_type, executors) in other.listeners {
            self.listeners
                .entry(event_type)
                .or_default()
                .extend(executors);
        }
    }

    /// 分发事件，收集所有结果（对齐设计 §10 执行流程）
    pub async fn dispatch(&self, event: &HookEvent) -> Vec<HookOutcome> {
        let mut outcomes = Vec::new();
        if let Some(executors) = self.listeners.get(&event.event_type()) {
            for executor in executors {
                match executor.execute(event).await {
                    Ok(outcome) => outcomes.push(outcome),
                    Err(e) => {
                        tracing::warn!("Hook 执行错误 ({}): {}", executor.hook_type(), e);
                        outcomes.push(HookOutcome::Continue);
                    }
                }
            }
        }
        outcomes
    }
}

fn parse_event_type(event_name: &str) -> Option<HookEventType> {
    match event_name {
        "SessionStart" => Some(HookEventType::SessionStart),
        "SessionEnd" => Some(HookEventType::SessionEnd),
        "PreToolExecute" => Some(HookEventType::PreToolExecute),
        "PostToolExecute" => Some(HookEventType::PostToolExecute),
        "UserInput" => Some(HookEventType::UserInput),
        "PreLLMCall" => Some(HookEventType::PreLLMCall),
        "PostLLMCall" => Some(HookEventType::PostLLMCall),
        "PermissionRequested" => Some(HookEventType::PermissionRequested),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that parse_event_type recognizes valid event names and returns None for unknown.
    #[test]
    fn test_parse_event_type() {
        assert!(parse_event_type("SessionStart").is_some());
        assert!(parse_event_type("PreToolExecute").is_some());
        assert!(parse_event_type("UnknownEvent").is_none());
    }

    /// Verifies that event_type() returns the correct HookEventType from a HookEvent variant.
    #[test]
    fn test_event_type_from_event() {
        let event = HookEvent::PreToolExecute {
            tool: "bash".into(),
            params: serde_json::json!({"command": "ls"}),
        };
        assert_eq!(event.event_type(), HookEventType::PreToolExecute);
    }

    /// Verifies that all 8 event types are recognized by parse_event_type.
    #[test]
    fn test_parse_all_event_types() {
        let valid = [
            "SessionStart",
            "SessionEnd",
            "PreToolExecute",
            "PostToolExecute",
            "UserInput",
            "PreLLMCall",
            "PostLLMCall",
            "PermissionRequested",
        ];
        for name in valid {
            assert!(parse_event_type(name).is_some(), "should parse {}", name);
        }
    }

    /// Verifies that dispatch routes events to registered executors and returns outcomes.
    #[tokio::test]
    async fn test_dispatch_with_registered_executor() {
        struct StubExecutor;
        #[async_trait]
        impl HookExecutor for StubExecutor {
            fn hook_type(&self) -> &str {
                "stub"
            }
            async fn execute(&self, _: &HookEvent) -> anyhow::Result<HookOutcome> {
                Ok(HookOutcome::Continue)
            }
        }

        let mut registry = HookRegistry::new();
        registry.register(HookEventType::SessionStart, Box::new(StubExecutor));
        let event = HookEvent::SessionStart;
        let outcomes = registry.dispatch(&event).await;
        assert_eq!(outcomes.len(), 1);
    }

    /// Verifies that merge combines executors from another registry for the same event type.
    #[test]
    fn test_merge_registries() {
        struct StubA;
        #[async_trait]
        impl HookExecutor for StubA {
            fn hook_type(&self) -> &str {
                "a"
            }
            async fn execute(&self, _: &HookEvent) -> anyhow::Result<HookOutcome> {
                Ok(HookOutcome::Continue)
            }
        }

        let mut reg1 = HookRegistry::new();
        reg1.register(HookEventType::SessionStart, Box::new(StubA));
        let reg2 = HookRegistry::new(); // empty
        reg1.merge(reg2);
        assert_eq!(
            reg1.listeners
                .get(&HookEventType::SessionStart)
                .unwrap()
                .len(),
            1
        );
    }

    /// Verifies that all 8 event type variants produce correct event_type() mappings.
    #[test]
    fn test_all_event_type_variants() {
        let pairs = vec![
            (HookEvent::SessionStart, HookEventType::SessionStart),
            (HookEvent::SessionEnd, HookEventType::SessionEnd),
            (
                HookEvent::PreToolExecute {
                    tool: "".into(),
                    params: serde_json::json!({}),
                },
                HookEventType::PreToolExecute,
            ),
            (
                HookEvent::PostToolExecute {
                    tool: "".into(),
                    result: crate::tools::ToolOutput {
                        content: "".into(),
                        metadata: None,
                    },
                },
                HookEventType::PostToolExecute,
            ),
            (
                HookEvent::UserInput { text: "".into() },
                HookEventType::UserInput,
            ),
            (
                HookEvent::PreLLMCall { messages: vec![] },
                HookEventType::PreLLMCall,
            ),
            (
                HookEvent::PostLLMCall {
                    response: "".into(),
                    usage: Default::default(),
                },
                HookEventType::PostLLMCall,
            ),
            (
                HookEvent::PermissionRequested {
                    tool: "".into(),
                    risk: crate::permissions::RiskLevel::ReadOnly,
                },
                HookEventType::PermissionRequested,
            ),
        ];
        for (event, expected) in pairs {
            assert_eq!(event.event_type(), expected);
        }
    }
}
