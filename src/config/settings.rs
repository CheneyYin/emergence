use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
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
fn default_version() -> u32 {
    1
}
fn default_model() -> String {
    "deepseek/deepseek-v4-pro".to_string()
}
fn default_max_tokens() -> u32 {
    32000
}
fn default_temperature() -> f64 {
    0.7
}
fn default_top_p() -> f64 {
    1.0
}
fn default_auto_approve() -> Vec<String> {
    vec!["read".into(), "grep".into(), "glob".into()]
}
fn default_store_dir() -> String {
    "~/.emergence/sessions".to_string()
}
fn default_true() -> bool {
    true
}
fn default_compaction_threshold() -> u32 {
    80000
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that Settings::default() produces correct default values for all fields.
    #[test]
    fn test_settings_default() {
        let s = Settings::default();
        assert_eq!(s.version, 1);
        assert_eq!(s.model, "deepseek/deepseek-v4-pro");
        assert_eq!(s.generation.max_tokens, 32000);
        assert_eq!(s.generation.temperature, 0.7);
        assert_eq!(s.generation.top_p, 1.0);
        assert!(s.generation.thinking.is_none());
        assert!(s.generation.stop_sequences.is_empty());
        assert!(s.providers.is_empty());
        assert_eq!(s.permissions.auto_approve, vec!["read", "grep", "glob"]);
        assert!(s.permissions.deny_patterns.is_empty());
        assert!(s.tools.disabled.is_empty());
        assert!(s.session.auto_save);
        assert_eq!(s.session.compaction_threshold_tokens, 80000);
    }

    /// Verifies that Settings can be deserialized from JSON with partial field overrides.
    #[test]
    fn test_deserialize_from_json() {
        let json = r#"{
            "version": 2,
            "model": "gpt-4",
            "generation": {
                "max_tokens": 100,
                "temperature": 0.5,
                "top_p": 0.9,
                "thinking": 8000
            }
        }"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.version, 2);
        assert_eq!(s.model, "gpt-4");
        assert_eq!(s.generation.max_tokens, 100);
        assert_eq!(s.generation.temperature, 0.5);
        assert_eq!(s.generation.top_p, 0.9);
        assert_eq!(s.generation.thinking, Some(8000));
    }
}
