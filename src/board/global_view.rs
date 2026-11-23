//! Rendering of the global screen of the full-screen `agm tui`. No I/O.

use ratatui::Frame;

use super::global::{GlobalData, GlobalState};
use super::theme::Theme;

pub fn draw_global(
    frame: &mut Frame,
    state: &GlobalState,
    data: &GlobalData,
    theme: &Theme,
    ascii: bool,
) {
    let _ = (frame, state, data, theme, ascii);
    todo!("Task D")
}
