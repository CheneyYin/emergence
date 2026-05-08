use super::*;

pub struct ClearCommand;

#[async_trait::async_trait]
impl Command for ClearCommand {
    fn name(&self) -> &str { "clear" }
    fn description(&self) -> &str { "清空当前对话上下文，保留 system prompt" }
    fn usage(&self) -> &str { "/clear" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        ctx.session.clear();
        Ok(CommandOutput::Success {
            message: "已清空对话上下文。system prompt 和配置保留不变。".into(),
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
        let mut session = crate::session::SessionManager::new("test".into());
        let msg = crate::llm::ChatMessage {
            role: crate::llm::Role::User,
            content: crate::llm::Content::Text("hi".into()),
            name: None, tool_call_id: None,
        };
        session.begin_turn(msg);
        (session, config, "default".into(), false)
    }

    /// Verifies that ClearCommand empties the session turns.
    #[tokio::test]
    async fn test_clear_removes_turns() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        assert!(session.turns().len() > 0);
        let cmd = ClearCommand;
        let result = cmd.execute(&[], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: None,
        }).await.unwrap();
        assert_eq!(session.turns().len(), 0);
        assert!(matches!(result, CommandOutput::Success { .. }));
    }
}
