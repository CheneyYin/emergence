use std::io;
use ratatui::prelude::*;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers, KeyEvent},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use tokio::sync::mpsc;

use crate::protocol::{Action, Event as AppEvent};

pub mod themes;
pub mod widgets;
pub mod popups;

/// TUI 状态
pub struct TuiState {
    pub messages: Vec<RenderedMessage>,
    pub status_text: String,
    pub input_buffer: String,
    pub show_permission_dialog: Option<PermissionDialogState>,
    pub streaming: bool,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub pending_input: String,
}

#[derive(Debug, Clone)]
pub enum RenderedMessage {
    User { timestamp: String, content: String },
    Assistant { timestamp: String, content: String, thinking: Option<String>, duration: Option<String>, tokens: Option<u32> },
    ToolCall { tool: String, params: String, duration: Option<String> },
    ToolResult { output: String },
    Thinking { content: String },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct PermissionDialogState {
    pub tool_name: String,
    pub risk: crate::permissions::RiskLevel,
    pub params: serde_json::Value,
    pub tool_id: String,
}

/// 启动 TUI 主循环
pub async fn run(
    action_tx: mpsc::UnboundedSender<Action>,
    mut event_rx: mpsc::UnboundedReceiver<AppEvent>,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState {
        messages: Vec::new(),
        status_text: "emergence · 启动中 · ✓ ready".into(),
        input_buffer: String::new(),
        show_permission_dialog: None,
        streaming: false,
        input_history: load_input_history(),
        history_index: None,
        pending_input: String::new(),
    };

    let res = app_loop(&mut terminal, &mut state, &action_tx, &mut event_rx).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

async fn app_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
    action_tx: &mpsc::UnboundedSender<Action>,
    event_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| {
            widgets::render(f, state);
            if let Some(ref dialog) = state.show_permission_dialog {
                popups::render_permission_dialog(f, dialog);
            }
        })?;

