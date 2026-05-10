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
    fn name(&self) -> &str {
        "help"
    }
    fn aliases(&self) -> &[&str] {
        &["?"]
    }
    fn description(&self) -> &str {
        "列出所有命令或查看某命令详情"
    }
    fn usage(&self) -> &str {
        "/help [command]"
    }

    async fn execute(
        &self,
        args: &[String],
        _ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
        if let Some(cmd_name) = args.first() {
            for meta in &self.metas {
                if meta.name == *cmd_name || meta.aliases.iter().any(|a| a == cmd_name) {
                    let aliases_str = if meta.aliases.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", meta.aliases.join(", "))
                    };
                    return Ok(CommandOutput::Success {
                        message: format!(
                            "/{} — {}{}\n用法: {}",
                            meta.name, meta.description, aliases_str, meta.usage
                        ),
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
            msg.push_str(&format!(
                "  /{:12} - {}{}\n",
                meta.name, meta.description, alias_str
            ));
        }
        Ok(CommandOutput::Success { message: msg })
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

    fn sample_metas() -> Vec<CommandMeta> {
        vec![
            CommandMeta {
                name: "quit".into(),
                aliases: vec!["q".into()],
                description: "退出".into(),
                usage: "/quit".into(),
            },
            CommandMeta {
                name: "clear".into(),
                aliases: vec![],
                description: "清空".into(),
                usage: "/clear".into(),
            },
        ]
    }

    /// Verifies that HelpCommand with no args lists all commands.
    #[tokio::test]
    async fn test_help_lists_all() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = HelpCommand::new(sample_metas());
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
                assert!(message.contains("quit"));
                assert!(message.contains("clear"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that HelpCommand with a known name shows details.
    #[tokio::test]
    async fn test_help_single_command() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = HelpCommand::new(sample_metas());
        let result = cmd
            .execute(
                &["quit".into()],
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
            CommandOutput::Success { message } => assert!(message.contains("/quit")),
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that HelpCommand with unknown name returns Error.
    #[tokio::test]
    async fn test_help_unknown_command() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = HelpCommand::new(sample_metas());
        let result = cmd
            .execute(
                &["nonexistent".into()],
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
        assert!(matches!(result, CommandOutput::Error { .. }));
    }
}
