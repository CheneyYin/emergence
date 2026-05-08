use super::*;

pub struct ModelCommand;

#[async_trait::async_trait]
impl Command for ModelCommand {
    fn name(&self) -> &str { "model" }
    fn aliases(&self) -> &[&str] { &["m"] }
    fn description(&self) -> &str { "快速切换模型" }
    fn usage(&self) -> &str { "/model <provider/model>" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
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