        // 周期性检查键盘输入 + agent 事件：
        // sleep 提供轮询间隔，避免 spawn_blocking 不可取消导致进程挂起
        tokio::select! {
            app_event = event_rx.recv() => {
                match app_event {
                    Some(event) => handle_app_event(event, state)?,
                    None => break,
                }
            }

            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                // 非阻塞检查是否有输入事件
                if event::poll(std::time::Duration::ZERO)? {
                    let crossterm_event = event::read()?;
                    match crossterm_event {
                        CEvent::Key(key) => {
                            if state.show_permission_dialog.is_some() {
                                handle_permission_key(key, state, action_tx)?;
                            } else {
                                handle_input_key(key, state, action_tx).await?;
                            }
                        }
                        CEvent::Resize(_, _) => { /* 自动重绘 */ }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_permission_key(
    key: KeyEvent,
    state: &mut TuiState,
    action_tx: &mpsc::UnboundedSender<Action>,
) -> anyhow::Result<()> {
    match key.code {
        KeyCode::Char('a') | KeyCode::Char('A') => {
            state.show_permission_dialog = None;
            action_tx.send(Action::ApproveOnce)?;
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            state.show_permission_dialog = None;
            action_tx.send(Action::ApproveAlways)?;
        }
        KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Esc => {
            state.show_permission_dialog = None;
            action_tx.send(Action::Deny)?;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_input_key(
    key: KeyEvent,
    state: &mut TuiState,
    action_tx: &mpsc::UnboundedSender<Action>,
) -> anyhow::Result<()> {
    match key {
        KeyEvent { code: KeyCode::Char('s'), modifiers: KeyModifiers::CONTROL, .. } |
        KeyEvent { code: KeyCode::Enter, modifiers: _, .. } => {
            // 不要在 streaming 或等待许可时吞掉输入 — AgentLoop 会静默丢弃
            if state.streaming || state.show_permission_dialog.is_some() {
                return Ok(());
            }
            if !state.input_buffer.trim().is_empty() {
                let input = std::mem::take(&mut state.input_buffer);
                if state.input_history.last().map(|s| s != &input).unwrap_or(true) {
                    state.input_history.push(input.clone());
                    if state.input_history.len() > 1000 {
                        state.input_history.remove(0);
                    }
                }
                state.history_index = None;
                state.pending_input.clear();
                // 用户消息立即回显在对话区域
                state.messages.push(RenderedMessage::User {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    content: input.clone(),
                });
                action_tx.send(Action::Submit(input))?;
                state.status_text = "emergence · 处理中 · ⏳ streaming".into();
            }
        }
        KeyEvent { code: KeyCode::Up, modifiers: _, .. } => {
            if !state.input_history.is_empty() {
                if state.history_index.is_none() {
                    state.pending_input = std::mem::take(&mut state.input_buffer);
                    state.history_index = Some(state.input_history.len() - 1);
                } else if let Some(idx) = state.history_index {
                    if idx > 0 {
                        state.history_index = Some(idx - 1);
                    }
                }
                if let Some(idx) = state.history_index {
                    state.input_buffer = state.input_history[idx].clone();
                }
            }
        }
        KeyEvent { code: KeyCode::Down, modifiers: _, .. } => {
            if let Some(idx) = state.history_index {
                if idx + 1 < state.input_history.len() {
                    state.history_index = Some(idx + 1);
                    state.input_buffer = state.input_history[idx + 1].clone();
                } else {
                    state.history_index = None;
                    state.input_buffer = std::mem::take(&mut state.pending_input);
                }
            }
        }
        KeyEvent { code: KeyCode::Esc, modifiers: _, .. } => {
            state.input_buffer.clear();
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. } => {
            if state.streaming {
                action_tx.send(Action::Cancel)?;
            }
        }
        KeyEvent { code: KeyCode::Char(c), modifiers: _, .. } => {
            state.input_buffer.push(c);
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent { code: KeyCode::Backspace, modifiers: _, .. } => {
            state.input_buffer.pop();
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent { code: KeyCode::Tab, modifiers: _, .. } => {
            state.input_buffer.push_str("    ");
        }
        _ => {}
    }
    Ok(())
}

fn load_input_history() -> Vec<String> {
    Vec::new()
}

fn _save_input_history(history: &[String]) {
    let _ = history;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::permissions::RiskLevel;
    use crate::llm::StopReason;

    /// Verifies that TuiState can be constructed with default values.
    #[test]
    fn test_tui_state_construction() {
        let state = TuiState {
            messages: vec![],
            status_text: "ready".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
        };
        assert!(state.messages.is_empty());
        assert_eq!(state.status_text, "ready");
    }

    /// Verifies that RenderedMessage variants can be constructed and debugged.
    #[test]
    fn test_rendered_message_variants() {
        let user = RenderedMessage::User { timestamp: "12:00".into(), content: "hi".into() };
        assert!(format!("{:?}", user).contains("User"));

        let err = RenderedMessage::Error { message: "oops".into() };
        assert!(format!("{:?}", err).contains("Error"));
    }

    /// Verifies that handle_app_event processes TextDelta by updating the last assistant message or creating one.
    #[test]
    fn test_handle_text_delta() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::TextDelta { content: "Hello".into(), finish_reason: None };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.streaming);
        assert_eq!(state.messages.len(), 1);
    }

    /// Verifies that handle_app_event processes ThinkingDelta by appending a Thinking message.
    #[test]
    fn test_handle_thinking_delta() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::ThinkingDelta { content: "thinking...".into() };
        handle_app_event(event, &mut state).unwrap();
        assert!(matches!(state.messages.last(), Some(RenderedMessage::Thinking { .. })));
    }

    /// Verifies that handle_app_event processes ToolRequest by setting the permission dialog.
    #[test]
    fn test_handle_tool_request() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::ToolRequest { id: "t1".into(), name: "bash".into(), params: serde_json::json!({"cmd": "ls"}), risk: RiskLevel::Write };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.show_permission_dialog.is_some());
        let dialog = state.show_permission_dialog.unwrap();
        assert_eq!(dialog.tool_name, "bash");
        assert_eq!(dialog.risk, RiskLevel::Write);
    }

    /// Verifies that handle_app_event processes AgentDone by stopping streaming and updating status.
    #[test]
    fn test_handle_agent_done() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: true, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::AgentDone { stop_reason: StopReason::EndTurn };
        handle_app_event(event, &mut state).unwrap();
        assert!(!state.streaming);
        assert!(state.status_text.contains("ready"));
    }

    /// Verifies that handle_app_event processes Error by appending an Error message.
    #[test]
    fn test_handle_error() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::Error { message: "failed".into() };
        handle_app_event(event, &mut state).unwrap();
        assert!(matches!(state.messages.last(), Some(RenderedMessage::Error { .. })));
    }

    /// Verifies that handle_permission_key with 'a' sends ApproveOnce and clears dialog.
    #[test]
    fn test_permission_key_approve_once() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: Some(PermissionDialogState { tool_name: "bash".into(), risk: RiskLevel::Write, params: serde_json::json!({}), tool_id: "t1".into() }), streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<crate::protocol::Action>();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        handle_permission_key(key, &mut state, &tx).unwrap();
        assert!(state.show_permission_dialog.is_none());
    }

    /// Verifies that handle_app_event processes ToolResult by pushing ToolCall and ToolResult messages.
    #[test]
    fn test_handle_tool_result() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::ToolResult { id: "t1".into(), name: "read".into(), params: serde_json::json!({"file": "x"}), output: "content".into(), metadata: None };
        handle_app_event(event, &mut state).unwrap();
        assert_eq!(state.messages.len(), 2);
        assert!(matches!(state.messages[0], RenderedMessage::ToolCall { .. }));
        assert!(matches!(state.messages[1], RenderedMessage::ToolResult { .. }));
    }

