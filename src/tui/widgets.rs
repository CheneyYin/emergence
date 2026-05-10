use super::themes;
use super::{RenderedMessage, TuiState};
use ratatui::prelude::*;
use ratatui::widgets::*;
use unicode_width::UnicodeWidthStr;

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
                let content_lines: Vec<&str> = content.lines().collect();
                for (i, line_content) in content_lines.iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(format!("[{}] You: ", timestamp), themes::dim_style()),
                            Span::styled(*line_content, themes::user_style()),
                        ]));
                    } else {
                        lines.push(Line::from(vec![Span::styled(
                            *line_content,
                            themes::user_style(),
                        )]));
                    }
                }
            }
            RenderedMessage::Assistant {
                timestamp,
                content,
                thinking,
                duration,
                tokens,
            } => {
                if let Some(t) = thinking {
                    let think_lines: Vec<Line> = t
                        .lines()
                        .map(|l| Line::from(vec![Span::styled(l, themes::thinking_style())]))
                        .collect();
                    lines.extend(think_lines);
                }
                let mut prefix = format!("[{}] 🤖", timestamp);
                if let Some(d) = duration {
                    prefix.push_str(&format!(" ({})", d));
                }
                if let Some(tok) = tokens {
                    prefix.push_str(&format!(" {} tokens", tok));
                }
                let md_lines = super::markdown::render_markdown(content);
                if md_lines.is_empty() {
                    lines.push(Line::from(vec![Span::styled(prefix, themes::dim_style())]));
                } else {
                    let mut first_spans =
                        vec![Span::styled(format!("{} ", &prefix), themes::dim_style())];
                    first_spans.extend(md_lines[0].spans.clone());
                    lines.push(Line::from(first_spans));
                    for md_line in &md_lines[1..] {
                        lines.push(md_line.clone());
                    }
                }
            }
            RenderedMessage::ToolCall {
                tool,
                params,
                duration,
            } => {
                let mut prefix = format!("tool:{}", tool);
                if let Some(d) = duration {
                    prefix.push_str(&format!(" ({})", d));
                }
                lines.push(Line::from(vec![Span::styled(
                    format!("{}: {}", prefix, params),
                    themes::tool_style(),
                )]));
            }
            RenderedMessage::ToolResult { output } => {
                let truncated: Vec<&str> = output.lines().take(20).collect();
                lines.push(Line::from(vec![Span::styled("┌", themes::tool_style())]));
                for line_content in &truncated {
                    lines.push(Line::from(vec![Span::styled(
                        format!("│ {}", line_content),
                        themes::tool_style(),
                    )]));
                }
                lines.push(Line::from(vec![Span::styled("└", themes::tool_style())]));
            }
            RenderedMessage::Thinking { content } => {
                let think_lines: Vec<Line> = content
                    .lines()
                    .map(|l| Line::from(vec![Span::styled(l, themes::thinking_style())]))
                    .collect();
                lines.extend(think_lines);
            }
            RenderedMessage::Error { message } => {
                lines.push(Line::from(vec![Span::styled(
                    format!("⚠ {}", message),
                    themes::error_style(),
                )]));
            }
        }
    }

    // 精确折行计数：每行的展示宽度 / 列宽，向上取整
    let col_width = (area.width as usize).max(1);
    let total_visual_lines: usize = lines
        .iter()
        .map(|l| {
            let w = l.width().max(1);
            (w + col_width - 1) / col_width
        })
        .sum();
    let max_scroll = total_visual_lines.saturating_sub(area.height as usize) as u16;

    let auto_follow = state.streaming || state.follow_bottom;
    let scroll_offset = if auto_follow {
        max_scroll
    } else {
        state.scroll_y.min(max_scroll as usize) as u16
    };

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: true })
        .scroll((scroll_offset, 0));

    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, state: &TuiState) {
    let status = Paragraph::new(state.status_text.as_str()).style(themes::status_bar_style());
    f.render_widget(status, area);
}

fn render_input_box(f: &mut Frame, area: Rect, state: &TuiState) {
    let input = Paragraph::new(format!("> {}", state.input_buffer))
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::White));
    f.render_widget(input, area);
    let display_width = state.input_buffer[..state.cursor_pos].width() as u16;
    f.set_cursor_position(Position::new(
        (2 + display_width).min(area.width.saturating_sub(1)),
        area.y + 1,
    ));
}
