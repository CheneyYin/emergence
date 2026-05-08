use super::*;
use crate::session::SessionKey;

pub struct SessionsCommand;

#[async_trait::async_trait]
impl Command for SessionsCommand {
    fn name(&self) -> &str { "sessions" }
    fn aliases(&self) -> &[&str] { &["s"] }
    fn description(&self) -> &str { "列出、切换、删除、别名管理会话" }
    fn usage(&self) -> &str { "/sessions [list|load <id|alias>|delete <id|alias>|alias <name>]" }

    async fn execute(&self, args: &[String], ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        let store = ctx.session_store
            .ok_or_else(|| anyhow::anyhow!("SessionStore 不可用"))?;

        match args.first().map(|s| s.as_str()) {
            Some("list") | None => {
                let metas = store.list().await?;
                if metas.is_empty() {
                    return Ok(CommandOutput::Success {
                        message: "没有保存的会话。".into(),
                    });
                }
                let mut msg = format!("会话列表 ({} 个):\n\n", metas.len());
                for meta in &metas {
                    let current = if meta.id == ctx.session.session().id { " ← 当前" } else { "" };
                    let alias = meta.alias.as_deref().unwrap_or("-");
                    msg.push_str(&format!(
                        "  {} | 别名: {} | {} 条消息 | {}{}\n",
                        meta.id, alias, meta.message_count,
                        meta.updated_at.format("%Y-%m-%d %H:%M"), current,
                    ));
                }
                msg.push_str("\n使用 /sessions load <id|别名> 切换会话");
                Ok(CommandOutput::Success { message: msg })
            }
            Some("load") => {
                if let Some(key_str) = args.get(1) {
                    let key = if key_str.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        SessionKey::Id(key_str.clone())
                    } else {
                        SessionKey::Alias(key_str.clone())
                    };
                    match store.load(&key).await? {
                        Some(session) => Ok(CommandOutput::SwitchSession { session }),
                        None => Ok(CommandOutput::Error {
                            message: format!("未找到会话: {}", key_str),
                        }),
                    }
                } else {
                    Ok(CommandOutput::Error {
                        message: "用法: /sessions load <id|别名>".into(),
                    })
                }
            }
            Some("delete") => {
                if let Some(key_str) = args.get(1) {
                    let key = if key_str.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                        SessionKey::Id(key_str.clone())
                    } else {
                        SessionKey::Alias(key_str.clone())
                    };
                    store.delete(&key).await?;
                    Ok(CommandOutput::Success {
                        message: format!("已删除会话: {}", key_str),
                    })
                } else {
                    Ok(CommandOutput::Error {
                        message: "用法: /sessions delete <id|别名>".into(),
                    })
                }
            }
            Some("alias") => {
                if let Some(alias) = args.get(1) {
                    let id = ctx.session.session().id.clone();
                    ctx.session.set_alias(alias.clone());
                    store.set_alias(&id, alias).await?;
                    Ok(CommandOutput::Success {
                        message: format!("已设置别名: {}", alias),
                    })
                } else {
                    Ok(CommandOutput::Error {
                        message: "用法: /sessions alias <name>".into(),
                    })
                }
            }
            _ => Ok(CommandOutput::Error {
                message: "用法: /sessions [list|load <id|alias>|delete <id|alias>|alias <name>]".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::store::{JsonFileStore, SessionStore};
    use crate::session::Session;
    use tempfile::TempDir;

    fn make_ctx(
        store: JsonFileStore,
    ) -> (crate::session::SessionManager, crate::config::ConfigManager, String, bool, JsonFileStore) {
        let home = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let config = crate::config::ConfigManager::load(
            home.path().to_path_buf(), project.path().to_path_buf(), None,
        ).unwrap();
        let session = crate::session::SessionManager::new("test-sess".into());
        (session, config, "default".into(), false, store)
    }

    /// Verifies that SessionsCommand returns Error when no SessionStore is provided.
    #[tokio::test]
    async fn test_missing_store_errors() {
        let (mut session, mut config, mut model, mut should_quit) = {
            let home = TempDir::new().unwrap();
            let project = TempDir::new().unwrap();
            let config = crate::config::ConfigManager::load(
                home.path().to_path_buf(), project.path().to_path_buf(), None,
            ).unwrap();
            let session = crate::session::SessionManager::new("test".into());
            (session, config, "default".into(), false)
        };
        let cmd = SessionsCommand;
        let result = cmd.execute(&[], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: None,
        }).await;
        assert!(result.is_err());
    }

    /// Verifies that /sessions (no args) shows empty list message when no sessions exist.
    #[tokio::test]
    async fn test_sessions_list_empty() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        let (mut session, mut config, mut model, mut should_quit, store) = make_ctx(store);

        let cmd = SessionsCommand;
        let result = cmd.execute(&[], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: Some(&store),
        }).await.unwrap();
        match result {
            CommandOutput::Success { message } => assert!(message.contains("没有保存的会话")),
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that /sessions after saving a session shows it in the list.
    #[tokio::test]
    async fn test_sessions_list_with_data() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        store.save(&Session::new("sess-1".into())).await.unwrap();

        let (mut session, mut config, mut model, mut should_quit, store) = make_ctx(store);

        let cmd = SessionsCommand;
        let result = cmd.execute(&[], &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: Some(&store),
        }).await.unwrap();
        match result {
            CommandOutput::Success { message } => {
                assert!(message.contains("sess-1"));
                assert!(message.contains("会话列表"));
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    /// Verifies that /sessions delete removes a session by numeric-prefixed ID.
    #[tokio::test]
    async fn test_sessions_delete() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        store.save(&Session::new("1-to-delete".into())).await.unwrap();

        let (mut session, mut config, mut model, mut should_quit, store) = make_ctx(store);

        let cmd = SessionsCommand;
        let args = vec!["delete".into(), "1-to-delete".into()];
        let result = cmd.execute(&args, &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: Some(&store),
        }).await.unwrap();
        assert!(matches!(result, CommandOutput::Success { .. }));

        // Verify it's gone
        let loaded = store.load(&SessionKey::Id("1-to-delete".into())).await.unwrap();
        assert!(loaded.is_none());
    }

