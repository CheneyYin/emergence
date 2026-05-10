use std::path::{Path, PathBuf};

use crate::utils::env;

pub mod agents_md;
pub mod settings;

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

        let project_path = project_dir.join(".emergence").join("settings.json");
        let project_settings =
            load_settings_file(&project_path).unwrap_or_else(|_| Settings::default());

        let mut settings = user_settings;
        // 仅当项目配置文件实际存在时才合并
        if project_path.exists() {
            merge_settings(&mut settings, &project_settings);
        }

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
        let new = Self::load(self.home_dir.clone(), self.project_dir.clone(), None)?;
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
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs_functions::home_dir() {
            return home.join(rest);
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
    // 仅在 overlay 显式设置时覆盖标量字段（判断依据：值不等于默认值）
    if overlay.model != "deepseek/deepseek-v4-pro" {
        base.model.clone_from(&overlay.model);
    }
    if overlay.version != 1 {
        base.version = overlay.version;
    }
    if overlay.generation.max_tokens != 32000 {
        base.generation.max_tokens = overlay.generation.max_tokens;
    }
    if overlay.generation.temperature != 0.7 {
        base.generation.temperature = overlay.generation.temperature;
    }
    if overlay.generation.top_p != 1.0 {
        base.generation.top_p = overlay.generation.top_p;
    }
    if !overlay.generation.stop_sequences.is_empty() {
        base.generation
            .stop_sequences
            .clone_from(&overlay.generation.stop_sequences);
    }
    if overlay.generation.thinking.is_some() {
        base.generation.thinking = overlay.generation.thinking;
    }
    for (name, cfg) in &overlay.providers {
        base.providers
            .entry(name.clone())
            .or_insert_with(|| cfg.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn home_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn project_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    /// Verifies that ConfigManager::load returns default settings when no config files exist.
    #[test]
    fn test_load_with_defaults() {
        let home = home_dir();
        let project = project_dir();
        let cm = ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        assert_eq!(cm.settings.model, "deepseek/deepseek-v4-pro");
        assert_eq!(cm.settings.generation.max_tokens, 32000);
    }

    /// Verifies that the CLI model argument overrides the default model setting.
    #[test]
    fn test_cli_model_overrides() {
        let home = home_dir();
        let project = project_dir();
        let cm = ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            Some("gpt-4".into()),
        )
        .unwrap();
        assert_eq!(cm.settings.model, "gpt-4");
    }

    /// Verifies that ConfigManager::reload() succeeds when no config files exist.
    #[test]
    fn test_reload_is_ok() {
        let home = home_dir();
        let project = project_dir();
        let mut cm = ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        assert!(cm.reload().is_ok());
    }

    /// Verifies that session_store_dir() expands ~/ to the home directory path.
    #[test]
    fn test_session_store_dir_expands_tilde() {
        let home = home_dir();
        let project = project_dir();
        let cm = ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        let dir = cm.session_store_dir();
        assert!(!dir.to_string_lossy().starts_with('~'));
    }

    /// Verifies that generation_config() correctly converts Settings into GenerationConfig.
    #[test]
    fn test_generation_config_conversion() {
        let home = home_dir();
        let project = project_dir();
        let cm = ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        let gc = cm.generation_config();
        assert_eq!(gc.max_tokens, 32000);
        assert_eq!(gc.temperature, 0.7);
        assert_eq!(gc.top_p, 1.0);
        assert!(gc.thinking.is_none());
        assert!(gc.tools.is_none());
    }

    /// Verifies that ConfigManager::load reads and applies settings from a user settings file.
    #[test]
    fn test_load_with_user_settings_file() {
        let home = home_dir();
        let project = project_dir();
        let emergence_dir = home.path().join(".emergence");
        fs::create_dir_all(&emergence_dir).unwrap();
        fs::write(
            emergence_dir.join("settings.json"),
            r#"{"model":"custom-model","version":5}"#,
        )
        .unwrap();

        let cm = ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        assert_eq!(cm.settings.model, "custom-model");
        assert_eq!(cm.settings.version, 5);
    }
}
