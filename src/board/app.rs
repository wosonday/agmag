//! Full-screen `agm tui`: hosts the global screen and the project board as two screens.

use anyhow::Result;

use super::cmd::Dispatched;
use crate::config::Config;
use crate::dashboard::Section;
use crate::hc::Hc;
use crate::store::Store;

/// Runs the app until the user quits. `dispatch` runs one command line (shared with `--plain`).
/// Caller must have passed the human-only check.
pub fn run_app(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    refresh_s: u64,
    scan: &mut dyn FnMut(&Store) -> Result<()>,
    initial_view: Vec<Section>,
    dispatch: &mut dyn FnMut(&str) -> Dispatched,
) -> Result<()> {
    let _ = (store, cfg, hc, refresh_s, scan, initial_view, dispatch);
    todo!("Task E")
}
