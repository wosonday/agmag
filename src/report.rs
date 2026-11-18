//! Tổng hợp số liệu từ CSDL cho CLI: phiên, chi phí, hạn mức gói, lượt hội thoại.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use rusqlite::params;
use serde::Serialize;

use crate::config::{context_pct, cost_usd, Billing, Config};
use crate::model::{Role, Source, TurnSummary, Usage};
use crate::store::Store;

#[derive(Debug, Serialize)]
pub struct SessionRow {
    pub source: Source,
    pub session_id: Option<String>,
    pub path: String,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub last_ms: Option<i64>,
    pub usage: Usage,
    pub context_pct: Option<f64>,
    pub unknown_lines: u64,
    pub bad_lines: u64,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GroupBy {
    Model,
    Day,
    Source,
    Cwd,
}

#[derive(Debug, Serialize)]
pub struct CostRow {
    pub key: String,
    pub usage: Usage,
    pub billing: Option<Billing>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaRow {
    pub provider: String,
    pub window_minutes: u32,
    pub used_percent: f64,
    pub resets_at_s: i64,
    pub observed_ms: i64,
    pub reset_passed: bool,
}

const USAGE_SUMS: &str = "COALESCE(SUM(u.input_uncached),0), COALESCE(SUM(u.cache_read),0), \
     COALESCE(SUM(u.cache_write),0), COALESCE(SUM(u.output),0), COALESCE(SUM(u.reasoning),0)";

fn source_from(s: &str) -> Result<Source> {
    match s {
        "codex" => Ok(Source::Codex),
        "claude" => Ok(Source::Claude),
        "opencode" => Ok(Source::OpenCode),
        "agy" => Ok(Source::Agy),
        other => bail!("nguồn không rõ trong CSDL: {other}"),
    }
}

fn usage_at(row: &rusqlite::Row, first: usize) -> rusqlite::Result<Usage> {
    let get = |i: usize| row.get::<_, i64>(first + i).map(|v| v.max(0) as u64);
    Ok(Usage {
        input_uncached: get(0)?,
        cache_read: get(1)?,
        cache_write: get(2)?,
        output: get(3)?,
        reasoning: get(4)?,
    })
}

/// Các phiên có hoạt động từ `since_ms`, mới nhất trước.
pub fn sessions(store: &Store, cfg: &Config, since_ms: i64) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "SELECT s.source, s.session_id, s.path, s.cwd, s.model, s.last_ms, s.ctx_used, s.ctx_window,
                s.unknown_lines, s.bad_lines,
                (SELECT COUNT(*) FROM turns t WHERE t.session = s.id),
                (SELECT COUNT(*) FROM usage u WHERE u.session = s.id),
                {USAGE_SUMS}
         FROM sessions s LEFT JOIN usage u ON u.session = s.id
         WHERE COALESCE(s.last_ms, 0) >= ?1
         GROUP BY s.id
         ORDER BY s.last_ms DESC"
    );
    let mut stmt = store.conn().prepare(&sql)?;
    let rows = stmt.query_map(params![since_ms], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<i64>>(6)?,
            r.get::<_, Option<i64>>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, i64>(10)?,
            r.get::<_, i64>(11)?,
            usage_at(r, 12)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (source, session_id, path, cwd, model, last_ms, ctx_used, ctx_window, unknown, bad, turns, usages, usage) =
            row?;
        let window = ctx_window
            .map(|w| w as u64)
            .or_else(|| model.as_deref().and_then(|m| cfg.model(m)).and_then(|m| m.context_window));
        let warning = if bad > 0 {
            Some(format!("không đọc được {bad} dòng"))
        } else if turns > 0 && usages == 0 {
            Some("định dạng lạ?".to_string())
        } else {
            None
        };
        out.push(SessionRow {
            source: source_from(&source)?,
            session_id,
            path,
            cwd,
            model,
            last_ms,
            usage,
            context_pct: ctx_used.and_then(|used| context_pct(used as u64, window)),
            unknown_lines: unknown as u64,
            bad_lines: bad as u64,
            warning,
        });
    }
    Ok(out)
}

