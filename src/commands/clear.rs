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
