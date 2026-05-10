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

use crate::protocol::{Action, Event as AppEvent};

pub mod markdown;
pub mod popups;
pub mod themes;
pub mod widgets;

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
    pub scroll_y: usize,
    pub follow_bottom: bool,
    pub cursor_pos: usize,
}

#[derive(Debug, Clone)]
pub enum RenderedMessage {
    User {
        timestamp: String,
        content: String,
    },
    Assistant {
        timestamp: String,
        content: String,
        thinking: Option<String>,
        duration: Option<String>,
        tokens: Option<u32>,
    },
    ToolCall {
        tool: String,
        params: String,
        duration: Option<String>,
    },
    ToolResult {
        output: String,
    },
    Thinking {
        content: String,
    },
    Error {
        message: String,
    },
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
        messages: Vec::new(),
        status_text: "emergence · 启动中 · ✓ ready".into(),
        input_buffer: String::new(),
        show_permission_dialog: None,
        streaming: false,
        input_history: load_input_history(),
        history_index: None,
        pending_input: String::new(),
        scroll_y: 0,
        follow_bottom: true,
        cursor_pos: 0,
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

        // 周期性检查键盘输入 + agent 事件：
        // sleep 提供轮询间隔，避免 spawn_blocking 不可取消导致进程挂起
        tokio::select! {
            app_event = event_rx.recv() => {
                match app_event {
                    Some(event) => handle_app_event(event, state)?,
                    None => break,
                }
            }

            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
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
            if !state.input_buffer.trim().is_empty() {
                let input = std::mem::take(&mut state.input_buffer);
                state.cursor_pos = 0;
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
                state.messages.push(RenderedMessage::User {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    content: input.clone(),
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
                    state.pending_input = std::mem::take(&mut state.input_buffer);
                    state.history_index = Some(state.input_history.len() - 1);
                } else if let Some(idx) = state.history_index {
                    if idx > 0 {
                        state.history_index = Some(idx - 1);
                    }
                }
                if let Some(idx) = state.history_index {
                    state.input_buffer = state.input_history[idx].clone();
                    state.cursor_pos = state.input_buffer.len();
                }
            }
        }
        KeyEvent {
            code: KeyCode::Down,
            modifiers: _,
            ..
        } => {
            if let Some(idx) = state.history_index {
                if idx + 1 < state.input_history.len() {
                    state.history_index = Some(idx + 1);
                    state.input_buffer = state.input_history[idx + 1].clone();
                } else {
                    state.history_index = None;
                    state.input_buffer = std::mem::take(&mut state.pending_input);
                }
                state.cursor_pos = state.input_buffer.len();
            }
        }
        KeyEvent {
            code: KeyCode::Esc,
            modifiers: _,
            ..
        } => {
            state.input_buffer.clear();
            state.cursor_pos = 0;
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
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: _,
            ..
        } => {
            state.input_buffer.insert_str(state.cursor_pos, "    ");
            state.cursor_pos += 4;
            state.history_index = None;
            state.pending_input.clear();
        }
        _ => {
            let modified = apply_input_edit(key, &mut state.input_buffer, &mut state.cursor_pos);
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
            messages: vec![],
            status_text: "ready".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
        };
        assert!(state.messages.is_empty());
        assert_eq!(state.status_text, "ready");
    }

    /// Verifies that RenderedMessage variants can be constructed and debugged.
    #[test]
    fn test_rendered_message_variants() {
        let user = RenderedMessage::User {
            timestamp: "12:00".into(),
            content: "hi".into(),
        };
        assert!(format!("{:?}", user).contains("User"));

        let err = RenderedMessage::Error {
            message: "oops".into(),
        };
        assert!(format!("{:?}", err).contains("Error"));
    }

