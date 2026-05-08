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

    /// Verifies that TokensCommand shows token stats including turn and message counts.
    #[tokio::test]
    async fn test_tokens_shows_stats() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = TokensCommand;
        let result = cmd.execute(&[], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: None,
        }).await.unwrap();
        match result {
            CommandOutput::Success { message } => {
                assert!(message.contains("总 token 数"));
                assert!(message.contains("Turn 数"));
                assert!(message.contains("消息数"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }
}
