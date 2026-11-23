//! Board rendering with ratatui widgets. No I/O.

use ratatui::Frame;

use super::BoardData;
use super::state::BoardState;
use super::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// ≥ 120 columns and ≥ 20 rows: projects | orchestration | detail.
    Wide,
    /// 80–119 columns: orchestration | detail.
    Medium,
    /// Anything smaller: stacked single column.
    Narrow,
}

pub fn layout_mode(width: u16, height: u16) -> LayoutMode {
    let _ = (width, height);
    todo!("Task 3")
}

pub fn draw(frame: &mut Frame, state: &BoardState, data: &BoardData, theme: &Theme, ascii: bool) {
    let _ = (frame, state, data, theme, ascii);
    todo!("Task 3")
}
