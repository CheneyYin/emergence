use super::*;

pub struct QuitCommand;

#[async_trait::async_trait]
impl Command for QuitCommand {
    fn name(&self) -> &str { "quit" }
    fn aliases(&self) -> &[&str] { &["q", "exit"] }
    fn description(&self) -> &str { "退出程序" }
    fn usage(&self) -> &str { "/quit" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        *ctx.should_quit = true;
        Ok(CommandOutput::Quit)
    }
}