    /// Verifies that handle_app_event processes TextDelta by updating the last assistant message or creating one.
    #[test]
    fn test_handle_text_delta() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
        };
        let event = AppEvent::TextDelta {
            content: "Hello".into(),
            finish_reason: None,
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(state.streaming);
        assert_eq!(state.messages.len(), 1);
    }

    /// Verifies that handle_app_event processes ThinkingDelta by appending a Thinking message.
    #[test]
    fn test_handle_thinking_delta() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
        };
        let event = AppEvent::ThinkingDelta {
            content: "thinking...".into(),
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(matches!(
            state.messages.last(),
            Some(RenderedMessage::Thinking { .. })
        ));
    }

    /// Verifies that handle_app_event processes ToolRequest by setting the permission dialog.
    #[test]
    fn test_handle_tool_request() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
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
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: true,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
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
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
        };
        let event = AppEvent::Error {
            message: "failed".into(),
        };
        handle_app_event(event, &mut state).unwrap();
        assert!(matches!(
            state.messages.last(),
            Some(RenderedMessage::Error { .. })
        ));
    }

    /// Verifies that handle_permission_key with 'a' sends ApproveOnce and clears dialog.
    #[test]
    fn test_permission_key_approve_once() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
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
            cursor_pos: 0,
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<crate::protocol::Action>();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        handle_permission_key(key, &mut state, &tx).unwrap();
        assert!(state.show_permission_dialog.is_none());
    }

    /// Verifies that handle_app_event processes ToolResult by pushing ToolCall and ToolResult messages.
    #[test]
    fn test_handle_tool_result() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
        };
        let event = AppEvent::ToolResult {
            id: "t1".into(),
            name: "read".into(),
            params: serde_json::json!({"file": "x"}),
            output: "content".into(),
            metadata: None,
        };
        handle_app_event(event, &mut state).unwrap();
        assert_eq!(state.messages.len(), 2);
        assert!(matches!(
            state.messages[0],
            RenderedMessage::ToolCall { .. }
        ));
        assert!(matches!(
            state.messages[1],
            RenderedMessage::ToolResult { .. }
        ));
    }

    /// Verifies that handle_app_event processes StatusUpdate by setting the status text.
    #[test]
    fn test_handle_status_update() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "".into(),
            input_buffer: String::new(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 0,
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
    fn test_render_input_cursor_position() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "ready".into(),
            input_buffer: "hello".into(),
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 5,
        };

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| widgets::render(f, &mut state)).unwrap();

        // Input box is layout[2] at y=21, height=3.
        // Borders::TOP consumes 1 row for the border, so content starts at y=22.
        // Cursor x = area.x + len("> ") + cursor_pos = 0 + 2 + 5 = 7.
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(cursor.x, 7);
        assert_eq!(cursor.y, 22);
    }

    /// Verifies cursor uses display width, not byte offset, for multi-byte chars.
    #[test]
    fn test_render_input_cursor_position_multibyte() {
        let mut state = TuiState {
            messages: vec![],
            status_text: "ready".into(),
            input_buffer: "你好a".into(), // 6 bytes + 1 byte = 7 bytes, display width = 2+2+1 = 5
            show_permission_dialog: None,
            streaming: false,
            input_history: vec![],
            history_index: None,
            pending_input: String::new(),
            scroll_y: 0,
            follow_bottom: true,
            cursor_pos: 7, // byte offset at end
        };

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| widgets::render(f, &mut state)).unwrap();

        // Display width = 5, cursor x = 2 + 5 = 7 (byte offset would give 2 + 7 = 9 — drift!)
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(cursor.x, 7);
        assert_eq!(cursor.y, 22);
    }

    // ── apply_input_edit tests ──

    /// Verifies that Left arrow moves cursor one position backward.
    #[test]
    fn test_edit_left_moves_cursor() {
        let mut buf = String::from("hello");
        let mut cursor = 3;
        apply_input_edit(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 2);
        assert_eq!(buf, "hello"); // buffer unchanged
    }

    /// Verifies that Left arrow at position 0 does nothing.
    #[test]
    fn test_edit_left_at_zero_is_noop() {
        let mut buf = String::from("hi");
        let mut cursor = 0;
        apply_input_edit(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 0);
    }

    /// Verifies that Right arrow moves cursor one position forward.
    #[test]
    fn test_edit_right_moves_cursor() {
        let mut buf = String::from("hello");
        let mut cursor = 2;
        apply_input_edit(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 3);
        assert_eq!(buf, "hello"); // buffer unchanged
    }

    /// Verifies that Right arrow at end of buffer does nothing.
    #[test]
    fn test_edit_right_at_end_is_noop() {
        let mut buf = String::from("hi");
        let mut cursor = 2;
        apply_input_edit(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 2);
    }

    /// Verifies that Home key moves cursor to position 0.
    #[test]
    fn test_edit_home_moves_to_start() {
        let mut buf = String::from("hello");
        let mut cursor = 4;
        apply_input_edit(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 0);
        assert_eq!(buf, "hello");
    }

    /// Verifies that End key moves cursor to end of buffer.
    #[test]
    fn test_edit_end_moves_to_end() {
        let mut buf = String::from("hello");
        let mut cursor = 1;
        apply_input_edit(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 5);
        assert_eq!(buf, "hello");
    }

    /// Verifies that character input inserts at cursor position, not just at end.
    #[test]
    fn test_edit_char_inserts_at_cursor() {
        let mut buf = String::from("hlo");
        let mut cursor = 1;
        apply_input_edit(
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "helo");
        assert_eq!(cursor, 2);
    }

    /// Verifies that character input at end of buffer works like append.
    #[test]
    fn test_edit_char_at_end_appends() {
        let mut buf = String::from("hi");
        let mut cursor = 2;
        apply_input_edit(
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "hi!");
        assert_eq!(cursor, 3);
    }

    /// Verifies that Backspace deletes the character before the cursor.
    #[test]
    fn test_edit_backspace_deletes_before_cursor() {
        let mut buf = String::from("hello");
        let mut cursor = 3;
        apply_input_edit(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "helo");
        assert_eq!(cursor, 2);
    }

    /// Verifies that Backspace at position 0 does nothing.
    #[test]
    fn test_edit_backspace_at_zero_is_noop() {
        let mut buf = String::from("hi");
        let mut cursor = 0;
        apply_input_edit(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "hi");
        assert_eq!(cursor, 0);
    }

    /// Verifies that Delete removes the character at the cursor position.
    #[test]
    fn test_edit_delete_removes_at_cursor() {
        let mut buf = String::from("hello");
        let mut cursor = 1;
        apply_input_edit(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "hllo");
        assert_eq!(cursor, 1); // cursor stays
    }

    /// Verifies that Delete at end of buffer does nothing.
    #[test]
    fn test_edit_delete_at_end_is_noop() {
        let mut buf = String::from("hi");
        let mut cursor = 2;
        apply_input_edit(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "hi");
        assert_eq!(cursor, 2);
    }

    /// Verifies that typing a multi-byte char (Chinese) advances cursor by its byte width.
    #[test]
    fn test_edit_chinese_char_cursor_advances_correctly() {
        let mut buf = String::new();
        let mut cursor = 0;
        apply_input_edit(
            KeyEvent::new(KeyCode::Char('你'), KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "你");
        assert_eq!(cursor, 3); // 3-byte char, cursor on boundary after it
                               // Insert ASCII after — must not panic
        apply_input_edit(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "你a");
        assert_eq!(cursor, 4);
    }

    /// Verifies that Left arrow skips over a multi-byte character correctly.
    #[test]
    fn test_edit_left_over_multibyte_char() {
        let mut buf = String::from("你a"); // cursor at 0,3,4 boundaries
        let mut cursor = 4; // after "你a"
        apply_input_edit(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 3); // before 'a', after '你'
        apply_input_edit(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 0); // before '你'
    }

    /// Verifies that Right arrow skips over a multi-byte character correctly.
    #[test]
    fn test_edit_right_over_multibyte_char() {
        let mut buf = String::from("你a");
        let mut cursor = 0;
        apply_input_edit(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 3); // after '你'
        apply_input_edit(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(cursor, 4); // after 'a'
    }

    /// Verifies that Backspace removes the entire multi-byte character before cursor.
    #[test]
    fn test_edit_backspace_multibyte_char() {
        let mut buf = String::from("你a");
        let mut cursor = 4; // after "你a"
        apply_input_edit(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "你");
        assert_eq!(cursor, 3); // after '你'
                               // Backspace again removes the Chinese char
        apply_input_edit(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &mut buf,
            &mut cursor,
        );
        assert_eq!(buf, "");
        assert_eq!(cursor, 0);
    }
}

/// Applies an editing keystroke to the input buffer at the cursor position.
/// Returns true if the buffer content was modified.
fn apply_input_edit(key: KeyEvent, buffer: &mut String, cursor: &mut usize) -> bool {
    match key {
        KeyEvent {
            code: KeyCode::Left,
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            if *cursor > 0 {
                *cursor = buffer.floor_char_boundary(*cursor - 1);
            }
            false
        }
        KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            if *cursor < buffer.len() {
                *cursor = buffer.ceil_char_boundary(*cursor + 1);
            }
            false
        }
        KeyEvent {
            code: KeyCode::Home,
            modifiers: _,
            ..
        } => {
            *cursor = 0;
            false
        }
        KeyEvent {
            code: KeyCode::End,
            modifiers: _,
            ..
        } => {
            *cursor = buffer.len();
            false
        }
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: _,
            ..
        } => {
            buffer.insert(*cursor, c);
            *cursor += c.len_utf8();
            true
        }
        KeyEvent {
            code: KeyCode::Backspace,
            modifiers: _,
            ..
        } if *cursor > 0 => {
            let prev = buffer.floor_char_boundary(*cursor - 1);
            buffer.remove(prev);
            *cursor = prev;
            true
        }
        KeyEvent {
            code: KeyCode::Delete,
            modifiers: _,
            ..
        } if *cursor < buffer.len() => {
            buffer.remove(*cursor);
            true
        }
        _ => false,
    }
}

fn handle_app_event(event: AppEvent, state: &mut TuiState) -> anyhow::Result<()> {
    match event {
        AppEvent::TextDelta {
            content,
            finish_reason: _,
        } => {
            state.streaming = true;
            if let Some(RenderedMessage::Assistant {
                content: ref mut existing,
                ..
            }) = state.messages.last_mut()
            {
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
            if let Some(RenderedMessage::Thinking {
                content: ref mut existing,
            }) = state.messages.last_mut()
            {
                existing.push_str(&content);
            } else {
                state.messages.push(RenderedMessage::Thinking { content });
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
            state.follow_bottom = true;
            state.status_text = format!("emergence · ✓ ready ({:?})", stop_reason);
        }
        AppEvent::Error { message } => {
            state.messages.push(RenderedMessage::Error { message });
        }
    }
    Ok(())
}
