use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::llm::{ChatMessage, Usage};

pub mod context;
pub mod store;

/// 会话 ID
pub type SessionId = String;

/// Turn ID
pub type TurnId = String;

/// 会话查找键
#[derive(Debug, Clone)]
pub enum SessionKey {
    Id(SessionId),
    Alias(String),
}

/// 会话元信息（列表用，不含消息体）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: SessionId,
    pub alias: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub summary: Option<String>,
}

/// Turn 状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    InProgress,
    Completed,
}

/// 一个对话轮次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: TurnId,
    pub messages: Vec<ChatMessage>,
    pub status: TurnStatus,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub usage: Usage,
}

/// 会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub alias: Option<String>,
    pub turns: Vec<Turn>,
    pub summary: Option<String>,
    pub active_skills: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Session {
    pub fn new(id: SessionId) -> Self {
        let now = Utc::now();
        Self {
            id,
            alias: None,
            turns: Vec::new(),
            summary: None,
            active_skills: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn message_count(&self) -> usize {
        self.turns.iter().map(|t| t.messages.len()).sum()
    }
}

/// SessionManager — 管理会话生命周期
pub struct SessionManager {
    session: Session,
    turn_counter: usize,
}

impl SessionManager {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session: Session::new(session_id),
            turn_counter: 0,
        }
    }

    pub fn load(session: Session) -> Self {
        let turn_counter = session.turns.len();
        Self {
            session,
            turn_counter,
        }
    }

    pub fn current_turn(&self) -> Option<&Turn> {
        self.session.turns.last()
    }

    pub fn turns(&self) -> &[Turn] {
        &self.session.turns
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// 开始新 Turn
    pub fn begin_turn(&mut self, user_message: ChatMessage) -> &Turn {
        self.turn_counter += 1;
        let turn = Turn {
            id: format!("turn-{}", self.turn_counter),
            messages: vec![user_message],
            status: TurnStatus::InProgress,
            started_at: Utc::now(),
            completed_at: None,
            usage: Usage::default(),
        };
        self.session.turns.push(turn);
        self.session.updated_at = Utc::now();
        self.session.turns.last().unwrap()
    }

    /// 向当前 Turn 追加消息
    pub fn push(&mut self, message: ChatMessage) -> anyhow::Result<()> {
        let turn = self.session.turns.last_mut()
            .ok_or_else(|| anyhow::anyhow!("没有进行中的 turn"))?;
        turn.messages.push(message);
        self.session.updated_at = Utc::now();
        Ok(())
    }

    /// 完成当前 Turn
    pub fn complete_turn(&mut self) -> anyhow::Result<()> {
        let turn = self.session.turns.last_mut()
            .ok_or_else(|| anyhow::anyhow!("没有进行中的 turn"))?;
        turn.status = TurnStatus::Completed;
        turn.completed_at = Some(Utc::now());
        self.session.updated_at = Utc::now();
        Ok(())
    }

    /// 构建发送给 LLM 的消息上下文（委托给 ContextBuilder）
    pub fn build_context(
        &self,
        system_prompt: &str,
        tools: &[crate::llm::ToolDefinition],
        available_skills_text: &str,
        active_skill_contents: &[String],
        project_instructions: Option<&str>,
    ) -> Vec<ChatMessage> {
        crate::session::context::ContextBuilder::build(
            self.session(),
            system_prompt,
            tools,
            available_skills_text,
            active_skill_contents,
            project_instructions,
        )
    }

    /// 估算上下文 token 数（粗略：每字符 0.25 tokens）
    pub fn estimated_tokens(&self) -> u32 {
        let char_count: usize = self.session.turns.iter()
            .flat_map(|t| t.messages.iter())
            .map(|m| match &m.content {
                crate::llm::Content::Text(t) => t.len(),
                crate::llm::Content::Parts(parts) => parts.iter()
                    .map(|p| match p {
                        crate::llm::ContentPart::Text { text } => text.len(),
                        crate::llm::ContentPart::ToolUse { input, .. } => input.to_string().len(),
                        crate::llm::ContentPart::ToolResult { content, .. } => content.len(),
                    })
                    .sum(),
            })
            .sum();
        (char_count as f32 * 0.25) as u32
    }

    /// 判断是否需要压缩
    pub fn should_compact(&self, threshold: u32) -> bool {
        self.estimated_tokens() > ((threshold as f32) * 0.8) as u32
    }

    /// 执行 compaction（保留最近 keep_recent 个 Turn，其余转为摘要）
    pub fn compact(&mut self, keep_recent: usize) {
        crate::session::context::ContextBuilder::compact(&mut self.session, keep_recent);
    }

    /// 清除所有消息（/clear 命令）
    pub fn clear(&mut self) {
        self.session.turns.clear();
        self.session.summary = None;
        self.turn_counter = 0;
    }

    /// 设置别名
    pub fn set_alias(&mut self, alias: String) {
        self.session.alias = Some(alias);
    }

    // Skill 管理
    pub fn activate_skill(&mut self, name: &str) -> anyhow::Result<()> {
        if !self.session.active_skills.contains(&name.to_string()) {
            self.session.active_skills.push(name.to_string());
        }
        Ok(())
    }

    pub fn deactivate_skill(&mut self, name: &str) -> anyhow::Result<()> {
        self.session.active_skills.retain(|s| s != name);
        Ok(())
    }

    pub fn active_skills(&self) -> &[String] {
        &self.session.active_skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Role, Content};

    fn make_user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content: Content::Text(text.to_string()),
            name: None, tool_call_id: None,
        }
    }

    #[test]
    fn test_begin_and_complete_turn() {
        let mut sm = SessionManager::new("test-1".into());

        sm.begin_turn(make_user_msg("hello"));
        assert_eq!(sm.turns().len(), 1);
        assert_eq!(sm.current_turn().unwrap().status, TurnStatus::InProgress);

        sm.complete_turn().unwrap();
        assert_eq!(sm.current_turn().unwrap().status, TurnStatus::Completed);
    }

    #[test]
    fn test_push_message() {
        let mut sm = SessionManager::new("test-2".into());
        sm.begin_turn(make_user_msg("hello"));
        sm.push(make_user_msg("world")).unwrap();
        assert_eq!(sm.current_turn().unwrap().messages.len(), 2);
    }

    #[test]
    fn test_build_context() {
        let mut sm = SessionManager::new("test-3".into());
        sm.begin_turn(make_user_msg("hello"));
        sm.complete_turn().unwrap();

        let ctx = sm.build_context("You are helpful. Be concise.", &[], "", &[], None);
        assert_eq!(ctx.first().unwrap().role, Role::System);
        assert!(ctx.iter().any(|m| matches!(&m.content, Content::Text(t) if t == "hello")));
    }

    #[test]
    fn test_estimated_tokens_positive() {
        let mut sm = SessionManager::new("test-4".into());
        sm.begin_turn(make_user_msg("hello world this is a test message"));
        let tokens = sm.estimated_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_clear() {
        let mut sm = SessionManager::new("test-5".into());
        sm.begin_turn(make_user_msg("hello"));
        sm.complete_turn().unwrap();
        sm.clear();
        assert_eq!(sm.turns().len(), 0);
    }

    #[test]
    fn test_push_without_turn_returns_error() {
        let mut sm = SessionManager::new("test-6".into());
        let result = sm.push(make_user_msg("orphan"));
        assert!(result.is_err());
    }

    #[test]
    fn test_complete_without_turn_returns_error() {
        let mut sm = SessionManager::new("test-7".into());
        let result = sm.complete_turn();
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_existing_session() {
        let mut sm = SessionManager::new("test-8".into());
        sm.begin_turn(make_user_msg("msg1"));
        sm.complete_turn().unwrap();
        sm.begin_turn(make_user_msg("msg2"));

        // 通过 load 重建，turn_counter 应匹配
        let session = sm.session().clone();
        let sm2 = SessionManager::load(session);
        assert_eq!(sm2.turns().len(), 2);
        assert_eq!(sm2.current_turn().unwrap().messages.len(), 1);
    }

    #[test]
    fn test_skill_management() {
        let mut sm = SessionManager::new("test-9".into());

        sm.activate_skill("typescript").unwrap();
        sm.activate_skill("rust").unwrap();
        assert_eq!(sm.active_skills(), &["typescript", "rust"]);

        // 重复激活不重复添加
        sm.activate_skill("rust").unwrap();
        assert_eq!(sm.active_skills().len(), 2);

        sm.deactivate_skill("typescript").unwrap();
        assert_eq!(sm.active_skills(), &["rust"]);
    }

    #[test]
    fn test_set_alias_and_message_count() {
        let mut sm = SessionManager::new("test-10".into());
        sm.set_alias("my-session".into());
        assert_eq!(sm.session().alias.as_deref(), Some("my-session"));

        sm.begin_turn(make_user_msg("a"));
        sm.push(make_user_msg("b")).unwrap();
        sm.begin_turn(make_user_msg("c"));
        // Session::message_count 统计所有 turn 的所有消息
        assert_eq!(sm.session().message_count(), 3);
    }

    #[test]
    fn test_should_compact() {
        let mut sm = SessionManager::new("test-11".into());
        // 空会话 — 不应压缩
        assert!(!sm.should_compact(100_000));

        // 添加一长消息使 token 超过 80% 阈值
        let long_msg = "x".repeat(10_000);
        sm.begin_turn(make_user_msg(&long_msg));
        // ~10_000 chars * 0.25 = 2_500 tokens, 阈值 3_000 * 0.8 = 2_400 → 应压缩
        assert!(sm.should_compact(3_000));
    }
}
