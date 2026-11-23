//! Read-only, bounded queries for the TUI project view.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::ToSql;

use super::{MessageRow, Store};
use crate::project::TurnRow;

/// `?1, ?2, … ?n` — numbered placeholders built from a slice length, never from values.
fn numbered_placeholders(n: usize) -> String {
    (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Store {
    /// Newest `limit` messages whose `from_t` or `to_t` is one of `parties`, newest first.
    /// Empty `parties` → empty result.
    pub fn messages_involving(&self, parties: &[String], limit: usize) -> Result<Vec<MessageRow>> {
        if parties.is_empty() {
            return Ok(Vec::new());
        }
        let ph = numbered_placeholders(parties.len());
        let limit_idx = parties.len() + 1;
        let sql = format!(
            "SELECT id, ts_ms, from_t, to_t, kind, task, text, status, detail FROM messages \
             WHERE from_t IN ({ph}) OR to_t IN ({ph}) ORDER BY id DESC LIMIT ?{limit_idx}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let lim = limit as i64;
        let mut binds: Vec<&dyn ToSql> = parties.iter().map(|p| p as &dyn ToSql).collect();
        binds.push(&lim);
        let rows = stmt
            .query_map(binds.as_slice(), |r| {
                Ok(MessageRow {
                    id: r.get(0)?,
                    ts_ms: r.get(1)?,
                    from: r.get(2)?,
                    to: r.get(3)?,
                    kind: r.get(4)?,
                    task: r.get(5)?,
                    text: r.get(6)?,
                    status: r.get(7)?,
                    detail: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Latest turn (role `assistant` or `tool`) per session; sessions without such turns are absent.
    /// Ties on `ts_ms` go to the higher `byte_offset`.
    pub fn latest_turns(&self, sessions: &[i64]) -> Result<HashMap<i64, TurnRow>> {
        if sessions.is_empty() {
            return Ok(HashMap::new());
        }
        let ph = numbered_placeholders(sessions.len());
        let sql = format!(
            "SELECT session, ts_ms, role, summary FROM ( \
               SELECT session, ts_ms, role, summary, \
                      ROW_NUMBER() OVER (PARTITION BY session ORDER BY ts_ms DESC, byte_offset DESC) AS rn \
               FROM turns WHERE session IN ({ph}) AND role IN ('assistant', 'tool') \
             ) WHERE rn = 1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let map = stmt
            .query_map(rusqlite::params_from_iter(sessions.iter()), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    TurnRow {
                        ts_ms: r.get(1)?,
                        role: r.get(2)?,
                        summary: r.get(3)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<HashMap<i64, TurnRow>>>()?;
        Ok(map)
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
        let mut out: HashMap<i64, Vec<u64>> =
            sessions.iter().map(|&s| (s, vec![0u64; n])).collect();
        if sessions.is_empty() || n == 0 || bucket_ms <= 0 {
            return Ok(out);
        }
        let k = sessions.len();
        let ph = numbered_placeholders(k);
        let since_idx = k + 1;
        let bucket_idx = k + 2;
        let end_idx = k + 3;
        let end = since_ms + bucket_ms * n as i64;
        let sql = format!(
            "SELECT session, (ts_ms - ?{since_idx}) / ?{bucket_idx} AS b, \
                    SUM(input_uncached + cache_write + output + reasoning) AS total \
             FROM usage \
             WHERE session IN ({ph}) AND ts_ms >= ?{since_idx} AND ts_ms < ?{end_idx} \
             GROUP BY session, b"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut binds: Vec<&dyn ToSql> = sessions.iter().map(|s| s as &dyn ToSql).collect();
        binds.push(&since_ms);
        binds.push(&bucket_ms);
        binds.push(&end);
        let mut rows = stmt.query(binds.as_slice())?;
        while let Some(r) = rows.next()? {
            let sid: i64 = r.get(0)?;
            let b: i64 = r.get(1)?;
            let total: i64 = r.get(2)?;
            if let Some(v) = out.get_mut(&sid)
                && b >= 0
                && (b as usize) < n
            {
                v[b as usize] = total.max(0) as u64;
            }
        }
        Ok(out)
    }
}
