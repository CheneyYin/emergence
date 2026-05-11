use super::themes;
use super::{TuiState, Turn, TurnStatus};
use ratatui::prelude::*;
use ratatui::widgets::*;

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

    for turn in &state.turns {
        render_turn(&mut lines, turn);
    }

    let col_width = (area.width as usize).max(1);
    let total_visual_lines: usize = lines
        .iter()
        .map(|l| {
            let w = l.width().max(1);
            w.div_ceil(col_width)
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

fn render_turn<'a>(lines: &mut Vec<Line<'a>>, turn: &'a Turn) {
    let dim = themes::dim_style();
    let border = Style::default().fg(Color::DarkGray);

    // ── User ──
    lines.push(Line::from(vec![
        Span::styled(format!("[{}] You: ", turn.user.timestamp), dim),
        Span::styled(&turn.user.content, themes::user_style()),
    ]));

    // ── Assistant header ──
    let mut header = format!("[{}] 🤖", turn.assistant.timestamp);
    if let Some(ref d) = turn.assistant.duration {
        header.push_str(&format!(" · {}", d));
    }
    if let Some(tok) = turn.assistant.tokens {
        header.push_str(&format!(" · {} tokens", tok));
    }
    if turn.status == TurnStatus::InProgress {
        header.push_str(" · ⏳");
    }
    lines.push(Line::from(vec![Span::styled(header, dim)]));

    // ── Thinking ──
    if !turn.assistant.thinking.is_empty() {
        // Push thinking as a header line + content lines
        for think_line in turn.assistant.thinking.lines() {
            lines.push(Line::from(vec![Span::styled(
                think_line,
                themes::thinking_style(),
            )]));
        }
    }

    // ── Tool blocks (before body, Claude Code style) ──
    for tb in &turn.assistant.tool_blocks {
        lines.push(Line::from(vec![
            Span::styled("  ● ", themes::dim_style()),
            Span::styled(format!("{}({})", tb.tool, tb.summary), themes::tool_style()),
        ]));
        if let Some(ref result) = tb.result {
            let mut result_lines = result.lines();
            // First line gets └ prefix, rest are plain
            if let Some(first) = result_lines.next() {
                lines.push(Line::from(vec![Span::styled(
                    format!("  └ {}", first),
                    themes::dim_style(),
                )]));
                for rline in result_lines.take(19) {
                    lines.push(Line::from(vec![Span::styled(
                        format!("    {}", rline),
                        themes::dim_style(),
                    )]));
                }
            }
        }
    }

    // ── Body (markdown) ──
    if !turn.assistant.content.is_empty() {
        let md_lines = super::markdown::render_markdown(&turn.assistant.content);
        for md_line in md_lines {
            lines.push(md_line);
        }
    }

    // ── Error ──
    if let Some(ref err) = turn.assistant.error {
        lines.push(Line::from(vec![Span::styled(
            format!("⚠ {}", err),
            themes::error_style(),
        )]));
    }

    // ── Turn separator ──
    lines.push(Line::from(vec![Span::styled(
        "\u{2500}".repeat(60),
        border,
    )]));
}

fn render_status_bar(f: &mut Frame, area: Rect, state: &TuiState) {
    let status = Paragraph::new(state.status_text.as_str()).style(themes::status_bar_style());
    f.render_widget(status, area);
}

fn render_input_box(f: &mut Frame, area: Rect, state: &TuiState) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    let [prompt_area, text_area] =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(inner);

    f.render_widget(block, area);
    f.render_widget(Paragraph::new("> ").fg(Color::White), prompt_area);
    f.render_widget(&state.textarea, text_area);
    // TextArea renders its own visual cursor (REVERSED style) — no need for terminal cursor
}