    /// Verifies that /sessions alias sets a session alias.
    #[tokio::test]
    async fn test_sessions_alias() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        store.save(&Session::new("test-sess".into())).await.unwrap();

        let (mut session, mut config, mut model, mut should_quit, store) = make_ctx(store);

        let cmd = SessionsCommand;
        let args = vec!["alias".into(), "quick-name".into()];
        let result = cmd.execute(&args, &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: Some(&store),
        }).await.unwrap();
        assert!(matches!(result, CommandOutput::Success { .. }));

        // Verify alias was set
        let loaded = store.load(&SessionKey::Alias("quick-name".into())).await.unwrap();
        assert!(loaded.is_some());
    }

    /// Verifies that /sessions load returns SwitchSession when session is found by alias.
    #[tokio::test]
    async fn test_sessions_load() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());
        store.save(&Session::new("1-target".into())).await.unwrap();
        store.set_alias("1-target", "my-alias").await.unwrap();

        let (mut session, mut config, mut model, mut should_quit, store) = make_ctx(store);

        let cmd = SessionsCommand;
        // Non-digit key is resolved as alias
        let args = vec!["load".into(), "my-alias".into()];
        let result = cmd.execute(&args, &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: Some(&store),
        }).await.unwrap();
        assert!(matches!(result, CommandOutput::SwitchSession { .. }));
    }

    /// Verifies that /sessions with unknown subcommand returns usage error.
    #[tokio::test]
    async fn test_sessions_unknown_subcommand() {
        let dir = TempDir::new().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        let (mut session, mut config, mut model, mut should_quit, store) = make_ctx(store);

        let cmd = SessionsCommand;
        let args = vec!["invalid".into()];
        let result = cmd.execute(&args, &mut CommandContext {
            config: &mut config, session: &mut session,
            model: &mut model, should_quit: &mut should_quit,
            skill_registry: None, session_store: Some(&store),
        }).await.unwrap();
        assert!(matches!(result, CommandOutput::Error { .. }));
    }
}
