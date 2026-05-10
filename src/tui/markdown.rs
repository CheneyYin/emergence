use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

/// Converts markdown text to ratatui Lines with styled spans.
pub fn render_markdown(content: &str) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(content, options);
    let mut lines: Vec<Line> = Vec::new();
    let mut current_line: Vec<Span> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut code_block_buf: Vec<Line> = Vec::new();
    let mut in_code_block = false;
    let mut blockquote_depth: u8 = 0;
    let mut list_indent: u16 = 0;

    // Table state
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell: String = String::new();
    let mut in_cell = false;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let s = heading_style(level as u8);
                    style_stack.push(s);
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                }
                Tag::Strong => {
                    let s = current_style(&style_stack).add_modifier(Modifier::BOLD);
                    style_stack.push(s);
                }
                Tag::Emphasis => {
                    let s = current_style(&style_stack).add_modifier(Modifier::ITALIC);
                    style_stack.push(s);
                }
                Tag::Strikethrough => {
                    let s = current_style(&style_stack).add_modifier(Modifier::CROSSED_OUT);
                    style_stack.push(s);
                }
                Tag::BlockQuote(_) => {
                    blockquote_depth += 1;
                }
                Tag::Item => {
                    list_indent += 2;
                }
                Tag::Link { .. } => {
                    current_line.push(Span::styled("[", Style::default().fg(Color::Blue)));
                    let s = current_style(&style_stack).fg(Color::Blue);
                    style_stack.push(s);
                }
                Tag::Table(_) => {
                    flush_line(&mut lines, &mut current_line);
                    table_rows.clear();
                }
                Tag::TableHead => {}
                Tag::TableRow => {
                    current_row.clear();
                }
                Tag::TableCell => {
                    in_cell = true;
                    current_cell.clear();
                }
                _ => {}
            },

            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    style_stack.pop();
                    flush_line(&mut lines, &mut current_line);
                }
                TagEnd::Paragraph => {
                    flush_line(&mut lines, &mut current_line);
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    lines.append(&mut code_block_buf);
                }
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::BlockQuote(_) => {
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                    flush_line(&mut lines, &mut current_line);
                }
                TagEnd::Item => {
                    list_indent = list_indent.saturating_sub(2);
                    flush_line(&mut lines, &mut current_line);
                }
                TagEnd::Link => {
                    style_stack.pop();
                    current_line.push(Span::styled("]", Style::default().fg(Color::Blue)));
                }
                TagEnd::Table => {
                    flush_table(&mut lines, &table_rows);
                }
                TagEnd::TableHead => {
                    table_rows.push(std::mem::take(&mut current_row));
                }
                TagEnd::TableRow => {
                    table_rows.push(std::mem::take(&mut current_row));
                }
                TagEnd::TableCell => {
                    in_cell = false;
                    current_row.push(std::mem::take(&mut current_cell));
                }
                _ => {}
            },

            Event::Text(text) => {
                if in_cell {
                    current_cell.push_str(&text);
                } else if in_code_block {
                    for line_text in text.lines() {
                        code_block_buf.push(Line::from(vec![
                            Span::styled(
                                format!("  {}", line_text.to_string()),
                                Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 40)),
                            ),
                        ]));
                    }
                } else {
                    let style = current_style(&style_stack);
                    let styled = if blockquote_depth > 0 {
                        Span::styled(text.to_string(), style.add_modifier(Modifier::DIM))
                    } else {
                        Span::styled(text.to_string(), style)
                    };
                    current_line.push(styled);
                }
            }

            Event::Code(code) => {
                if in_cell {
                    current_cell.push_str(&code);
                } else {
                    let s = Style::default()
                        .fg(Color::Green)
                        .bg(Color::Rgb(50, 50, 50));
                    current_line.push(Span::styled(code.to_string(), s));
                }
            }

            Event::SoftBreak => {
                if in_cell {
                    current_cell.push(' ');
                } else {
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::HardBreak => {
                flush_line(&mut lines, &mut current_line);
            }

            Event::Rule => {
                flush_line(&mut lines, &mut current_line);
                let w = 60;
                lines.push(Line::from(vec![Span::styled(
                    "\u{2500}".repeat(w),
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            _ => {}
        }
    }

    // Flush remaining
    flush_line(&mut lines, &mut current_line);
    lines.append(&mut code_block_buf);
    lines
}

fn flush_table(lines: &mut Vec<Line<'static>>, rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return;
    }

    // Pad cell content to display-width fill (not char count, for CJK).
    fn pad_cell(cell: &str, width: usize) -> String {
        let dw = cell.width();
        if dw >= width {
            cell.to_string()
        } else {
            format!("{}{}", cell, " ".repeat(width - dw))
        }
    }

    // Calculate column widths (display width, min 3).
    let mut widths: Vec<usize> = vec![3; col_count];
    for row in rows {
        for (ci, cell) in row.iter().enumerate() {
            if ci < col_count {
                widths[ci] = widths[ci].max(cell.width());
            }
        }
    }

    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let normal = Style::default();

    // Build a horizontal border: +-----+-----+
    let make_sep = |left: &str, mid: &str, right: &str| -> Line<'static> {
        let mut s = String::from(left);
        for (i, w) in widths.iter().enumerate() {
            s.push_str(&"\u{2500}".repeat(*w + 2)); // two spaces of padding
            if i + 1 < col_count {
                s.push_str(mid);
            }
        }
        s.push_str(right);
        Line::from(vec![Span::styled(s, dim)])
    };

    // Top border
    lines.push(make_sep("\u{250c}", "\u{252c}", "\u{2510}"));

    // Header row
    if let Some(header) = rows.first() {
        let mut spans: Vec<Span> = vec![Span::styled("\u{2502} ", dim)];
        for (ci, cell) in header.iter().enumerate().take(col_count) {
            spans.push(Span::styled(pad_cell(cell, widths[ci]), header_style));
            spans.push(Span::styled(" \u{2502} ", dim));
        }
        lines.push(Line::from(spans));

        // Header-data separator
        lines.push(make_sep("\u{251c}", "\u{253c}", "\u{2524}"));
    }

    // Data rows
    for row in rows.iter().skip(1) {
        let mut spans: Vec<Span> = vec![Span::styled("\u{2502} ", dim)];
        for (ci, cell) in row.iter().enumerate().take(col_count) {
            spans.push(Span::styled(pad_cell(cell, widths[ci]), normal));
            spans.push(Span::styled(" \u{2502} ", dim));
        }
        lines.push(Line::from(spans));
    }

    // Bottom border
    lines.push(make_sep("\u{2514}", "\u{2534}", "\u{2518}"));
}

fn heading_style(level: u8) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        1 => base.fg(Color::Yellow),
        2 => base.fg(Color::Yellow),
        3 => base.fg(Color::LightYellow),
        _ => base.fg(Color::LightYellow),
    }
}

