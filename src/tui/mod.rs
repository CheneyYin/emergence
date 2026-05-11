use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use tokio::sync::mpsc;
use tui_textarea::TextArea;

use crate::protocol::{Action, Event as AppEvent};

pub mod markdown;
pub mod popups;
pub mod themes;
pub mod widgets;

/// TUI 状态
pub struct TuiState {
    pub turns: Vec<Turn>,
    pub status_text: String,
    pub textarea: TextArea<'static>,
    pub show_permission_dialog: Option<PermissionDialogState>,
    pub streaming: bool,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub pending_input: String,
    pub scroll_y: usize,
    pub follow_bottom: bool,
}

/// A conversation turn: one user message + the assistant response.
#[derive(Debug, Clone)]
pub struct Turn {
    pub user: UserPart,
    pub assistant: AssistantPart,
    pub status: TurnStatus,
}

#[derive(Debug, Clone)]
pub struct UserPart {
    pub timestamp: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct AssistantPart {
    pub timestamp: String,
    pub content: String,
    pub thinking_tokens: Option<u32>,
    pub duration: Option<String>,
    pub tokens: Option<u32>,
    pub tool_blocks: Vec<ToolBlock>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolBlock {
    pub tool: String,
    pub summary: String,
    pub result: Option<String>,
    pub duration: Option<String>,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TurnStatus {
    InProgress,
    Complete,
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
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState {
        turns: Vec::new(),
        status_text: "emergence · 启动中 · ✓ ready".into(),
        textarea: TextArea::default(),
        show_permission_dialog: None,
        streaming: false,
        input_history: load_input_history(),
        history_index: None,
        pending_input: String::new(),
        scroll_y: 0,
        follow_bottom: true,
    };

    let res = app_loop(&mut terminal, &mut state, &action_tx, &mut event_rx).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
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

        // 非阻塞排空所有 agent 事件
        loop {
            match event_rx.try_recv() {
                Ok(event) => handle_app_event(event, state)?,
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => return Ok(()),
            }
        }

        // 始终检查键盘/鼠标输入（不会被事件洪水饿死）
        if event::poll(std::time::Duration::from_millis(10))? {
            let crossterm_event = event::read()?;
            match crossterm_event {
                CEvent::Key(key) => {
                    if state.show_permission_dialog.is_some() {
                        handle_permission_key(key, state, action_tx)?;
                    } else {
                        handle_input_key(key, state, action_tx).await?;
                    }
                }
                CEvent::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => {
                        state.follow_bottom = false;
                        state.scroll_y = state.scroll_y.saturating_add(3);
                    }
                    MouseEventKind::ScrollUp => {
                        state.follow_bottom = false;
                        state.scroll_y = state.scroll_y.saturating_sub(3);
                    }
                    _ => {}
                },
                CEvent::Resize(_, _) => {}
                _ => {}
            }
        }
    }
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
        KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
        | KeyEvent {
            code: KeyCode::Enter,
            modifiers: _,
            ..
        } => {
            if state.streaming || state.show_permission_dialog.is_some() {
                return Ok(());
            }
            let lines: Vec<String> = state.textarea.lines().to_vec();
            let input = lines.join("\n");
            if !input.trim().is_empty() {
                state.textarea = TextArea::default();
                if state
                    .input_history
                    .last()
                    .map(|s| s != &input)
                    .unwrap_or(true)
                {
                    state.input_history.push(input.clone());
                    if state.input_history.len() > 1000 {
                        state.input_history.remove(0);
                    }
                }
                state.history_index = None;
                state.pending_input.clear();
                state.turns.push(Turn {
                    user: UserPart {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        content: input.clone(),
                    },
                    assistant: AssistantPart {
                        timestamp: String::new(),
                        content: String::new(),
                        thinking_tokens: None,
                        duration: None,
                        tokens: None,
                        tool_blocks: Vec::new(),
                        error: None,
                    },
                    status: TurnStatus::InProgress,
                });
                action_tx.send(Action::Submit(input))?;
                state.status_text = "emergence · 处理中 · ⏳ streaming".into();
            }
        }
        KeyEvent {
            code: KeyCode::Up,
            modifiers: _,
            ..
        } => {
            if !state.input_history.is_empty() {
                if state.history_index.is_none() {
                    state.pending_input = state.textarea.lines().join("\n");
                    state.history_index = Some(state.input_history.len() - 1);
                } else if let Some(idx) = state.history_index {
                    if idx > 0 {
                        state.history_index = Some(idx - 1);
                    }
                }
                if let Some(idx) = state.history_index {
                    let content = state.input_history[idx].clone();
                    state.textarea =
                        TextArea::new(content.lines().map(String::from).collect::<Vec<_>>());
                }
            }
        }
        KeyEvent {
            code: KeyCode::Down,
            modifiers: _,
            ..
        } => {
            if let Some(idx) = state.history_index {
                let content = if idx + 1 < state.input_history.len() {
                    state.history_index = Some(idx + 1);
                    state.input_history[idx + 1].clone()
                } else {
                    state.history_index = None;
                    std::mem::take(&mut state.pending_input)
                };
                state.textarea =
                    TextArea::new(content.lines().map(String::from).collect::<Vec<_>>());
            }
        }
        KeyEvent {
            code: KeyCode::Esc,
            modifiers: _,
            ..
        } => {
            state.textarea = TextArea::default();
            state.history_index = None;
            state.pending_input.clear();
        }
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => {
            if state.streaming {
                action_tx.send(Action::Cancel)?;
            }
        }
        _ => {
            let modified = state.textarea.input(key);
            if modified {
                state.history_index = None;
                state.pending_input.clear();
            }
        }
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
    use crate::llm::StopReason;
    use crate::permissions::RiskLevel;

    /// Verifies that TuiState can be constructed with default values.
    #[test]
    fn test_tui_state_construction() {
        let state = TuiState {
            turns: vec![],
            status_text: "ready".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        assert!(state.turns.is_empty());
        assert_eq!(state.status_text, "ready");
    }

    /// Verifies that Turn and its sub-structs can be constructed and debugged.
    #[test]
    fn test_turn_structure() {
        let turn = Turn {
            user: UserPart {
                timestamp: "12:00".into(),
                content: "hi".into(),
            },
            assistant: AssistantPart {
                timestamp: "12:01".into(),
                content: "Hello!".into(),
                thinking_tokens: None,
                duration: None,
                tokens: None,
                tool_blocks: vec![],
                error: None,
            },
            status: TurnStatus::Complete,
        };
        assert!(format!("{:?}", turn).contains("Turn"));
        assert_eq!(turn.user.content, "hi");
    }

    /// Verifies that handle_app_event processes TextDelta by updating the last turn's assistant content.
    #[test]
    fn test_handle_text_delta() {
        let base_turn = Turn {
            user: UserPart {
                timestamp: "".into(),
                content: "hi".into(),
            },
            assistant: AssistantPart {
                timestamp: String::new(),
                content: String::new(),
                thinking_tokens: None,
                duration: None,
                tokens: None,
                tool_blocks: vec![],
                error: None,
            },
            status: TurnStatus::InProgress,
        };
        let mut state = TuiState {
            turns: vec![base_turn],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::TextDelta {
            content: "Hello".into(),
            finish_reason: None,
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.streaming);
        assert_eq!(state.turns[0].assistant.content, "Hello");
    }

    /// Verifies that handle_app_event processes ThinkingDelta by appending to thinking field.
    #[test]
    fn test_handle_thinking_delta() {
        let base_turn = Turn {
            user: UserPart {
                timestamp: "".into(),
                content: "hi".into(),
            },
            assistant: AssistantPart {
                timestamp: String::new(),
                content: String::new(),
                thinking_tokens: None,
                duration: None,
                tokens: None,
                tool_blocks: vec![],
                error: None,
            },
            status: TurnStatus::InProgress,
        };
        let mut state = TuiState {
            turns: vec![base_turn],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::ThinkingDelta {
            content: "thinking...".into(),
        };
        handle_app_event(event, &mut state).unwrap();
        assert_eq!(state.turns[0].assistant.thinking_tokens, Some(1));
    }

    /// Verifies that handle_app_event processes ToolRequest by setting the permission dialog.
    #[test]
    fn test_handle_tool_request() {
        let mut state = TuiState {
            turns: vec![],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::ToolRequest {
            id: "t1".into(),
            name: "bash".into(),
            params: serde_json::json!({"cmd": "ls"}),
            risk: RiskLevel::Write,
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.show_permission_dialog.is_some());
        let dialog = state.show_permission_dialog.unwrap();
        assert_eq!(dialog.tool_name, "bash");
        assert_eq!(dialog.risk, RiskLevel::Write);
    }

    /// Verifies that handle_app_event processes AgentDone by stopping streaming and updating status.
    #[test]
    fn test_handle_agent_done() {
        let mut state = TuiState {
            turns: vec![],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: true,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::AgentDone {
            stop_reason: StopReason::EndTurn,
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(!state.streaming);
        assert!(state.status_text.contains("ready"));
    }

    /// Verifies that handle_app_event processes Error by appending an Error message.
    #[test]
    fn test_handle_error() {
        let base_turn = Turn {
            user: UserPart {
                timestamp: "".into(),
                content: "".into(),
            },
            assistant: AssistantPart {
                timestamp: String::new(),
                content: String::new(),
                thinking_tokens: None,
                duration: None,
                tokens: None,
                tool_blocks: vec![],
                error: None,
            },
            status: TurnStatus::InProgress,
        };
        let mut state = TuiState {
            turns: vec![base_turn],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::Error {
            message: "failed".into(),
        };
        handle_app_event(event, &mut state).unwrap();
        assert_eq!(state.turns[0].assistant.error.as_deref(), Some("failed"));
    }

    /// Verifies that handle_permission_key with 'a' sends ApproveOnce and clears dialog.
    #[test]
    fn test_permission_key_approve_once() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut state = TuiState {
            turns: vec![],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: Some(PermissionDialogState {
                tool_name: "bash".into(),
                risk: RiskLevel::Write,
                params: serde_json::json!({}),
                tool_id: "t1".into(),
            }),
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<crate::protocol::Action>();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        handle_permission_key(key, &mut state, &tx).unwrap();
        assert!(state.show_permission_dialog.is_none());
    }

    /// Verifies that handle_app_event processes ToolResult by pushing ToolCall and ToolResult messages.
    #[test]
    fn test_handle_tool_result() {
        let base_turn = Turn {
            user: UserPart {
                timestamp: "".into(),
                content: "hi".into(),
            },
            assistant: AssistantPart {
                timestamp: String::new(),
                content: String::new(),
                thinking_tokens: None,
                duration: None,
                tokens: None,
                tool_blocks: vec![],
                error: None,
            },
            status: TurnStatus::InProgress,
        };
        let mut state = TuiState {
            turns: vec![base_turn],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::ToolResult {
            id: "t1".into(),
            name: "read".into(),
            params: serde_json::json!({"file": "x"}),
            output: "content".into(),
            metadata: None,
        };
        handle_app_event(event, &mut state).unwrap();
        assert_eq!(state.turns.len(), 1);
        assert_eq!(state.turns[0].assistant.tool_blocks.len(), 1);
        assert_eq!(state.turns[0].assistant.tool_blocks[0].tool, "read");
    }

    /// Verifies that handle_app_event processes StatusUpdate by setting the status text.
    #[test]
    fn test_handle_status_update() {
        let mut state = TuiState {
            turns: vec![],
            status_text: "".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };
        let event = AppEvent::StatusUpdate {
            tokens: 42,
            model: "gpt-4".into(),
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.status_text.contains("gpt-4"));
        assert!(state.status_text.contains("42"));
    }

    /// Verifies that rendering places the cursor in the input box after the prompt text.
    #[test]
    fn test_render_input_textarea_content() {
        let mut textarea = TextArea::new(vec!["hello".to_string()]);
        textarea.move_cursor(tui_textarea::CursorMove::End);
        let mut state = TuiState {
            turns: vec![],
            status_text: "ready".into(),
            textarea,
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| widgets::render(f, &mut state)).unwrap();

        let buffer = terminal.backend().buffer();
        // Textarea text "hello" should appear in the input area (layout[2], inner y=22).
        // prompt area is columns 0-1, textarea starts at column 2.
        let input_row = &buffer[(2, 22)];
        assert_eq!(input_row.symbol(), "h");
        // No terminal cursor set (TextArea renders its own REVERSED visual cursor)
    }

    /// Verifies the textarea widget renders even when empty.
    #[test]
    fn test_render_input_textarea_empty() {
        let mut state = TuiState {
            turns: vec![],
            status_text: "ready".into(),
            textarea: TextArea::default(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
        };

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| widgets::render(f, &mut state)).unwrap();

        // Should not panic
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    // ── tool_summary tests ──

    /// Verifies bash tool uses description field from params.
    #[test]
    fn test_tool_summary_bash_description() {
        let summary = tool_summary("bash", &serde_json::json!({"description": "install deps"}));
        assert_eq!(summary, "install deps");
    }

    /// Verifies bash falls back to command when description is missing.
    #[test]
    fn test_tool_summary_bash_command_fallback() {
        let summary = tool_summary("bash", &serde_json::json!({"command": "npm install"}));
        assert_eq!(summary, "npm install");
    }

    /// Verifies read tool shows basename of file_path.
    #[test]
    fn test_tool_summary_read_basename() {
        let summary = tool_summary(
            "read",
            &serde_json::json!({"file_path": "/home/user/src/app.rs"}),
        );
        assert_eq!(summary, "app.rs");
    }

    /// Verifies grep tool shows pattern truncated.
    #[test]
    fn test_tool_summary_grep_pattern() {
        let summary = tool_summary("grep", &serde_json::json!({"pattern": "async fn"}));
        assert_eq!(summary, "async fn");
    }

    /// Verifies long strings are truncated with ellipsis.
    #[test]
    fn test_tool_summary_truncation() {
        let long = "a".repeat(100);
        let summary = tool_summary("bash", &serde_json::json!({"command": &long}));
        assert_eq!(summary.len(), 63); // 60 chars + '…' (3 bytes)
        assert!(summary.ends_with('…'));
    }

    /// Verifies unknown tool returns a placeholder.
    #[test]
    fn test_tool_summary_unknown_tool() {
        let summary = tool_summary("unknown", &serde_json::json!({"key": "val"}));
        assert_eq!(summary, "val");
    }
}

/// Generate a compact one-line summary for a tool call (Claude Code style).
fn tool_summary(tool: &str, params: &serde_json::Value) -> String {
    let trunc = |s: &str, max: usize| -> String {
        if s.len() > max {
            format!("{}…", &s[..max])
        } else {
            s.to_string()
        }
    };

    match tool {
        "bash" => params
            .get("description")
            .and_then(|v| v.as_str())
            .or_else(|| params.get("command").and_then(|v| v.as_str()))
            .map(|s| trunc(s, 60))
            .unwrap_or_else(|| "…".into()),
        "read" | "write" | "edit" => params
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
                    .to_string()
            })
            .unwrap_or_else(|| "…".into()),
        "grep" | "glob" => params
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| trunc(s, 60))
            .unwrap_or_else(|| "…".into()),
        "web_search" => params
            .get("query")
            .and_then(|v| v.as_str())
            .map(|s| trunc(s, 60))
            .unwrap_or_else(|| "…".into()),
        "web_fetch" => params
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| trunc(s, 60))
            .unwrap_or_else(|| "…".into()),
        _ => {
            // Generic: show first string value or "…"
            if let Some(obj) = params.as_object() {
                obj.values()
                    .filter_map(|v| v.as_str())
                    .next()
                    .map(|s| trunc(s, 40))
                    .unwrap_or_else(|| "…".into())
            } else {
                "…".into()
            }
        }
    }
}

fn handle_app_event(event: AppEvent, state: &mut TuiState) -> anyhow::Result<()> {
    match event {
        AppEvent::TextDelta {
            content,
            finish_reason: _,
        } => {
            state.streaming = true;
            if let Some(turn) = state.turns.last_mut() {
                if turn.assistant.timestamp.is_empty() {
                    turn.assistant.timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                }
                turn.assistant.content.push_str(&content);
            }
        }
        AppEvent::ThinkingDelta { content: _ } => {
            if let Some(turn) = state.turns.last_mut() {
                let count = turn.assistant.thinking_tokens.unwrap_or(0);
                turn.assistant.thinking_tokens = Some(count + 1);
            }
        }
        AppEvent::ToolRequest {
            id,
            name,
            params,
            risk,
        } => {
            state.show_permission_dialog = Some(PermissionDialogState {
                tool_name: name,
                risk,
                params,
                tool_id: id,
            });
        }
        AppEvent::ToolResult {
            id: _,
            name,
            params,
            output,
            metadata: _,
        } => {
            if let Some(turn) = state.turns.last_mut() {
                turn.assistant.tool_blocks.push(ToolBlock {
                    summary: tool_summary(&name, &params),
                    tool: name,
                    result: Some(output),
                    duration: None,
                    ok: true,
                });
            }
        }
        AppEvent::StatusUpdate { tokens, model } => {
            state.status_text = format!("emergence · {} · {} tokens · ⏳ streaming", model, tokens);
            if let Some(turn) = state.turns.last_mut() {
                turn.assistant.tokens = Some(tokens);
            }
        }
        AppEvent::AgentDone { stop_reason } => {
            state.streaming = false;
            state.follow_bottom = true;
            if let Some(turn) = state.turns.last_mut() {
                turn.status = TurnStatus::Complete;
            }
            state.status_text = format!("emergence · ✓ ready ({:?})", stop_reason);
        }
        AppEvent::Error { message } => {
            if let Some(turn) = state.turns.last_mut() {
                turn.assistant.error = Some(message);
            }
        }
    }
    Ok(())
}
