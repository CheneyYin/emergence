use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use crate::config::ConfigManager;
use crate::session::{SessionManager, SessionKey};
use crate::session::store::{JsonFileStore, SessionStore};
use crate::tools::ToolRegistry;
use crate::commands::{CommandRegistry, CommandContext, CommandOutput};
use crate::permissions::{PermissionStore, RiskLevel};
use crate::hooks::{HookRegistry, HookEvent, HookOutcome};
use crate::protocol::{Action, Event};
use crate::llm::{
    StreamEvent, ChatMessage, Role, Content,
    GenerationConfig, ToolDefinition, StopReason, ModelInfo,
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
        let home_dir = dirs_functions::home_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        let project_dir = std::env::current_dir()?;

        // 1. 加载配置
        let config = ConfigManager::load(
            home_dir.clone(),
            project_dir.clone(),
            self.cli_model.clone(),
        )?;

        // 2. 加载 Skill 注册表
        let skill_registry = crate::skills::SkillRegistry::load_default()
            .unwrap_or_else(|_| crate::skills::SkillRegistry::new());

        // 3. 创建 SessionStore（持久化）
        let store_dir = config.session_store_dir();
        let session_store: Box<dyn SessionStore> =
            Box::new(JsonFileStore::new(store_dir));

        // 3.5 创建/加载会话
        let session_id = self.cli_session.clone()
            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string());

        let session_manager = if let Some(ref cli_sess) = self.cli_session {
            let key = if cli_sess.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                SessionKey::Id(cli_sess.clone())
            } else {
                SessionKey::Alias(cli_sess.clone())
            };
            match session_store.load(&key).await {
                Ok(Some(session)) => SessionManager::load(session),
                _ => SessionManager::new(session_id),
            }
        } else {
            SessionManager::new(session_id)
        };

        // 4. 初始化工具注册表
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register(crate::tools::file::ReadTool);
        tool_registry.register(crate::tools::file::WriteTool);
        tool_registry.register(crate::tools::file::EditTool);
        tool_registry.register(crate::tools::search::GrepTool);
        tool_registry.register(crate::tools::search::GlobTool);
        tool_registry.register(crate::tools::bash::BashTool);
        tool_registry.register(crate::tools::web::WebFetchTool);
        tool_registry.register(crate::tools::web::WebSearchTool);

        // 5. 初始化命令注册表
        let mut command_registry = CommandRegistry::new();
        command_registry.register_all();

        // 6. 初始化 Provider 注册表
        let mut provider_registry = crate::llm::ProviderRegistry::new();
        for (name, provider_cfg) in &config.settings.providers {
            let models = vec![ModelInfo {
                id: provider_cfg.default_model.clone().unwrap_or_else(|| "default".into()),
                name: name.clone(),
                max_tokens: 128000,
            }];
            let adapter = crate::llm::openai::OpenAIAdapter::new(
                provider_cfg.base_url.clone(),
                provider_cfg.api_key.clone(),
                models,
            );
            provider_registry.register(name.clone(), Box::new(adapter));
        }

        // 6.5 加载 Hook 注册表
        let user_hooks_path = home_dir.join(".emergence").join("hooks.json");
        let project_hooks_path = project_dir.join(".emergence").join("hooks.json");
        let mut hook_registry = HookRegistry::load(&user_hooks_path)
            .unwrap_or_else(|_| HookRegistry::new());
        if let Ok(project_hr) = HookRegistry::load(&project_hooks_path) {
            hook_registry.merge(project_hr);
        }

        // 7. 创建通道
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // 8. 启动 AgentLoop
        let mut agent_loop = AgentLoop::new(
            config,
            session_manager,
            tool_registry,
            command_registry,
            skill_registry,
            hook_registry,
            provider_registry,
            Some(session_store),
            action_rx,
            event_tx,
        );

        // 9. 在后台运行 AgentLoop
        let agent_handle = tokio::spawn(async move {
            if let Err(e) = agent_loop.run().await {
                tracing::error!("AgentLoop 错误: {}", e);
            }
        });

        // 10. 启动 TUI
        crate::tui::run(action_tx.clone(), event_rx).await?;

        // 11. 清理
        let _ = action_tx.send(Action::Quit);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), agent_handle).await;

        Ok(())
    }
}