/// Ngày UTC `YYYY-MM-DD` của một mốc mili-giây (thuật toán civil_from_days).
pub fn utc_day(ts_ms: i64) -> String {
    let days = ts_ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

struct CostAcc {
    usage: Usage,
    billing: Option<Option<Billing>>, // None = chưa có hàng; Some(x) = billing chung (x None nếu lẫn/không rõ)
    cost: Option<f64>,                // None khi có ít nhất một hàng không tính được USD
}

/// Tổng token theo nhóm kể từ `since_ms` (theo thời điểm của từng bản ghi usage).
pub fn cost(store: &Store, cfg: &Config, by: GroupBy, since_ms: i64) -> Result<Vec<CostRow>> {
    let mut stmt = store.conn().prepare(
        "SELECT u.model, u.ts_ms, s.source, s.cwd,
                u.input_uncached, u.cache_read, u.cache_write, u.output, u.reasoning
         FROM usage u JOIN sessions s ON s.id = u.session
         WHERE u.ts_ms >= ?1",
    )?;
    let rows = stmt.query_map(params![since_ms], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            usage_at(r, 4)?,
        ))
    })?;
    let mut groups: BTreeMap<String, CostAcc> = BTreeMap::new();
    for row in rows {
        let (model, ts_ms, source, cwd, usage) = row?;
        let key = match by {
            GroupBy::Model => model.clone().unwrap_or_else(|| "?".into()),
            GroupBy::Day => utc_day(ts_ms),
            GroupBy::Source => source,
            GroupBy::Cwd => cwd.unwrap_or_else(|| "?".into()),
        };
        let billing = model.as_deref().and_then(|m| cfg.billing_for_model(m));
        let price = model.as_deref().and_then(|m| cfg.model(m)).and_then(|m| m.price.as_ref());
        // cost_usd chỉ được gọi cho model trả theo API và có giá (gói tháng không quy ra tiền).
        let row_cost = match (billing, price) {
            (Some(Billing::Api), Some(p)) => Some(cost_usd(&usage, p)),
            _ => None,
        };
        let acc = groups.entry(key).or_insert(CostAcc { usage: Usage::default(), billing: None, cost: Some(0.0) });
        acc.usage.add(&usage);
        acc.billing = Some(match acc.billing {
            None => billing,
            Some(prev) if prev == billing => prev,
            Some(_) => None,
        });
        acc.cost = match (acc.cost, row_cost) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
    }
    Ok(groups
        .into_iter()
        .map(|(key, acc)| CostRow { key, usage: acc.usage, billing: acc.billing.flatten(), cost_usd: acc.cost })
        .collect())
}

/// Bản ghi hạn mức mới nhất cho từng (nhà cung cấp, cửa sổ).
pub fn quota(store: &Store, now_s: i64) -> Result<Vec<QuotaRow>> {
    let mut stmt = store.conn().prepare(
        "SELECT q.provider, q.window_minutes, q.used_percent, q.resets_at_s, q.ts_ms FROM quota q
         WHERE q.ts_ms = (SELECT MAX(q2.ts_ms) FROM quota q2
                          WHERE q2.provider = q.provider AND q2.window_minutes = q.window_minutes)
         ORDER BY q.provider, q.window_minutes",
    )?;
    let rows = stmt.query_map([], |r| {
        let resets_at_s: i64 = r.get(3)?;
        Ok(QuotaRow {
            provider: r.get(0)?,
            window_minutes: r.get(1)?,
            used_percent: r.get(2)?,
            resets_at_s,
            observed_ms: r.get(4)?,
            reset_passed: resets_at_s <= now_s,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Tìm đúng một phiên theo tiền tố `session_id`; trả (id hàng, đường dẫn log).
pub fn resolve_session(store: &Store, prefix: &str) -> Result<(i64, String)> {
    let mut stmt = store.conn().prepare(
        "SELECT id, session_id, path FROM sessions
         WHERE session_id IS NOT NULL AND substr(session_id, 1, length(?1)) = ?1
         ORDER BY session_id",
    )?;
    let found = stmt
        .query_map(params![prefix], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match found.as_slice() {
        [] => bail!("không tìm thấy phiên có tiền tố '{prefix}'"),
        [(id, _, path)] => Ok((*id, path.clone())),
        many => {
            let ids: Vec<&str> = many.iter().map(|(_, sid, _)| sid.as_str()).collect();
            bail!("tiền tố '{prefix}' khớp {} phiên: {}", ids.len(), ids.join(", "))
        }
    }
}

/// `limit` lượt cuối của một phiên, theo thứ tự thời gian.
pub fn turns(store: &Store, session_prefix: &str, limit: usize) -> Result<Vec<TurnSummary>> {
    let (sid, _) = resolve_session(store, session_prefix)?;
    let mut stmt = store.conn().prepare(
        "SELECT ts_ms, role, summary, byte_offset FROM turns WHERE session = ?1
         ORDER BY byte_offset DESC LIMIT ?2",
    )?;
    let mut out = stmt
        .query_map(params![sid, limit as i64], |r| {
            let role: String = r.get(1)?;
            Ok(TurnSummary {
                ts_ms: r.get(0)?,
                role: match role.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::Tool,
                },
                summary: r.get(2)?,
                byte_offset: r.get::<_, i64>(3)? as u64,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    out.reverse();
    Ok(out)
}
