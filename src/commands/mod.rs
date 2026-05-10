use std::collections::HashMap;

pub mod clear;
pub mod compact_cmd;
pub mod config_cmd;
pub mod help;
pub mod model_cmd;
pub mod quit;
pub mod sessions_cmd;
pub mod skills_cmd;
pub mod tokens_cmd;
pub mod tools_cmd;

/// 命令执行上下文 — 命令可访问的子系统引用
pub struct CommandContext<'a> {
    pub config: &'a mut crate::config::ConfigManager,
    pub session: &'a mut crate::session::SessionManager,
    pub model: &'a mut String,
    pub should_quit: &'a mut bool,
    pub skill_registry: Option<&'a crate::skills::SkillRegistry>,
    pub session_store: Option<&'a dyn crate::session::store::SessionStore>,
}

/// 命令执行输出
#[derive(Debug, Clone)]
pub enum CommandOutput {
    Success { message: String },
    Error { message: String },
    Quit,
    SwitchSession { session: crate::session::Session },
}

/// 命令元信息
#[derive(Debug, Clone)]
pub struct CommandMeta {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub usage: String,
}

/// 建议项（模糊匹配用）
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub name: String,
    pub distance: usize,
}

/// Command trait
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> &[&str] {
        &[]
    }
    fn description(&self) -> &str;
    fn usage(&self) -> &str;
    async fn execute(
        &self,
        args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput>;
}

/// 命令注册表
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
    /// 所有已知命令名（用于模糊匹配）
    known_names: Vec<String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            known_names: Vec::new(),
        }
    }

    pub fn register<C: Command + 'static>(&mut self, cmd: C) {
        let name = cmd.name().to_string();
        for alias in cmd.aliases() {
            self.known_names.push(alias.to_string());
        }
        self.known_names.push(name.clone());
        self.commands.insert(name, Box::new(cmd));
    }

    /// 解析 /command input 并分发
    pub async fn dispatch(
        &self,
        input: &str,
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
        let trimmed = input.trim().trim_start_matches('/');
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.is_empty() {
            return Ok(CommandOutput::Error {
                message: "空命令".into(),
            });
        }

        let name = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        // 精确匹配
        if let Some(cmd) = self.commands.get(name) {
            return cmd.execute(&args, ctx).await;
        }

        // 别名匹配
        for (_, cmd) in &self.commands {
            if cmd.aliases().contains(&name) {
                return cmd.execute(&args, ctx).await;
            }
        }

        // 模糊匹配
        let suggestions = self.fuzzy_find(name);
        if !suggestions.is_empty() {
            let hint = suggestions
                .iter()
                .map(|s| format!("→ /{} ({})", s.name, s.distance))
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(CommandOutput::Error {
                message: format!("未知命令 '/{}'。你可能是想:\n{}", name, hint),
            });
        }

        Ok(CommandOutput::Error {
            message: format!("未知命令 '/{}'。输入 /help 查看所有命令。", name),
        })
    }

    /// 模糊匹配（Levenshtein 距离 ≤ 3）
    pub fn fuzzy_find(&self, input: &str) -> Vec<Suggestion> {
        let mut suggestions: Vec<Suggestion> = self
            .known_names
            .iter()
            .filter_map(|name| {
                let distance = crate::utils::fuzzy::levenshtein_distance(input, name);
                if distance <= 3 {
                    Some(Suggestion {
                        name: name.clone(),
                        distance,
                    })
                } else {
                    None
                }
            })
            .collect();
        suggestions.sort_by_key(|s| s.distance);
        suggestions.truncate(3);
        suggestions
    }

    /// 注册所有内置命令
    /// HelpCommand 最后注册，以便获取所有命令的 meta
    pub fn register_all(&mut self) {
        self.register(clear::ClearCommand);
        self.register(compact_cmd::CompactCommand);
        self.register(config_cmd::ConfigCommand);
        self.register(sessions_cmd::SessionsCommand);
        self.register(quit::QuitCommand);
        self.register(model_cmd::ModelCommand);
        self.register(tokens_cmd::TokensCommand);
        self.register(tools_cmd::ToolsCommand);
        self.register(skills_cmd::SkillsCommand);
        self.register(skills_cmd::SkillCommand);

        // HelpCommand 最后注册，获取所有已注册命令的 metas
        let metas = self.list();
        self.register(help::HelpCommand::new(metas));
    }

    pub fn list(&self) -> Vec<CommandMeta> {
        let mut seen = std::collections::HashSet::new();
        let mut metas = Vec::new();
        for (_, cmd) in &self.commands {
            if seen.insert(cmd.name().to_string()) {
                metas.push(CommandMeta {
                    name: cmd.name().to_string(),
                    aliases: cmd.aliases().iter().map(|s| s.to_string()).collect(),
                    description: cmd.description().to_string(),
                    usage: cmd.usage().to_string(),
                });
            }
        }
        metas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand;

    #[async_trait::async_trait]
    impl Command for TestCommand {
        fn name(&self) -> &str {
            "test"
        }
        fn aliases(&self) -> &[&str] {
            &["t"]
        }
        fn description(&self) -> &str {
            "测试命令"
        }
        fn usage(&self) -> &str {
            "/test"
        }
        async fn execute(
            &self,
            _args: &[String],
            _ctx: &mut CommandContext<'_>,
        ) -> anyhow::Result<CommandOutput> {
            Ok(CommandOutput::Success {
                message: "ok".into(),
            })
        }
    }

    /// Verifies that a registered command appears in list() with correct metadata.
    #[test]
    fn test_register_and_list() {
        let mut registry = CommandRegistry::new();
        registry.register(TestCommand);
        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test");
    }

    /// Verifies that fuzzy_find suggests close matches within Levenshtein distance ≤ 3.
    #[test]
    fn test_fuzzy_find() {
        let mut registry = CommandRegistry::new();
        registry.register(TestCommand);
        let suggestions = registry.fuzzy_find("tst");
        assert!(!suggestions.is_empty());
    }
}
