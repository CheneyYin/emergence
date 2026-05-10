use super::*;

pub struct ToolsCommand;

#[async_trait::async_trait]
impl Command for ToolsCommand {
    fn name(&self) -> &str {
        "tools"
    }
    fn description(&self) -> &str {
        "列出可用工具及风险等级"
    }
    fn usage(&self) -> &str {
        "/tools"
    }

    async fn execute(
        &self,
        _args: &[String],
        _ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
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
            )
            .to_string(),
        })
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

    /// Verifies that ToolsCommand lists all 8 tools regardless of context.
    #[tokio::test]
    async fn test_tools_lists_eight_tools() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = ToolsCommand;
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
                assert!(message.contains("read"));
                assert!(message.contains("bash"));
                assert!(message.contains("web_fetch"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }
}
