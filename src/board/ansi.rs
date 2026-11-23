//! Converts the dashboard's ANSI text (SGR colours only) into ratatui `Text`.

use ratatui::text::Text;

/// Keeps `ESC [ … m` styling; drops every other escape sequence and control character except `\n`.
pub fn ansi_to_text(s: &str) -> Text<'static> {
    let _ = s;
    todo!("Task B")
}
