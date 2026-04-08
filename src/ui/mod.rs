pub mod message_view;
pub mod session_list;
pub mod status_bar;

use ratatui::style::{Color, Style};

use crate::model::Role;

pub fn role_style(role: Role) -> Style {
    match role {
        Role::User => Style::default().fg(Color::Cyan),
        Role::Assistant => Style::default().fg(Color::Green),
        Role::System => Style::default().fg(Color::DarkGray),
        Role::Tool => Style::default().fg(Color::Yellow),
    }
}
