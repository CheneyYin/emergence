use ratatui::prelude::*;
use ratatui::widgets::*;
use super::PermissionDialogState;
use crate::permissions::RiskLevel;

pub fn render_permission_dialog(f: &mut Frame, state: &PermissionDialogState) {
    let area = centered_rect(60, 40, f.area());

    let risk_label = match state.risk {
        RiskLevel::ReadOnly => "ReadOnly",
        RiskLevel::Write => "⚠ Write",
        RiskLevel::System => "🚫 System",
    };

    let text = format!(
        "Tool: {}\nRisk: {}\n\nParams:\n  {}\n\n[A]pprove Once  [Y]es Always  [D]eny",
        state.tool_name,
        risk_label,
        serde_json::to_string_pretty(&state.params).unwrap_or_default(),
    );

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Permission Required ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
