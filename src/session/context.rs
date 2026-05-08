use crate::llm::{ChatMessage, ToolDefinition};
use super::Session;

pub struct ContextBuilder;

impl ContextBuilder {
    pub fn build(
        session: &Session,
        system_prompt: &str,
        _tools: &[ToolDefinition],
        _available_skills_text: &str,
        _active_skill_contents: &[String],
        _project_instructions: Option<&str>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        // 注入 system prompt
        messages.push(ChatMessage {
            role: crate::llm::Role::System,
            content: crate::llm::Content::Text(system_prompt.to_string()),
            name: None,
            tool_call_id: None,
        });
        // 附加会话历史
        for turn in &session.turns {
            messages.extend(turn.messages.clone());
        }
        messages
    }

    pub fn compact(session: &mut Session, keep_recent: usize) {
        if session.turns.len() <= keep_recent {
            return;
        }
        let split_at = session.turns.len() - keep_recent;
        session.turns = session.turns.split_off(split_at);
    }
}
