use ratatui::prelude::*;
use ratatui::widgets::*;
use super::{TuiState, RenderedMessage};
use super::themes;

pub fn render(f: &mut Frame, state: &super::TuiState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    render_chat_panel(f, layout[0], state);
    render_status_bar(f, layout[1], state);
    render_input_box(f, layout[2], state);
}

fn render_chat_panel(f: &mut Frame, area: Rect, state: &TuiState) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &state.messages {
        match msg {
            RenderedMessage::User { timestamp, content } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("[{}] You: ", timestamp), themes::dim_style()),
                    Span::styled(content, themes::user_style()),
                ]));
            }
            RenderedMessage::Assistant { timestamp, content, thinking, duration, tokens } => {
                if let Some(t) = thinking {
                    lines.push(Line::from(vec![
                        Span::styled(format!("🤖 (thinking): {}", t), themes::thinking_style()),
                    ]));
                }
                let mut prefix = format!("[{}] 🤖", timestamp);
                if let Some(d) = duration {
                    prefix.push_str(&format!(" ({})", d));
                }
                if let Some(tok) = tokens {
                    prefix.push_str(&format!(" {} tokens", tok));
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", prefix), themes::dim_style()),
                    Span::styled(content, themes::assistant_style()),
                ]));
            }
            RenderedMessage::ToolCall { tool, params, duration } => {
                let mut prefix = format!("tool:{}", tool);
                if let Some(d) = duration {
                    prefix.push_str(&format!(" ({})", d));
                }
                lines.push(Line::from(vec![
                    Span::styled(format!("{}: {}", prefix, params), themes::tool_style()),
                ]));
            }
            RenderedMessage::ToolResult { output } => {
                let truncated: String = output.lines().take(20).collect::<Vec<_>>().join("\n");
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("┌──────────────────────────────┐\n{}└──────────────────────────────┘", truncated),
                        themes::tool_style(),
                    ),
                ]));
            }
            RenderedMessage::Thinking { content } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("🤖 (thinking): {}", content), themes::thinking_style()),
                ]));
            }
            RenderedMessage::Error { message } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("⚠ {}", message), themes::error_style()),
                ]));
            }
        }
    }

    let line_count = lines.len();
    // 自动跟随：streaming 时始终滚到底部，否则使用用户手动偏移
    let max_scroll = line_count.saturating_sub(area.height as usize) as u16;
    let scroll_v = if state.streaming { max_scroll } else { state.scroll_offset.min(max_scroll) };
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .scroll((scroll_v, 0));

    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, state: &TuiState) {
    let status = Paragraph::new(state.status_text.as_str())
        .style(themes::status_bar_style());
    f.render_widget(status, area);
}

fn render_input_box(f: &mut Frame, area: Rect, state: &TuiState) {
    let input = Paragraph::new(format!("> {}", state.input_buffer))
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::White));
    f.render_widget(input, area);
}
