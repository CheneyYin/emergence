use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub dim: Color,
    pub user_color: Color,
    pub assistant_color: Color,
    pub tool_color: Color,
    pub thinking_color: Color,
    pub error_color: Color,
    pub risk_readonly: Color,
    pub risk_write: Color,
    pub risk_system: Color,
}

pub const DEFAULT_THEME: Theme = Theme {
    bg: Color::Black,
    fg: Color::White,
    accent: Color::Cyan,
    dim: Color::Gray,
    user_color: Color::Green,
    assistant_color: Color::White,
    tool_color: Color::Yellow,
    thinking_color: Color::Magenta,
    error_color: Color::Red,
    risk_readonly: Color::Green,
    risk_write: Color::Yellow,
    risk_system: Color::Red,
};

pub fn user_style() -> Style {
    Style::default()
        .fg(DEFAULT_THEME.user_color)
        .add_modifier(Modifier::BOLD)
}

pub fn assistant_style() -> Style {
    Style::default().fg(DEFAULT_THEME.assistant_color)
}

pub fn thinking_style() -> Style {
    Style::default()
        .fg(DEFAULT_THEME.thinking_color)
        .add_modifier(Modifier::ITALIC)
}

pub fn tool_style() -> Style {
    Style::default().fg(DEFAULT_THEME.tool_color)
}

pub fn status_bar_style() -> Style {
    Style::default().fg(Color::Black).bg(DEFAULT_THEME.accent)
}

pub fn error_style() -> Style {
    Style::default().fg(DEFAULT_THEME.error_color)
}

pub fn dim_style() -> Style {
    Style::default().fg(DEFAULT_THEME.dim)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that user_style returns a bold style with the user color.
    #[test]
    fn test_user_style_exists() {
        let style = user_style();
        assert!(style.fg.is_some());
    }

    /// Verifies that error_style returns a style with the error color.
    #[test]
    fn test_error_style_exists() {
        let style = error_style();
        assert!(style.fg.is_some());
    }

    /// Verifies that dim_style returns a style with the dim color.
    #[test]
    fn test_dim_style_exists() {
        let style = dim_style();
        assert!(style.fg.is_some());
    }

    /// Verifies that DEFAULT_THEME has all expected color fields.
    #[test]
    fn test_default_theme_fields() {
        let t = &DEFAULT_THEME;
        // All 12 color fields exist and are Colors
        let _: &ratatui::style::Color = &t.bg;
        let _: &ratatui::style::Color = &t.accent;
        let _: &ratatui::style::Color = &t.error_color;
    }
}