    /// Verifies that handle_app_event processes StatusUpdate by setting the status text.
    #[test]
    fn test_handle_status_update() {
        let mut state = TuiState { messages: vec![], status_text: "".into(), input_buffer: String::new(), show_permission_dialog: None, streaming: false, input_history: vec![], history_index: None, pending_input: String::new() };
        let event = AppEvent::StatusUpdate { tokens: 42, model: "gpt-4".into() };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.status_text.contains("gpt-4"));
        assert!(state.status_text.contains("42"));
    }
}

fn handle_app_event(event: AppEvent, state: &mut TuiState) -> anyhow::Result<()> {
    match event {
        AppEvent::TextDelta { content, finish_reason: _ } => {
            state.streaming = true;
            if let Some(RenderedMessage::Assistant { content: ref mut existing, .. }) = state.messages.last_mut() {
                existing.push_str(&content);
            } else {
                state.messages.push(RenderedMessage::Assistant {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    content,
                    thinking: None,
                    duration: None,
                    tokens: None,
                });
            }
        }
        AppEvent::ThinkingDelta { content } => {
            // 累积到上一条 thinking 消息，避免每个 token 一行
            if let Some(RenderedMessage::Thinking { content: ref mut existing }) = state.messages.last_mut() {
                existing.push_str(&content);
            } else {
                state.messages.push(RenderedMessage::Thinking { content });
            }
        }
        AppEvent::ToolRequest { id, name, params, risk } => {
            state.show_permission_dialog = Some(PermissionDialogState {
                tool_name: name,
                risk,
                params,
                tool_id: id,
            });
        }
        AppEvent::ToolResult { id: _, name, params, output, metadata: _ } => {
            state.messages.push(RenderedMessage::ToolCall {
                tool: name,
                params: serde_json::to_string_pretty(&params).unwrap_or_default(),
                duration: None,
            });
            state.messages.push(RenderedMessage::ToolResult { output });
        }
        AppEvent::StatusUpdate { tokens, model } => {
            state.status_text = format!("emergence · {} · {} tokens · ⏳ streaming", model, tokens);
        }
        AppEvent::AgentDone { stop_reason } => {
            state.streaming = false;
            state.status_text = format!("emergence · ✓ ready ({:?})", stop_reason);
        }
        AppEvent::Error { message } => {
            state.messages.push(RenderedMessage::Error { message });
        }
    }
    Ok(())
}
