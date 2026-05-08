use super::*;

pub struct ConfigCommand;

#[async_trait::async_trait]
impl Command for ConfigCommand {
    fn name(&self) -> &str { "config" }
    fn description(&self) -> &str { "查看/修改配置" }
    fn usage(&self) -> &str { "/config [model <name>|reload]" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        match args.first().map(|s| s.as_str()) {
            Some("model") => {
                if let Some(model) = args.get(1) {
                    *ctx.model = model.clone();
                    ctx.config.settings.model.clone_from(model);
                    Ok(CommandOutput::Success {
                        message: format!("模型已切换为: {}", model),
                    })
                } else {
                    Ok(CommandOutput::Success {
                        message: format!("当前模型: {}", ctx.config.settings.model),
                    })
                }
            }
            Some("reload") => {
                ctx.config.reload()?;
                Ok(CommandOutput::Success {
                    message: "配置已重载。".into(),
                })
            }
            _ => {
                let s = &ctx.config.settings;
                let msg = format!(
                    "模型: {}\n生成参数: max_tokens={}, temperature={}, top_p={}\nProvider 数: {}\n会话目录: {}",
                    s.model,
                    s.generation.max_tokens,
                    s.generation.temperature,
                    s.generation.top_p,
                    s.providers.len(),
                    s.session.store_dir,
                );
                Ok(CommandOutput::Success { message: msg })
            }
        }
    }
}
