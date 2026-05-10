use super::*;

pub struct ModelCommand;

#[async_trait::async_trait]
impl Command for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }
    fn aliases(&self) -> &[&str] {
        &["m"]
    }
    fn description(&self) -> &str {
        "快速切换模型"
    }
    fn usage(&self) -> &str {
        "/model <provider/model>"
    }

    async fn execute(
        &self,
        args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> (
        crate::session::SessionManager,
        crate::config::ConfigManager,
        String,
        bool,
    ) {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let config = crate::config::ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        let session = crate::session::SessionManager::new("test".into());
        (session, config, "deepseek/v4".into(), false)
    }

    /// Verifies that ModelCommand with no args shows current model.
    #[tokio::test]
    async fn test_model_shows_current() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = ModelCommand;
        let result = cmd
            .execute(
                &[],
                &mut CommandContext {
                    config: &mut config,
                    session: &mut session,
                    model: &mut model,
                    should_quit: &mut should_quit,
                    skill_registry: None,
                    session_store: None,
                },
            )
            .await
            .unwrap();
        match result {
            CommandOutput::Success { message } => {
                assert!(message.contains("deepseek/deepseek-v4-pro"))
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that ModelCommand with an arg switches the model.
    #[tokio::test]
    async fn test_model_switches() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = ModelCommand;
        let result = cmd
            .execute(
                &["gpt-4".into()],
                &mut CommandContext {
                    config: &mut config,
                    session: &mut session,
                    model: &mut model,
                    should_quit: &mut should_quit,
                    skill_registry: None,
                    session_store: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(model, "gpt-4");
        match result {
            CommandOutput::Success { message } => assert!(message.contains("gpt-4")),
            other => panic!("expected Success, got {:?}", other),
        }
    }
}
