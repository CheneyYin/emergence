use tokio::sync::{mpsc, oneshot};
use crate::config::ConfigManager;
use crate::session::SessionManager;
use crate::tools::ToolRegistry;
use crate::commands::{CommandRegistry, CommandContext, CommandOutput};
use crate::permissions::{PermissionStore, RiskLevel};
use crate::hooks::{HookRegistry, HookEvent, HookOutcome};
use crate::protocol::{Action, Event};
use crate::llm::{
    StreamEvent, ChatMessage, Role, Content,
    GenerationConfig, ToolDefinition, StopReason,
};
use futures::StreamExt;

/// Agent 状态机
#[derive(Debug, Clone, PartialEq)]
enum AgentState {
    Idle,
    Processing,
    Streaming,
    WaitingPermission {
        tool_name: String,
        tool_id: String,
        params: serde_json::Value,
        risk: RiskLevel,
    },
}

pub struct App {
    cli_session: Option<String>,
    cli_model: Option<String>,
}

impl App {
    pub fn new(session: Option<String>, model: Option<String>) -> anyhow::Result<Self> {
        Ok(Self { cli_session: session, cli_model: model })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("App::run() — 将在任务 24 中实现完整集成");
        Ok(())
    }
}

pub struct AgentLoop {
    config: ConfigManager,
    session: SessionManager,
    tool_registry: ToolRegistry,
    command_registry: CommandRegistry,
    skill_registry: crate::skills::SkillRegistry,
    hook_registry: HookRegistry,
    permission_store: PermissionStore,
    provider_registry: crate::llm::ProviderRegistry,
    session_store: Option<Box<dyn crate::session::store::SessionStore>>,

    state: AgentState,
    model: String,
    system_prompt: String,
    stream_cancel: Option<oneshot::Sender<()>>,

    action_rx: mpsc::UnboundedReceiver<Action>,
    event_tx: mpsc::UnboundedSender<Event>,

    tool_call_buffer: Option<(String, String, String)>,
    retry_count: u32,
    max_retries: u32,
    should_exit: bool,
}

