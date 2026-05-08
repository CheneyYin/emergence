/// 调用 LLM 生成对话摘要
pub struct Summarizer;

impl Summarizer {
    /// 生成压缩摘要（v1: 简单截断策略）
    /// 完整版需要调用 LLM，此处使用简单策略：
    /// 保留最近 N 个 Turn，将早期 Turn 压缩为摘要
    pub fn summarize_turns(turns: &[super::Turn], recent_keep: usize) -> String {
        if turns.len() <= recent_keep {
            return "".to_string();
        }

        let old_turns = &turns[..turns.len() - recent_keep];
        let mut summary = String::from("此前对话摘要:\n");

        for turn in old_turns {
            summary.push_str(&format!("[Turn {}]: ", turn.id));
            for msg in &turn.messages {
                match msg.role {
                    crate::llm::Role::User => {
                        if let crate::llm::Content::Text(ref t) = msg.content {
                            summary.push_str(&format!("用户: {} | ", t.chars().take(100).collect::<String>()));
                        }
                    }
                    crate::llm::Role::Assistant => {
                        if let crate::llm::Content::Text(ref t) = msg.content {
                            summary.push_str(&format!("助手: {} | ", t.chars().take(200).collect::<String>()));
                        }
                    }
                    _ => {}
                }
            }
            summary.push('\n');
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Turn;
    use crate::llm::{ChatMessage, Content, Role};
    use chrono::Utc;

    /// Verifies that summarize_turns includes old turns and excludes recent ones.
    #[test]
    fn test_summarize_turns() {
        let turns: Vec<Turn> = (1..=5).map(|i| Turn {
            id: format!("turn-{}", i),
            messages: vec![
                ChatMessage { role: Role::User, content: Content::Text(format!("question {}", i)), name: None, tool_call_id: None },
                ChatMessage { role: Role::Assistant, content: Content::Text(format!("answer {}", i)), name: None, tool_call_id: None },
            ],
            status: crate::session::TurnStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            usage: Default::default(),
        }).collect();

        let summary = Summarizer::summarize_turns(&turns, 3);
        assert!(summary.contains("turn-1"));
        assert!(summary.contains("turn-2"));
        assert!(!summary.contains("turn-5")); // 保留最近 3 个
    }

    /// Verifies that summarize_turns returns empty string when all turns are within keep range.
    #[test]
    fn test_summarize_all_recent_returns_empty() {
        let turns: Vec<Turn> = (1..=2).map(|i| Turn {
            id: format!("turn-{}", i),
            messages: vec![],
            status: crate::session::TurnStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            usage: Default::default(),
        }).collect();

        let summary = Summarizer::summarize_turns(&turns, 3);
        assert!(summary.is_empty());
    }

    /// Verifies that user messages are truncated at 100 chars and assistant at 200 chars.
    #[test]
    fn test_summarize_truncates_long_messages() {
        let long_user = "x".repeat(150);
        let long_asst = "y".repeat(250);
        let turns = vec![Turn {
            id: "turn-1".into(),
            messages: vec![
                ChatMessage { role: Role::User, content: Content::Text(long_user), name: None, tool_call_id: None },
                ChatMessage { role: Role::Assistant, content: Content::Text(long_asst), name: None, tool_call_id: None },
            ],
            status: crate::session::TurnStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            usage: Default::default(),
        }];

        let summary = Summarizer::summarize_turns(&turns, 0);
        // 用户消息不应超过 ~100 字符
        assert!(!summary.contains(&"x".repeat(101)));
        // 助手消息不应超过 ~200 字符
        assert!(!summary.contains(&"y".repeat(201)));
    }
}
