//! Read-only, bounded queries for the TUI project view.
// Skeleton (Task 0): bodies are filled by Task 2; Task 5 removes this allow.
#![allow(dead_code, unused_variables)]

use std::collections::HashMap;

use anyhow::Result;

use super::{MessageRow, Store};
use crate::project::TurnRow;

impl Store {
    /// Newest `limit` messages whose `from_t` or `to_t` is one of `parties`, newest first.
    /// Empty `parties` → empty result.
    pub fn messages_involving(&self, parties: &[String], limit: usize) -> Result<Vec<MessageRow>> {
        todo!("Task 2")
    }

    /// Latest turn (role `assistant` or `tool`) per session; sessions without such turns are absent.
    /// Ties on `ts_ms` go to the higher `byte_offset`.
    pub fn latest_turns(&self, sessions: &[i64]) -> Result<HashMap<i64, TurnRow>> {
        todo!("Task 2")
    }

    /// Per session, `n` buckets of `bucket_ms` starting at `since_ms` (oldest first) summing
    /// `input_uncached + cache_write + output + reasoning`. Every requested session is present.
    pub fn usage_buckets(
        &self,
        sessions: &[i64],
        since_ms: i64,
        bucket_ms: i64,
        n: usize,
    ) -> Result<HashMap<i64, Vec<u64>>> {
        todo!("Task 2")
    }
}
