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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> (crate::session::SessionManager, crate::config::ConfigManager, String, bool) {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let config = crate::config::ConfigManager::load(
            home.path().to_path_buf(), project.path().to_path_buf(), None,
        ).unwrap();
        let session = crate::session::SessionManager::new("test".into());
        (session, config, "default".into(), false)
    }

    /// Verifies that /config (no args) shows current configuration summary.
    #[tokio::test]
    async fn test_config_shows_summary() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = ConfigCommand;
        let result = cmd.execute(&[], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: None,
        }).await.unwrap();
        match result {
            CommandOutput::Success { message } => {
                assert!(message.contains("max_tokens"));
                assert!(message.contains("Provider 数"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that /config model <name> switches the model.
    #[tokio::test]
    async fn test_config_model_switches() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = ConfigCommand;
        let result = cmd.execute(&["model".into(), "claude-opus".into()], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: None,
        }).await.unwrap();
        assert_eq!(model, "claude-opus");
        assert_eq!(config.settings.model, "claude-opus");
        assert!(matches!(result, CommandOutput::Success { .. }));
    }

    /// Verifies that /config reload succeeds even with no config file changes.
    #[tokio::test]
    async fn test_config_reload() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = ConfigCommand;
        let result = cmd.execute(&["reload".into()], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: None,
        }).await.unwrap();
        assert!(matches!(result, CommandOutput::Success { .. }));
    }
}
