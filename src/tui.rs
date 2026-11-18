//! `agm tui` — viết ở Task 12.
use anyhow::Result;

use agmag::config::Config;
use agmag::store::Store;

pub fn run(_store: &Store, _cfg: &Config, _refresh: u64, _scan: impl FnMut(&Store) -> Result<()>) -> Result<()> {
    anyhow::bail!("agm tui chưa có")
}
