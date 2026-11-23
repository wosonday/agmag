//! Renders the TUI project view (AGENTS, INBOX, MESSAGES) with the dashboard's ANSI helpers.
// Skeleton (Task 0): bodies are filled by Task 3; Task 5 removes this allow.
#![allow(dead_code, unused_variables)]

use crate::project::{ProjectSummary, ProjectView};
use crate::store::InboxRow;

/// AGENTS + INBOX + MESSAGES sections for one project (no header/footer).
pub fn render_project_body(pv: &ProjectView, width: usize, now_ms: i64) -> String {
    todo!("Task 3")
}

/// The `cd` listing: one line per project plus `(no project)` when `no_project > 0`.
pub fn render_project_list(list: &[ProjectSummary], no_project: usize, width: usize) -> String {
    todo!("Task 3")
}

/// One inbox item in full (wrapped, no line cap) for `show <#>`.
pub fn render_inbox_item(item: &InboxRow, width: usize) -> String {
    todo!("Task 3")
}
