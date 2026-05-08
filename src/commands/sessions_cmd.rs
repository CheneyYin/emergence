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