impl AgentLoop {
    pub fn new(
        config: ConfigManager,
        session: SessionManager,
        tool_registry: ToolRegistry,
        command_registry: CommandRegistry,
        skill_registry: crate::skills::SkillRegistry,
        hook_registry: HookRegistry,
        provider_registry: crate::llm::ProviderRegistry,
        session_store: Option<Box<dyn crate::session::store::SessionStore>>,
        action_rx: mpsc::UnboundedReceiver<Action>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Self {
        let model = config.settings.model.clone();

        let mut permission_store = PermissionStore::new();
        for tool_name in &config.settings.permissions.auto_approve {
            permission_store.approve_always(tool_name, RiskLevel::Write);
            permission_store.approve_always(tool_name, RiskLevel::System);
        }

        Self {
            config,
            session,
            tool_registry,
            command_registry,
            skill_registry,
            hook_registry,
            permission_store,
            provider_registry,
            session_store,
            state: AgentState::Idle,
            model,
            system_prompt: "You are a helpful coding assistant.".into(),
            stream_cancel: None,
            action_rx,
            event_tx,
            tool_call_buffer: None,
            retry_count: 0,
            max_retries: 3,
            should_exit: false,
        }
    }

    /// 主循环入口
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let _ = self.event_tx.send(Event::TextDelta {
            content: "emergence v0.1.0 已就绪。输入消息开始对话...\n".into(),
            finish_reason: None,
        });

        while let Some(action) = self.action_rx.recv().await {
            match &self.state {
                AgentState::WaitingPermission { .. } => {
                    match action {
                        Action::ApproveOnce | Action::ApproveAlways | Action::Deny => {
                            self.handle_permission_response(action).await?;
                        }
                        Action::Quit => {
                            self.save_and_exit().await?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                _ => {
                    match action {
                        Action::Submit(input) => {
                            self.handle_submit(input).await?;
                            if self.should_exit {
                                return Ok(());
                            }
                        }
                        Action::Cancel => {
                            self.cancel_stream();
                            if let Some(turn) = self.session.current_turn() {
                                if turn.status == crate::session::TurnStatus::InProgress {
                                    let _ = self.session.complete_turn();
                                }
                            }
                        }
                        Action::Quit => {
                            self.save_and_exit().await?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn save_and_exit(&mut self) -> anyhow::Result<()> {
        tracing::info!("保存会话并退出");
        if let Some(turn) = self.session.current_turn() {
            if turn.status == crate::session::TurnStatus::InProgress {
                let _ = self.session.complete_turn();
            }
        }
        if let Some(ref store) = self.session_store {
            if let Err(e) = store.save(self.session.session()).await {
                tracing::error!("会话保存失败: {}", e);
            }
        }
        Ok(())
    }

    fn cancel_stream(&mut self) {
        if let Some(cancel) = self.stream_cancel.take() {
            let _ = cancel.send(());
        }
    }

    async fn handle_submit(&mut self, input: String) -> anyhow::Result<()> {
        if input.starts_with('/') {
            return self.handle_command(input).await;
        }

        if self.state != AgentState::Idle {
            return Ok(());
        }

        self.state = AgentState::Processing;

        let _ = self.hook_registry.dispatch(&HookEvent::UserInput { text: input.clone() }).await;

        let user_msg = ChatMessage {
            role: Role::User,
            content: Content::Text(input),
            name: None, tool_call_id: None,
        };
        self.session.begin_turn(user_msg);

        let (messages, tools) = self.build_messages();

        self.hook_registry.dispatch(&HookEvent::PreLLMCall { messages: messages.clone() }).await;

        self.retry_count = 0;
        if let Err(e) = self.call_llm_with_retry(messages, &tools).await {
            let _ = self.event_tx.send(Event::Error { message: e.to_string() });
            self.state = AgentState::Idle;
        }

        Ok(())
    }

    fn build_messages(&self) -> (Vec<ChatMessage>, Vec<ToolDefinition>) {
        let tools = self.tool_registry.definitions();
        let available_skills_text = self.skill_registry.format_available_for_prompt();
        let active_contents: Vec<String> = self.session.active_skills()
            .iter()
            .filter_map(|name| self.skill_registry.load_full_content(name).ok())
            .collect();

        let messages = crate::session::context::ContextBuilder::build(
            self.session.session(),
            &self.system_prompt,
            &tools,
            &available_skills_text,
            &active_contents,
            self.config.agents_md_content.as_deref(),
        );
        (messages, tools)
    }

    async fn call_llm_with_retry(
        &mut self,
        messages: Vec<ChatMessage>,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<()> {
        loop {
            match self.call_llm_and_process(messages.clone(), tools).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("429") || err_msg.contains("rate") {
                        if self.retry_count < self.max_retries {
                            self.retry_count += 1;
                            let delay = 5u64;
                            let _ = self.event_tx.send(Event::Error {
                                message: format!("Rate limited, {}s 后重试 ({}/{})...", delay, self.retry_count, self.max_retries),
                            });
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            continue;
                        }
                    } else if err_msg.contains("500") || err_msg.contains("503") || err_msg.contains("server") {
                        if self.retry_count < self.max_retries {
                            self.retry_count += 1;
                            let delay = 2u64.pow(self.retry_count);
                            let _ = self.event_tx.send(Event::Error {
                                message: format!("服务器错误, {}s 后重试 ({}/{})...", delay, self.retry_count, self.max_retries),
                            });
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            continue;
                        }
                    } else if err_msg.contains("timeout") || err_msg.contains("timed out") {
                        if self.retry_count < 1 {
                            self.retry_count += 1;
                            let _ = self.event_tx.send(Event::Error {
                                message: "请求超时，正在重试...".into(),
                            });
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                    } else if err_msg.contains("401") || err_msg.contains("403") || err_msg.contains("auth") {
                        let _ = self.event_tx.send(Event::Error {
                            message: format!("认证错误: {}。请检查 API key。", err_msg),
                        });
                        return Err(e);
                    }
                    return Err(e);
                }
            }
        }
    }

    async fn handle_command(&mut self, input: String) -> anyhow::Result<()> {
        let mut should_quit = false;

        {
            let session_store_ref: Option<&dyn crate::session::store::SessionStore> =
                self.session_store.as_ref().map(|s| s.as_ref());

            let mut ctx = CommandContext {
                config: &mut self.config,
                session: &mut self.session,
                model: &mut self.model,
                should_quit: &mut should_quit,
                skill_registry: Some(&self.skill_registry),
                session_store: session_store_ref,
            };

            match self.command_registry.dispatch(&input, &mut ctx).await {
                Ok(output) => match output {
                    CommandOutput::Success { message } => {
                        let _ = self.event_tx.send(Event::TextDelta {
                            content: format!("{}\n", message), finish_reason: None,
                        });
                    }
                    CommandOutput::Error { message } => {
                        let _ = self.event_tx.send(Event::Error { message });
                    }
                    CommandOutput::Quit => {
                        should_quit = true;
                    }
                    CommandOutput::SwitchSession { session } => {
                        if let Some(ref store) = self.session_store {
                            let _ = store.save(self.session.session()).await;
                        }
                        self.session = SessionManager::load(session);
                        let _ = self.event_tx.send(Event::TextDelta {
                            content: format!("已切换到会话: {}\n", self.session.session().id),
                            finish_reason: None,
                        });
                    }
                },
                Err(e) => {
                    let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                }
            }
        }

        if should_quit {
            let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
            self.save_and_exit().await?;
            self.should_exit = true;
        }

        Ok(())
    }

    async fn call_llm_and_process(
        &mut self,
        messages: Vec<ChatMessage>,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<()> {
        let provider_name = self.model.split('/').next().unwrap_or("deepseek");
        let model_name = self.model.split('/').nth(1).unwrap_or("deepseek-v4-pro");

        let provider = self.provider_registry.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("未知 provider: {}", provider_name))?;

        let gen_config = self.config.generation_config();
        let gen_config = GenerationConfig {
            tools: if tools.is_empty() { None } else { Some(tools.to_vec()) },
            ..gen_config
        };

        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        self.stream_cancel = Some(cancel_tx);

        let mut stream = provider.chat(model_name, &messages, tools, &gen_config).await?;

        self.state = AgentState::Streaming;
        self.tool_call_buffer = None;

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    self.state = AgentState::Idle;
                    let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
                    return Ok(());
                }
                item = stream.next() => {
                    match item {
                        Some(Ok(event)) => {
                            if !self.process_stream_event(event, tools).await? {
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        self.stream_cancel = None;
        Ok(())
    }

    async fn process_stream_event(
        &mut self,
        event: StreamEvent,
        tools: &[ToolDefinition],
    ) -> anyhow::Result<bool> {
        match event {
            StreamEvent::TextDelta(content) => {
                let _ = self.event_tx.send(Event::TextDelta { content, finish_reason: None });
                Ok(true)
            }
            StreamEvent::ThinkingDelta(content) => {
                let _ = self.event_tx.send(Event::ThinkingDelta { content });
                Ok(true)
            }
            StreamEvent::ToolCallDelta { id, name, arguments_json_fragment } => {
                if let Some((_, _, ref mut args)) = &mut self.tool_call_buffer {
                    args.push_str(&arguments_json_fragment);
                } else {
                    self.tool_call_buffer = Some((id, name, arguments_json_fragment));
                }
                Ok(true)
            }
            StreamEvent::Finish { stop_reason, usage } => {
                let _ = self.event_tx.send(Event::StatusUpdate {
                    tokens: usage.input_tokens + usage.output_tokens,
                    model: self.model.clone(),
                });

                match stop_reason {
                    StopReason::ToolUse => {
                        if let Some((id, name, args)) = self.tool_call_buffer.take() {
                            self.handle_tool_use(id, name, args, tools).await?;
                        }
                    }
                    _ => {
                        self.hook_registry.dispatch(&HookEvent::PostLLMCall {
                            response: String::new(),
                            usage: usage.clone(),
                        }).await;

                        let _ = self.session.complete_turn();
                        if let Some(ref store) = self.session_store {
                            let _ = store.save(self.session.session()).await;
                        }

                        let threshold = self.config.settings.session.compaction_threshold_tokens;
                        if self.session.should_compact(threshold) {
                            self.session.compact(3);
                            let _ = self.event_tx.send(Event::Error {
                                message: format!("上下文已自动压缩。当前 token 用量: ~{}", self.session.estimated_tokens()),
                            });
                        }

                        self.state = AgentState::Idle;
                        let _ = self.event_tx.send(Event::AgentDone {
                            stop_reason: stop_reason.clone(),
                        });
                    }
                }

                Ok(false)
            }
        }
    }

    async fn handle_tool_use(
        &mut self,
        tool_id: String,
        tool_name: String,
        args_json: String,
        _tools: &[ToolDefinition],
    ) -> anyhow::Result<()> {
        let params: serde_json::Value = serde_json::from_str(&args_json)?;

        let risk = self.tool_registry.risk_level(&tool_name, &params)
            .unwrap_or(RiskLevel::System);

        match risk {
            RiskLevel::ReadOnly => {
                Box::pin(self.execute_and_feedback(tool_id, tool_name, params)).await?;
            }
            RiskLevel::Write | RiskLevel::System => {
                self.hook_registry.dispatch(&HookEvent::PermissionRequested {
                    tool: tool_name.clone(),
                    risk,
                }).await;

                if self.permission_store.is_allowed(&tool_name, risk) {
                    Box::pin(self.execute_and_feedback(tool_id, tool_name, params)).await?;
                } else {
                    self.state = AgentState::WaitingPermission {
                        tool_name: tool_name.clone(),
                        tool_id: tool_id.clone(),
                        params: params.clone(),
                        risk,
                    };
                    let _ = self.event_tx.send(Event::ToolRequest {
                        id: tool_id,
                        name: tool_name,
                        params,
                        risk,
                    });
                }
            }
        }

        Ok(())
    }

    async fn execute_and_feedback(
        &mut self,
        tool_id: String,
        tool_name: String,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        for outcome in self.hook_registry.dispatch(&HookEvent::PreToolExecute {
            tool: tool_name.clone(),
            params: params.clone(),
        }).await {
            if let HookOutcome::Abort { reason } = outcome {
                let abort_msg = format!("工具执行被 hook 中止: {}", reason);
                let _ = self.event_tx.send(Event::Error { message: abort_msg.clone() });
                let tool_msg = ChatMessage {
                    role: Role::Tool,
                    content: Content::Text(abort_msg),
                    name: Some(tool_name.clone()),
                    tool_call_id: Some(tool_id.clone()),
                };
                let _ = self.session.push(tool_msg);
                let (messages, tools) = self.build_messages();
                self.retry_count = 0;
                let _ = self.call_llm_with_retry(messages, &tools).await;
                return Ok(());
            }
        }

        let output = match self.tool_registry.execute(&tool_name, params.clone()).await {
            Ok(output) => {
                let _ = self.event_tx.send(Event::ToolResult {
                    id: tool_id.clone(),
                    name: tool_name.clone(),
                    params: params.clone(),
                    output: output.content.clone(),
                    metadata: output.metadata.clone(),
                });
                output
            }
            Err(e) => {
                let error_msg = format!("Tool 执行错误: {}", e);
                let _ = self.event_tx.send(Event::Error { message: error_msg.clone() });
                crate::tools::ToolOutput { content: error_msg, metadata: None }
            }
        };

        self.hook_registry.dispatch(&HookEvent::PostToolExecute {
            tool: tool_name.clone(),
            result: output.clone(),
        }).await;

        let tool_msg = ChatMessage {
            role: Role::Tool,
            content: Content::Text(output.content),
            name: Some(tool_name.clone()),
            tool_call_id: Some(tool_id.clone()),
        };
        let _ = self.session.push(tool_msg);

        let (messages, tools) = self.build_messages();
        self.retry_count = 0;
        // Box::pin to break recursive async chain
        let retry_fut = Box::pin(self.call_llm_with_retry(messages, &tools));
        if let Err(e) = retry_fut.await {
            let _ = self.event_tx.send(Event::Error { message: e.to_string() });
            self.state = AgentState::Idle;
        }

        Ok(())
    }

    async fn handle_permission_response(&mut self, action: Action) -> anyhow::Result<()> {
        let (tool_name, tool_id, params, risk) = match std::mem::replace(&mut self.state, AgentState::Processing) {
            AgentState::WaitingPermission { tool_name, tool_id, params, risk } => {
                (tool_name, tool_id, params, risk)
            }
            other => {
                self.state = other;
                return Ok(());
            }
        };

        match action {
            Action::ApproveOnce => {
                Box::pin(self.execute_and_feedback(tool_id, tool_name, params)).await?;
            }
            Action::ApproveAlways => {
                self.permission_store.approve_always(&tool_name, risk);
                Box::pin(self.execute_and_feedback(tool_id, tool_name, params)).await?;
            }
            Action::Deny => {
                let denied_msg = ChatMessage {
                    role: Role::Tool,
                    content: Content::Text(format!("denied by user: {}", tool_name)),
                    name: Some(tool_name.clone()),
                    tool_call_id: Some(tool_id.clone()),
                };
                let _ = self.session.push(denied_msg);

                let (messages, tools) = self.build_messages();
                self.retry_count = 0;
                let retry_fut = Box::pin(self.call_llm_with_retry(messages, &tools));
                if let Err(e) = retry_fut.await {
                    let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                    self.state = AgentState::Idle;
                }
            }
            _ => {
                self.state = AgentState::WaitingPermission { tool_name, tool_id, params, risk };
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::RiskLevel;
    use tempfile::TempDir;

    fn make_config() -> crate::config::ConfigManager {
        let home = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        crate::config::ConfigManager::load(
            home.path().to_path_buf(), project.path().to_path_buf(), None,
        ).unwrap()
    }

    /// Verifies that AgentState variants can be constructed and compared.
    #[test]
    fn test_agent_state_variants() {
        assert_eq!(AgentState::Idle, AgentState::Idle);
        assert_ne!(AgentState::Idle, AgentState::Processing);
        let waiting = AgentState::WaitingPermission {
            tool_name: "bash".into(), tool_id: "t1".into(),
            params: serde_json::json!({}), risk: RiskLevel::Write,
        };
        assert!(matches!(waiting, AgentState::WaitingPermission { .. }));
    }

    /// Verifies that App::new() succeeds with and without arguments.
    #[test]
    fn test_app_new() {
        assert!(App::new(None, None).is_ok());
        assert!(App::new(Some("sess-1".into()), Some("deepseek/v4".into())).is_ok());
    }

    /// Verifies that App::run() returns Ok for the placeholder implementation.
    #[tokio::test]
    async fn test_app_run_returns_ok() {
        let app = App::new(None, None).unwrap();
        assert!(app.run().await.is_ok());
    }

    /// Verifies that AgentLoop::new() sets model from config and initializes fields correctly.
    #[test]
    fn test_agent_loop_new() {
        let config = make_config();
        let session = crate::session::SessionManager::new("test".into());
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();

        let agent = AgentLoop::new(
            config,
            session,
            ToolRegistry::new(),
            CommandRegistry::new(),
            crate::skills::SkillRegistry::new(),
            HookRegistry::new(),
            crate::llm::ProviderRegistry::new(),
            None,
            action_rx,
            event_tx,
        );

        assert_eq!(agent.model, "deepseek/deepseek-v4-pro");
        assert_eq!(agent.state, AgentState::Idle);
        assert_eq!(agent.retry_count, 0);
        assert_eq!(agent.max_retries, 3);
        assert!(!agent.should_exit);
        // action_tx is kept to keep channel open
        drop(action_tx);
    }

    /// Verifies that auto_approve tools from config are pre-approved in permission_store.
    #[test]
    fn test_agent_loop_auto_approve_permissions() {
        let home = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let emergence_dir = home.path().join(".emergence");
        std::fs::create_dir_all(&emergence_dir).unwrap();
        std::fs::write(
            emergence_dir.join("settings.json"),
            r#"{"permissions": {"auto_approve": ["read", "grep"]}}"#,
        ).unwrap();

        let config = crate::config::ConfigManager::load(
            home.path().to_path_buf(), project.path().to_path_buf(), None,
        ).unwrap();
        let session = crate::session::SessionManager::new("test".into());
        let (_action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();

        let agent = AgentLoop::new(
            config,
            session,
            ToolRegistry::new(),
            CommandRegistry::new(),
            crate::skills::SkillRegistry::new(),
            HookRegistry::new(),
            crate::llm::ProviderRegistry::new(),
            None,
            action_rx,
            event_tx,
        );

        assert!(agent.permission_store.is_allowed("read", RiskLevel::Write));
        assert!(agent.permission_store.is_allowed("grep", RiskLevel::System));
        assert!(!agent.permission_store.is_allowed("bash", RiskLevel::Write));
    }
}
