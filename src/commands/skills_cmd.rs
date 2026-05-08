use super::*;

pub struct SkillsCommand;

#[async_trait::async_trait]
impl Command for SkillsCommand {
    fn name(&self) -> &str { "skills" }
    fn description(&self) -> &str { "列出可用 skill" }
    fn usage(&self) -> &str { "/skills" }

    async fn execute(&self, _args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if let Some(sr) = ctx.skill_registry {
            let skills = sr.list();
            if skills.is_empty() {
                return Ok(CommandOutput::Success {
                    message: "暂无可用 skill。在 ~/.emergence/skills/ 或 ./.emergence/skills/ 中添加 .md 文件。".into(),
                });
            }
            let mut msg = String::from("可用 Skills:\n\n");
            for meta in skills {
                let source = match meta.source {
                    crate::skills::SkillSource::User => "[user]",
                    crate::skills::SkillSource::Project => "[project]",
                };
                msg.push_str(&format!("  {} {} | {}\n", meta.name, source, meta.description));
            }
            Ok(CommandOutput::Success { message: msg })
        } else {
            Ok(CommandOutput::Error { message: "SkillRegistry 不可用".into() })
        }
    }
}

pub struct SkillCommand;

#[async_trait::async_trait]
impl Command for SkillCommand {
    fn name(&self) -> &str { "skill" }
    fn description(&self) -> &str { "激活/停用 skill" }
    fn usage(&self) -> &str { "/skill <name> 或 /skill --off <name>" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        if args.first().map(|s| s.as_str()) == Some("--off") {
            if let Some(name) = args.get(1) {
                ctx.session.deactivate_skill(name)?;
                return Ok(CommandOutput::Success {
                    message: format!("已停用 skill: {}", name),
                });
            }
            return Ok(CommandOutput::Error {
                message: "用法: /skill --off <name>".into(),
            });
        }

        if let Some(name) = args.first() {
            if let Some(sr) = ctx.skill_registry {
                if sr.load_full_content(name).is_err() {
                    return Ok(CommandOutput::Error {
                        message: format!("skill '{}' 不存在。使用 /skills 查看可用 skill。", name),
                    });
                }
            }
            ctx.session.activate_skill(name)?;
            Ok(CommandOutput::Success {
                message: format!("已激活 skill: {}", name),
            })
        } else {
            let active = ctx.session.active_skills();
            if active.is_empty() {
                Ok(CommandOutput::Success {
                    message: "当前无激活的 skill。使用 /skills 查看可用 skill，/skill <name> 激活。".into(),
                })
            } else {
                Ok(CommandOutput::Success {
                    message: format!("当前激活的 skill: {}", active.join(", ")),
                })
            }
        }
    }
}
