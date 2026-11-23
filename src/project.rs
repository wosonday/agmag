//! Project scope for `agm tui`: which project a terminal belongs to, who is in it, and the
//! data the project view renders. Pure functions except `git_common_root`.
// Skeleton (Task 0): bodies are filled by Task 1; Task 5 removes this allow.
#![allow(dead_code, unused_variables)]

use std::collections::HashMap;

use crate::store::{InboxRow, MessageRow, TerminalRow};

/// A project = repository root (worktrees join their main checkout).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectKey {
    /// `correlate::norm_cwd` of the repository root.
    pub root: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary {
    pub key: ProjectKey,
    pub live_agents: usize,
    pub open_inbox: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CdMatch {
    Found(ProjectKey),
    Ambiguous(Vec<String>),
    NotFound,
}

/// Latest `turns` row of a session (role `assistant` or `tool`).
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRow {
    pub ts_ms: i64,
    pub role: String,
    pub summary: String,
}

/// One agent line pair in the project view. Unknown values are `None` / `""`, never invented.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentView {
    pub address: String,
    /// Terminal name, else address.
    pub label: String,
    pub app: String,
    pub model: String,
    pub effort: String,
    /// herdr status (`-` when unknown).
    pub status: String,
    /// `live` | `exited`.
    pub state: String,
    pub depth: usize,
    pub last_sibling: bool,
    pub doing: Option<TurnRow>,
    /// (English kind, sent_by_this_terminal).
    pub last_msg: Option<(String, bool)>,
    pub context_pct: Option<f64>,
    pub tokens: Option<u64>,
    /// 12 buckets of 5 minutes, oldest first; None = no linked session.
    pub spark: Option<Vec<u64>>,
    /// `status` of the matching BRANCHES.md entry.
    pub stage: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectView {
    pub key: ProjectKey,
    pub agents: Vec<AgentView>,
    pub inbox: Vec<InboxRow>,
    pub messages: Vec<MessageRow>,
}

/// Terminal address → normalized project root.
pub type Roots = HashMap<String, String>;

/// cwd → root, resolved at most once per normalized cwd (failures cached too).
#[derive(Debug, Default)]
pub struct RootCache {
    map: HashMap<String, String>,
}

impl RootCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// `norm_cwd(cwd)` → `norm_cwd(root)`; when `resolve` returns None the root is `norm_cwd(cwd)`.
    pub fn root_of(&mut self, cwd: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
        todo!("Task 1")
    }
}

/// `git -C cwd rev-parse --path-format=absolute --git-common-dir` → its parent directory.
pub fn git_common_root(cwd: &str) -> Option<String> {
    todo!("Task 1")
}

/// Last path component of a root.
pub fn project_name(root: &str) -> String {
    todo!("Task 1")
}

/// Projects sorted by name, plus the number of open inbox items that belong to no project.
pub fn list_projects(
    terms: &[TerminalRow],
    inbox: &[InboxRow],
    roots: &Roots,
) -> (Vec<ProjectSummary>, usize) {
    todo!("Task 1")
}

pub fn match_project(list: &[ProjectSummary], query: &str) -> CdMatch {
    todo!("Task 1")
}

pub fn members<'a>(
    terms: &'a [TerminalRow],
    roots: &Roots,
    key: &ProjectKey,
) -> Vec<&'a TerminalRow> {
    todo!("Task 1")
}

/// Addresses and names of `members`, for message/inbox filtering.
pub fn parties(members: &[&TerminalRow]) -> Vec<String> {
    todo!("Task 1")
}

/// (index into members, depth, last_sibling) in display order.
pub fn tree_order(members: &[&TerminalRow]) -> Vec<(usize, usize, bool)> {
    todo!("Task 1")
}

/// `<lane>--<slug>` (cwd last component, else task) → `<lane>/<slug>` when `branches::validate_entry` accepts it.
pub fn branch_entry_for(cwd: Option<&str>, task: Option<&str>) -> Option<String> {
    todo!("Task 1")
}
