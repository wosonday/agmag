//! Full-screen project board (`agm board [project]`, `cd <project>` in `agm tui`).
//!
//! Pure parts: `flows` (edges between agents), `state` (keys → effects), `theme`, `view`
//! (ratatui rendering). This file owns all I/O: terminal guard, data loading, effects, loop.

pub mod flows;
pub mod state;
pub mod theme;
pub mod view;

use anyhow::Result;

use crate::config::Config;
use crate::hc::Hc;
use crate::project::{ProjectKey, ProjectSummary, ProjectView, RootCache};
use crate::store::{MessageRow, Store};

/// One-line footer message after an action or a failed refresh.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub ok: bool,
    pub text: String,
}

/// Everything one frame renders; rebuilt on every refresh.
#[derive(Debug, Clone, Default)]
pub struct BoardData {
    pub projects: Vec<ProjectSummary>,
    pub no_project_inbox: usize,
    /// None until a project is chosen.
    pub view: Option<ProjectView>,
    /// Parent/child edges of `view.agents` that exchanged at least one message.
    pub edges: Vec<flows::Edge>,
    /// Selected agent's newest ≤ 30 messages, newest first.
    pub conversation: Vec<MessageRow>,
    pub now_ms: i64,
}

/// Fails unless a human at an interactive terminal is calling (same rule as `agm tui`).
pub fn require_human(store: &Store, hc: &dyn Hc) -> Result<()> {
    let _ = (store, hc);
    todo!("Task 4")
}

/// `match_project` over the current project list; ambiguous / unknown → error.
pub fn resolve_project(
    store: &Store,
    cfg: &Config,
    cache: &mut RootCache,
    query: &str,
) -> Result<ProjectKey> {
    let _ = (store, cfg, cache, query);
    todo!("Task 4")
}

/// Loads one frame of data and calls `state.sync` before loading the selected agent's conversation.
pub fn load_data(
    store: &Store,
    cfg: &Config,
    cache: &mut RootCache,
    state: &mut state::BoardState,
    now_ms: i64,
) -> Result<BoardData> {
    let _ = (store, cfg, cache, state, now_ms);
    todo!("Task 4")
}

/// Runs an effect; the outcome (or error) becomes `state.note`. Never panics.
pub fn apply_effect(
    store: &Store,
    hc: &dyn Hc,
    effect: state::Effect,
    state: &mut state::BoardState,
    now_ms: i64,
) {
    let _ = (store, hc, effect, state, now_ms);
    todo!("Task 4")
}

/// Runs the board until the user quits. Caller must have passed `require_human`.
pub fn run(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    project: Option<ProjectKey>,
    refresh_s: u64,
    scan: &mut dyn FnMut(&Store) -> Result<()>,
) -> Result<()> {
    let _ = (store, cfg, hc, project, refresh_s, scan);
    todo!("Task 4")
}
