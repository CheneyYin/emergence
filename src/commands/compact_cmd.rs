use super::*;

pub struct CompactCommand;

#[async_trait::async_trait]
impl Command for CompactCommand {
    fn name(&self) -> &str { "compact" }
    fn description(&self) -> &str { "手动触发上下文压缩" }
    fn usage(&self) -> &str { "/compact [/compact status]" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if args.first().map(|s| s.as_str()) == Some("status") {
            let tokens = ctx.session.estimated_tokens();
            let threshold = ctx.config.settings.session.compaction_threshold_tokens;
            let should = ctx.session.should_compact(threshold);
            return Ok(CommandOutput::Success {
                message: format!(
                    "当前 token 用量: ~{} / {} (阈值 80%)\n状态: {}",
                    tokens,
                    threshold,
                    if should { "需要压缩" } else { "不需要压缩" }
                ),
            });
        }

        let threshold = ctx.config.settings.session.compaction_threshold_tokens;
        if ctx.session.should_compact(threshold) {
            ctx.session.compact(3);
            Ok(CommandOutput::Success {
                message: format!("压缩完成。当前 token 用量: ~{}", ctx.session.estimated_tokens()),
            })
        } else {
            Ok(CommandOutput::Success {
                message: "当前 token 用量未达阈值，无需压缩。".into(),
            })
        }
    }
}