fn current_style(stack: &[Style]) -> Style {
    *stack.last().unwrap_or(&Style::default())
}

fn flush_line<'a>(lines: &mut Vec<Line<'a>>, current: &mut Vec<Span<'a>>) {
    if !current.is_empty() {
        lines.push(Line::from(std::mem::take(current)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies plain text produces a single line without special styling.
    #[test]
    fn test_plain_text_single_line() {
        let result = render_markdown("hello world");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spans.len(), 1);
        assert_eq!(result[0].spans[0].content, "hello world");
    }

    /// Verifies bold text is rendered with BOLD modifier.
    #[test]
    fn test_bold_text() {
        let result = render_markdown("**bold**");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spans[0].content, "bold");
        assert!(result[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    /// Verifies italic text is rendered with ITALIC modifier.
    #[test]
    fn test_italic_text() {
        let result = render_markdown("*italic*");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spans[0].content, "italic");
        assert!(result[0].spans[0].style.add_modifier.contains(Modifier::ITALIC));
    }

    /// Verifies inline code is rendered with a different foreground color.
    #[test]
    fn test_inline_code() {
        let result = render_markdown("use `std::io` here");
        assert_eq!(result.len(), 1);
        let code_span = &result[0].spans[1]; // "std::io"
        assert_eq!(code_span.content, "std::io");
        assert_eq!(code_span.style.fg, Some(Color::Green));
    }

    /// Verifies headings are styled with BOLD and a heading color.
    #[test]
    fn test_heading() {
        let result = render_markdown("# Title");
        assert_eq!(result.len(), 1);
        let span = &result[0].spans[0];
        assert_eq!(span.content, "Title");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    /// Verifies code blocks are rendered with background color and cyan text.
    #[test]
    fn test_code_block() {
        let result = render_markdown("```\nfn main() {\n    println!();\n}\n```");
        assert!(result.len() >= 2);
        // Code lines should have a different background
        let code_span = &result[0].spans[0];
        assert_eq!(code_span.style.fg, Some(Color::Cyan));
        assert!(code_span.style.bg.is_some());
    }

    /// Verifies blockquotes are rendered with DIM modifier.
    #[test]
    fn test_blockquote() {
        let result = render_markdown("> quoted text");
        assert_eq!(result.len(), 1);
        let span = &result[0].spans[0];
        assert_eq!(span.content, "quoted text");
        assert!(span.style.add_modifier.contains(Modifier::DIM));
    }

    /// Verifies horizontal rules produce a separator line.
    #[test]
    fn test_horizontal_rule() {
        let result = render_markdown("before\n\n---\n\nafter");
        assert!(result.len() >= 3);
        // The separator line should be long dashes
        let sep = &result[1].spans[0];
        assert!(sep.content.contains('\u{2500}'));
    }

    /// Verifies that two paragraphs separated by a blank line produce two lines.
    #[test]
    fn test_multiline() {
        let result = render_markdown("line one\n\nline two");
        assert_eq!(result.len(), 2);
    }

    /// Verifies single newlines within a paragraph produce separate lines, not spaces.
    #[test]
    fn test_soft_break_preserves_newlines() {
        let result = render_markdown("Sessions:\nabc - msg\ndef - msg");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].spans[0].content, "Sessions:");
        assert_eq!(result[1].spans[0].content, "abc - msg");
        assert_eq!(result[2].spans[0].content, "def - msg");
    }

    /// Verifies empty content produces no lines.
    #[test]
    fn test_empty() {
        let result = render_markdown("");
        assert!(result.is_empty());
    }

    /// Verifies a simple table renders top border + header + separator + data + bottom border.
    #[test]
    fn test_simple_table() {
        let result = render_markdown("|A|B|\n|---|---|\n|1|2|");
        assert_eq!(result.len(), 5); // top + header + sep + data + bottom
        let hdr: String = result[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(hdr.contains("A") && hdr.contains("B"));
        let dat: String = result[3].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(dat.contains("1") && dat.contains("2"));
    }

    /// Verifies LLM-typical table format with spaces and longer dashes also works.
    #[test]
    fn test_llm_style_table() {
        let md = "| File | Description |\n|------|-------------|\n| main.rs | Entry point |\n| lib.rs | Library root |";
        let result = render_markdown(md);
        assert!(result.len() >= 4, "expected >=4 lines, got {} lines", result.len());
    }

    /// Verifies a table with single-column layout works.
    #[test]
    fn test_single_column_table() {
        let md = "| Value |\n|-------|\n| 1 |\n| 2 |";
        let result = render_markdown(md);
        assert_eq!(result.len(), 6); // top + header + sep + 2 data + bottom
    }
}
