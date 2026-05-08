use super::*;

pub struct HelpCommand {
    metas: Vec<CommandMeta>,
}

impl HelpCommand {
    pub fn new(metas: Vec<CommandMeta>) -> Self {
        Self { metas }
    }
}

#[async_trait::async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &str { "help" }
    fn aliases(&self) -> &[&str] { &["?"] }
    fn description(&self) -> &str { "列出所有命令或查看某命令详情" }
    fn usage(&self) -> &str { "/help [command]" }

    async fn execute(&self, args: &[String], _ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if let Some(cmd_name) = args.first() {
            for meta in &self.metas {
                if meta.name == *cmd_name || meta.aliases.iter().any(|a| a == cmd_name) {
                    let aliases_str = if meta.aliases.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", meta.aliases.join(", "))
                    };
                    return Ok(CommandOutput::Success {
                        message: format!("/{} — {}{}\n用法: {}", meta.name, meta.description, aliases_str, meta.usage),
                    });
                }
            }
            return Ok(CommandOutput::Error {
                message: format!("未找到命令 '/{}'", cmd_name),
            });
        }

        let mut msg = String::from("emergence 命令列表:\n\n");
        for meta in &self.metas {
            let alias_str = if meta.aliases.is_empty() {
                String::new()
            } else {
                format!(" ({})", meta.aliases.join(", "))
            };
            msg.push_str(&format!("  /{:12} - {}{}\n", meta.name, meta.description, alias_str));
        }
        Ok(CommandOutput::Success { message: msg })
    }
}