mod dirs_functions {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .ok()
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
    thinking_buffer: String,
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
            thinking_buffer: String::new(),
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
        // 欢迎消息后立即发送 AgentDone，否则 TUI 的 streaming 永久为 true，Enter 被拦截
        let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });

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
            let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
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
                        // 命令响应后必须发送 AgentDone，否则 TUI 的 streaming 不归零
                        let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
                    }
                    CommandOutput::Error { message } => {
                        let _ = self.event_tx.send(Event::Error { message });
                        let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
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
                        let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
                    }
                },
                Err(e) => {
                    let _ = self.event_tx.send(Event::Error { message: e.to_string() });
                    let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
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
        self.thinking_buffer.clear();

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
                self.thinking_buffer.push_str(&content);
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
        let cleaned = clean_tool_args(&args_json);
        let params: serde_json::Value = serde_json::from_str(&cleaned)
            .map_err(|e| anyhow::anyhow!("tool call args parse error: {}", e))?;

        // Push assistant tool_use 消息到会话（解析成功后再 push，确保 tool result 一定跟随）
        let thinking = std::mem::take(&mut self.thinking_buffer);
        let mut parts = Vec::new();
        if !thinking.is_empty() {
            parts.push(crate::llm::ContentPart::Text { text: thinking });
        }
        parts.push(crate::llm::ContentPart::ToolUse {
            id: tool_id.clone(),
            name: tool_name.clone(),
            input: params.clone(),
        });
        let tool_use_msg = ChatMessage {
            role: Role::Assistant,
            content: Content::Parts(parts),
            name: None,
            tool_call_id: None,
        };
        let _ = self.session.push(tool_use_msg);

        let risk = self.tool_registry.risk_level(&tool_name, &params)
            .unwrap_or(RiskLevel::System);

        // 执行工具（确保 tool result 一定跟随 tool_use，满足 API 要求）
        let result = match risk {
            RiskLevel::ReadOnly => {
                Box::pin(self.execute_and_feedback(tool_id.clone(), tool_name.clone(), params.clone())).await
            }
            RiskLevel::Write | RiskLevel::System => {
                self.hook_registry.dispatch(&HookEvent::PermissionRequested {
                    tool: tool_name.clone(),
                    risk,
                }).await;

                if self.permission_store.is_allowed(&tool_name, risk) {
                    Box::pin(self.execute_and_feedback(tool_id.clone(), tool_name.clone(), params.clone())).await
                } else {
                    self.state = AgentState::WaitingPermission {
                        tool_name: tool_name.clone(),
                        tool_id: tool_id.clone(),
                        params: params.clone(),
                        risk,
                    };
                    let _ = self.event_tx.send(Event::ToolRequest {
                        id: tool_id.clone(),
                        name: tool_name.clone(),
                        params: params.clone(),
                        risk,
                    });
                    return Ok(());
                }
            }
        };

        match result {
            Ok(()) => {}
            Err(e) => {
                // tool_use 消息已推入会话，必须跟一个 tool result
                let error_msg = ChatMessage {
                    role: Role::Tool,
                    content: Content::Text(format!("tool execution error: {}", e)),
                    name: Some(tool_name),
                    tool_call_id: Some(tool_id.clone()),
                };
                let _ = self.session.push(error_msg);
                let (messages, tools) = self.build_messages();
                self.retry_count = 0;
                let retry_fut = Box::pin(self.call_llm_with_retry(messages, &tools));
                if let Err(e2) = retry_fut.await {
                    let _ = self.event_tx.send(Event::Error { message: e2.to_string() });
                    self.state = AgentState::Idle;
                    let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
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
            let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
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
                    let _ = self.event_tx.send(Event::AgentDone { stop_reason: StopReason::EndTurn });
                }
            }
            _ => {
                self.state = AgentState::WaitingPermission { tool_name, tool_id, params, risk };
            }
        }

        Ok(())
    }
}

/// 清理累积的工具调用参数 JSON：从可能重复/重叠的字符串中提取有效 JSON
fn clean_tool_args(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return "{}".into();
    }
    // 尝试直接解析
    if serde_json::from_str::<serde_json::Value>(raw).is_ok() {
        return raw.to_string();
    }
    // 查找第一个完整的 JSON 对象 {...}（处理重复拼接）
    if let Some(end) = raw.find("}{") {
        let first = &raw[..=end];  // includes the first }
        if serde_json::from_str::<serde_json::Value>(first).is_ok() {
            return first.to_string();
        }
    }
    // 尝试取最后一个 {...}
    if let Some(last_start) = raw.rfind("}{") {
        let last = &raw[last_start + 1..];
        if serde_json::from_str::<serde_json::Value>(last).is_ok() {
            return last.to_string();
        }
    }
    // 回退：返回 {} 避免崩溃
    "{}".into()
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

    // Note: App::run() starts the full TUI + AgentLoop and requires a real terminal.
    // It cannot be tested in the standard test harness. Manual / integration testing required.

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

    /// Verifies that dirs_functions::home_dir() returns Some path based on HOME env.
    #[test]
    fn test_home_dir() {
        let home = dirs_functions::home_dir();
        assert!(home.is_some());
    }
}
