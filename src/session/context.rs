use crate::llm::{ChatMessage, Content, Role, ToolDefinition};
use super::Session;

/// ContextBuilder — 构建发送给 LLM 的上下文
pub struct ContextBuilder;

impl ContextBuilder {
    /// 构建完整上下文
    /// 上下文展开顺序（对齐设计 §6）：
    ///   SystemMessage(system_prompt + AGENTS.md + <available_skills> + tools)
    ///   → SkillContent(active_skill 1) → SkillContent(active_skill 2) → ...
    ///   → SummaryMessage → Turn[0].messages → Turn[1].messages → ...
    pub fn build(
        session: &Session,
        system_prompt: &str,
        tools: &[ToolDefinition],
        available_skills_text: &str,
        active_skill_contents: &[String],
        project_instructions: Option<&str>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // 1. System prompt 主内容
        let mut system_text = system_prompt.to_string();

        // 2. 添加 AGENTS.md 项目指令
        if let Some(instructions) = project_instructions {
            system_text.push_str(&format!("\n\n<project_instructions>\n{}\n</project_instructions>", instructions));
        }

        // 3. 添加可用 Skill 列表（轻量元信息，对齐设计 §8）
        if !available_skills_text.is_empty() {
            system_text.push_str("\n\n");
            system_text.push_str(available_skills_text);
        }

        // 4. 添加工具列表
        if !tools.is_empty() {
            system_text.push_str("\n\n<available_tools>");
            for tool in tools {
                system_text.push_str(&format!(
                    "\n- tool: {} | desc: {}",
                    tool.name, tool.description
                ));
            }
            system_text.push_str("\n</available_tools>");
        }

        messages.push(ChatMessage {
            role: Role::System,
            content: Content::Text(system_text),
            name: None, tool_call_id: None,
        });

        // 5. 注入 Active Skills 的完整内容
        for skill_content in active_skill_contents {
            messages.push(ChatMessage {
                role: Role::System,
                content: Content::Text(skill_content.clone()),
                name: Some("skill".into()), tool_call_id: None,
            });
        }

        // 6. 摘要（如有）
        if let Some(ref summary) = session.summary {
            messages.push(ChatMessage {
                role: Role::System,
                content: Content::Text(format!("<conversation_summary>\n{}\n</conversation_summary>", summary)),
                name: Some("summary".into()), tool_call_id: None,
            });
        }

        // 7. 展开所有 Turn 消息
        for turn in &session.turns {
            for msg in &turn.messages {
                messages.push(msg.clone());
            }
        }

        messages
    }

    /// 估算 total token count
    pub fn estimated_tokens(messages: &[ChatMessage]) -> u32 {
        let char_count: usize = messages.iter()
            .map(|m| match &m.content {
                Content::Text(t) => t.len(),
                Content::Parts(parts) => parts.iter().map(|p| match p {
                    crate::llm::ContentPart::Text { text } => text.len(),
                    crate::llm::ContentPart::ToolUse { input, .. } => input.to_string().len(),
                    crate::llm::ContentPart::ToolResult { content, .. } => content.len(),
                }).sum(),
            })
            .sum();
        (char_count as f32 * 0.25) as u32
    }

    /// 执行压缩：保留最近 keep_recent 个 turn，将其余转为摘要
    pub fn compact(session: &mut Session, keep_recent: usize) {
        if session.turns.len() <= keep_recent {
            return;
        }

        let old_turns: Vec<_> = session.turns.drain(..session.turns.len() - keep_recent).collect();

        let summary = super::summarizer::Summarizer::summarize_turns(&old_turns, 0);
        session.summary = Some(summary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use crate::llm::{ChatMessage, Content, Role};

    /// Verifies that estimated_tokens returns a positive value and reasonable upper bound for a short message.
    #[test]
    fn test_estimated_tokens() {
        let msgs = vec![
            ChatMessage {
                role: Role::User,
                content: Content::Text("hello world".into()),
                name: None, tool_call_id: None,
            },
        ];
        let tokens = ContextBuilder::estimated_tokens(&msgs);
        assert!(tokens > 0);
        assert!(tokens < 50);
    }

    /// Verifies that compact reduces turn count to keep_recent and produces a summary.
    #[test]
    fn test_compact_reduces_turns() {
        let mut session = Session::new("test".into());
        for i in 0..5 {
            session.turns.push(crate::session::Turn {
                id: format!("t{}", i),
                messages: vec![],
                status: crate::session::TurnStatus::Completed,
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                usage: Default::default(),
            });
        }
        ContextBuilder::compact(&mut session, 2);
        assert_eq!(session.turns.len(), 2);
        assert!(session.summary.is_some());
    }
}
