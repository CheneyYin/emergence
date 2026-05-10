use super::*;

pub struct SkillsCommand;

#[async_trait::async_trait]
impl Command for SkillsCommand {
    fn name(&self) -> &str {
        "skills"
    }
    fn description(&self) -> &str {
        "列出可用 skill"
    }
    fn usage(&self) -> &str {
        "/skills"
    }

    async fn execute(
        &self,
        _args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
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
                msg.push_str(&format!(
                    "  {} {} | {}\n",
                    meta.name, source, meta.description
                ));
            }
            Ok(CommandOutput::Success { message: msg })
        } else {
            Ok(CommandOutput::Error {
                message: "SkillRegistry 不可用".into(),
            })
        }
    }
}

pub struct SkillCommand;

#[async_trait::async_trait]
impl Command for SkillCommand {
    fn name(&self) -> &str {
        "skill"
    }
    fn description(&self) -> &str {
        "激活/停用 skill"
    }
    fn usage(&self) -> &str {
        "/skill <name> 或 /skill --off <name>"
    }

    async fn execute(
        &self,
        args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
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
                    message:
                        "当前无激活的 skill。使用 /skills 查看可用 skill，/skill <name> 激活。"
                            .into(),
                })
            } else {
                Ok(CommandOutput::Success {
                    message: format!("当前激活的 skill: {}", active.join(", ")),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_ctx(
        sr: Option<crate::skills::SkillRegistry>,
    ) -> (
        crate::session::SessionManager,
        crate::config::ConfigManager,
        String,
        bool,
        crate::skills::SkillRegistry,
    ) {
        let home = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let config = crate::config::ConfigManager::load(
            home.path().to_path_buf(),
            project.path().to_path_buf(),
            None,
        )
        .unwrap();
        let session = crate::session::SessionManager::new("test".into());
        (
            session,
            config,
            "default".into(),
            false,
            sr.unwrap_or_else(crate::skills::SkillRegistry::new),
        )
    }

    /// Verifies that SkillsCommand returns Error when no SkillRegistry is provided.
    #[tokio::test]
    async fn test_skills_no_registry() {
        let (mut session, mut config, mut model, mut should_quit) = {
            let home = TempDir::new().unwrap();
            let project = TempDir::new().unwrap();
            let config = crate::config::ConfigManager::load(
                home.path().to_path_buf(),
                project.path().to_path_buf(),
                None,
            )
            .unwrap();
            let session = crate::session::SessionManager::new("test".into());
            (session, config, "default".into(), false)
        };
        let cmd = SkillsCommand;
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
        assert!(matches!(result, CommandOutput::Error { .. }));
    }

    /// Verifies that SkillsCommand with empty registry returns empty skills message.
    #[tokio::test]
    async fn test_skills_empty_registry() {
        let (mut session, mut config, mut model, mut should_quit, sr) = make_ctx(None);
        let cmd = SkillsCommand;
        let result = cmd
            .execute(
                &[],
                &mut CommandContext {
                    config: &mut config,
                    session: &mut session,
                    model: &mut model,
                    should_quit: &mut should_quit,
                    skill_registry: Some(&sr),
                    session_store: None,
                },
            )
            .await
            .unwrap();
        match result {
            CommandOutput::Success { message } => assert!(message.contains("暂无可用 skill")),
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that SkillCommand activates a skill and it appears in active_skills.
    #[tokio::test]
    async fn test_skill_activate() {
        let (mut session, mut config, mut model, mut should_quit, _sr) = make_ctx(None);

        let cmd = SkillCommand;
        let result = cmd
            .execute(
                &["rust".into()],
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
            CommandOutput::Success { message } => assert!(message.contains("已激活")),
            other => panic!("expected Success, got {:?}", other),
        }
        assert_eq!(session.active_skills(), &["rust"]);
    }

    /// Verifies that SkillCommand --off deactivates a skill.
    #[tokio::test]
    async fn test_skill_deactivate() {
        let (mut session, mut config, mut model, mut should_quit, _sr) = make_ctx(None);
        session.activate_skill("rust").unwrap();

        let cmd = SkillCommand;
        let args = vec!["--off".into(), "rust".into()];
        let result = cmd
            .execute(
                &args,
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
            CommandOutput::Success { message } => assert!(message.contains("已停用")),
            other => panic!("expected Success, got {:?}", other),
        }
        assert!(session.active_skills().is_empty());
    }

    /// Verifies that SkillCommand without args shows active skills.
    #[tokio::test]
    async fn test_skill_show_active() {
        let (mut session, mut config, mut model, mut should_quit, _sr) = make_ctx(None);
        session.activate_skill("typescript").unwrap();

        let cmd = SkillCommand;
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
                assert!(message.contains("typescript"));
                assert!(message.contains("当前激活"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that SkillCommand without args when no skills are active shows empty message.
    #[tokio::test]
    async fn test_skill_show_empty() {
        let (mut session, mut config, mut model, mut should_quit, _sr) = make_ctx(None);

        let cmd = SkillCommand;
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
            CommandOutput::Success { message } => assert!(message.contains("当前无激活")),
            other => panic!("expected Success, got {:?}", other),
        }
    }
}
