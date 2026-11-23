//! Task E: the app loop's public surface.
//!
//! The per-outcome and key-routing tests live next to the code (`board::app`, `board::mod`):
//! their helpers (`apply_dispatched`, `terminal_project`, `BoardScreen`) are `pub(crate)` as the
//! plan requires, so an integration test cannot reach them. What a front end outside the crate
//! (`src/main.rs`, Task F) depends on is `run_app`'s exact signature, checked here.

use agmag::board::app::run_app;
use agmag::board::cmd::Dispatched;
use agmag::config::Config;
use agmag::dashboard::Section;
use agmag::hc::Hc;
use agmag::store::Store;
use anyhow::Result;

/// `run_app`'s shape: guarded by the human-only check and injected with the shared dispatcher.
type RunAppFn = for<'s, 'd> fn(
    &Store,
    &Config,
    &dyn Hc,
    u64,
    &'s mut (dyn FnMut(&Store) -> Result<()> + 's),
    Vec<Section>,
    &'d mut (dyn FnMut(&str) -> Dispatched + 'd),
) -> Result<()>;

#[test]
fn run_app_signature_matches_the_frozen_interface() {
    // A change here breaks the `agm tui` / `--plain` wiring; it must be deliberate.
    let frozen: RunAppFn = run_app;
    let _ = frozen;
}
