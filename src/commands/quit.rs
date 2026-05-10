use super::*;

pub struct QuitCommand;

#[async_trait::async_trait]
impl Command for QuitCommand {
    fn name(&self) -> &str {
        "quit"
    }
    fn aliases(&self) -> &[&str] {
        &["q", "exit"]
    }
    fn description(&self) -> &str {
        "退出程序"
    }
    fn usage(&self) -> &str {
        "/quit"
    }

    async fn execute(
        &self,
        _args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
        *ctx.should_quit = true;
        Ok(CommandOutput::Quit)
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
        (session, config, "default".into(), false)
    }

    /// Verifies that QuitCommand sets should_quit and returns CommandOutput::Quit.
    #[tokio::test]
    async fn test_quit_sets_flag() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = QuitCommand;
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
        assert!(should_quit);
        assert!(matches!(result, CommandOutput::Quit));
    }
}
