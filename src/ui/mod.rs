pub mod message_view;
pub mod session_list;
pub mod status_bar;

use ratatui::style::{Modifier, Style};

use crate::model::Role;

// Cohesive color palette
pub mod palette {
    use ratatui::style::Color;

    pub const ACCENT: Color = Color::Rgb(138, 173, 244);       // soft blue
    pub const ACCENT_DIM: Color = Color::Rgb(91, 118, 166);    // muted blue
    pub const TEXT: Color = Color::Rgb(205, 214, 244);          // light text
    pub const TEXT_DIM: Color = Color::Rgb(147, 153, 178);      // secondary text
    pub const TEXT_FAINT: Color = Color::Rgb(88, 91, 112);      // subtle text
    pub const SURFACE: Color = Color::Rgb(36, 39, 58);          // panel bg
    pub const OVERLAY: Color = Color::Rgb(49, 50, 68);          // highlight bg
    pub const GREEN: Color = Color::Rgb(166, 218, 149);         // success
    pub const RED: Color = Color::Rgb(237, 135, 150);           // error
    pub const YELLOW: Color = Color::Rgb(238, 212, 159);        // warning
    pub const PEACH: Color = Color::Rgb(245, 169, 127);         // warm accent
    pub const MAUVE: Color = Color::Rgb(198, 160, 246);         // purple accent
    pub const TEAL: Color = Color::Rgb(139, 213, 202);          // teal accent

    // Provider-specific
    pub const CLAUDE: Color = Color::Rgb(245, 169, 127);        // peach/orange
    pub const COPILOT: Color = Color::Rgb(166, 218, 149);       // green
    pub const GEMINI: Color = Color::Rgb(138, 173, 244);        // blue
    pub const CODEX: Color = Color::Rgb(205, 214, 244);         // white
    pub const OPENCODE: Color = Color::Rgb(198, 160, 246);      // purple
}

pub fn role_style(role: Role) -> Style {
    match role {
        Role::User => Style::default().fg(palette::ACCENT),
        Role::Assistant => Style::default().fg(palette::GREEN),
        Role::System => Style::default().fg(palette::TEXT_DIM),
        Role::Tool => Style::default().fg(palette::YELLOW),
    }
}

pub fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(palette::ACCENT)
    } else {
        Style::default().fg(palette::TEXT_FAINT)
    }
}

pub fn highlight_style() -> Style {
    Style::default()
        .bg(palette::OVERLAY)
        .fg(palette::TEXT)
        .add_modifier(Modifier::BOLD)
}
