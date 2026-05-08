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
