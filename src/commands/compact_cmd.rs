use super::*;

pub struct CompactCommand;

#[async_trait::async_trait]
impl Command for CompactCommand {
    fn name(&self) -> &str {
        "compact"
    }
    fn description(&self) -> &str {
        "手动触发上下文压缩"
    }
    fn usage(&self) -> &str {
        "/compact [/compact status]"
    }

    async fn execute(
        &self,
        args: &[String],
        ctx: &mut CommandContext<'_>,
    ) -> anyhow::Result<CommandOutput> {
        if args.first().map(|s| s.as_str()) == Some("status") {
            let tokens = ctx.session.estimated_tokens();
            let threshold = ctx.config.settings.session.compaction_threshold_tokens;
            let should = ctx.session.should_compact(threshold);
            return Ok(CommandOutput::Success {
                message: format!(
                    "当前 token 用量: ~{} / {} (阈值 80%)\n状态: {}",
                    tokens,
                    threshold,
                    if should {
                        "需要压缩"
                    } else {
                        "不需要压缩"
                    }
                ),
            });
        }

        let threshold = ctx.config.settings.session.compaction_threshold_tokens;
        if ctx.session.should_compact(threshold) {
            ctx.session.compact(3);
            Ok(CommandOutput::Success {
                message: format!(
                    "压缩完成。当前 token 用量: ~{}",
                    ctx.session.estimated_tokens()
                ),
            })
        } else {
            Ok(CommandOutput::Success {
                message: "当前 token 用量未达阈值，无需压缩。".into(),
            })
        }
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

    /// Verifies that /compact status reports token usage without modifying turns.
    #[tokio::test]
    async fn test_compact_status() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = CompactCommand;
        let result = cmd
            .execute(
                &["status".into()],
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
                assert!(message.contains("token 用量"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that /compact on an empty session reports no compression needed.
    #[tokio::test]
    async fn test_compact_empty_session() {
        let (mut session, mut config, mut model, mut should_quit) = make_ctx();
        let cmd = CompactCommand;
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
            CommandOutput::Success { message } => assert!(message.contains("无需压缩")),
            other => panic!("expected Success, got {:?}", other),
        }
    }
}
