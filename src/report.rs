//! Tổng hợp số liệu từ CSDL cho CLI: phiên, chi phí, hạn mức gói, lượt hội thoại.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, bail};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::config::{Billing, Config, context_pct, cost_usd};
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
    Month,
    Source,
    Cwd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum UsageWindow {
    #[value(name = "24h")]
    Last24h,
    Today,
    Month,
    #[value(name = "4months")]
    FourMonths,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageBucket {
    pub usage: Usage,
    /// Actually billed: API-billed models with a price only (None if one of them has no price).
    pub api_usd: Option<f64>,
    /// Worth at list API prices, every priced model (subscriptions included). Tokens without a price are
    /// counted in `unpriced_tokens` instead, so the value is a lower bound when that is non-zero.
    pub value_usd: f64,
    pub unpriced_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub now_ms: i64,
    pub offset_label: String,
    pub four_months_start_ms: i64,
    pub four_months_start_day: String,
    pub last_24h: UsageBucket,
    pub today: UsageBucket,
    pub this_month: UsageBucket,
    pub recent_months: Vec<CostRow>,
    pub covered_since_ms: Option<i64>,
    pub incomplete_coverage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBounds {
    pub offset_label: String,
    pub last_24h_start_ms: i64,
    pub today_start_ms: i64,
    pub month_start_ms: i64,
    pub four_months_start_ms: i64,
    pub recent_month_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostRow {
    pub key: String,
    pub usage: Usage,
    pub billing: Option<Billing>,
    /// Actually billed (API-billed models with a price); None when unknown or not billed per token.
    pub cost_usd: Option<f64>,
    /// Worth at list API prices for every priced model; None when no token in the row has a price.
    pub value_usd: Option<f64>,
    /// Tokens of models without a known price (not included in `value_usd`).
    pub unpriced_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaRow {
    pub provider: String,
    pub window_minutes: u32,
    /// Phần đã dùng, đúng như log/hook ghi (Codex `used_percent`, Claude `used_percentage`).
    pub used_percent: f64,
    /// Phần còn lại để hiển thị (giảm dần từ 100%, như Codex `/status` và agy `/usage`);
    /// 100 khi cửa sổ đã reset mà chưa có số liệu mới.
    pub left_percent: f64,
    pub resets_at_s: i64,
    pub observed_ms: i64,
    pub reset_passed: bool,
}

/// % đã dùng còn hiệu lực: cửa sổ đã reset mà chưa có số mới thì số cũ không còn đúng → 0.
pub fn effective_used_percent(used_percent: f64, resets_at_s: i64, now_s: i64) -> f64 {
    if resets_at_s <= now_s {
        0.0
    } else {
        used_percent
    }
}

/// % còn lại (0–100) từ % đã dùng; NaN giữ nguyên để hiển thị là chưa rõ.
pub fn left_percent(used_percent: f64, resets_at_s: i64, now_s: i64) -> f64 {
    let used = effective_used_percent(used_percent, resets_at_s, now_s);
    if used.is_nan() {
        f64::NAN
    } else {
        (100.0 - used).clamp(0.0, 100.0)
    }
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
        let (
            source,
            session_id,
            path,
            cwd,
            model,
            last_ms,
            ctx_used,
            ctx_window,
            unknown,
            bad,
            turns,
            usages,
            usage,
        ) = row?;
        let window = ctx_window.map(|w| w as u64).or_else(|| {
            model
                .as_deref()
                .and_then(|m| cfg.model(m))
                .and_then(|m| m.context_window)
        });
        let warning = if bad > 0 {
            Some(format!("{bad} unreadable lines"))
        } else if turns > 0 && usages == 0 {
            Some("unknown format?".to_string())
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

/// (năm, tháng 1..12, ngày 1..31) từ số ngày kể từ 1970-01-01 (Howard Hinnant).
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Số ngày kể từ 1970-01-01 cho (năm, tháng 1..12, ngày 1..31).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Ngày UTC `YYYY-MM-DD` của một mốc mili-giây.
pub fn utc_day(ts_ms: i64) -> String {
    let (y, m, d) = civil_from_days(ts_ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

fn local_ymd(ts_ms: i64, offset_at: &mut dyn FnMut(i64) -> i64) -> (i64, i64, i64) {
    let ts_s = ts_ms.div_euclid(1000);
    let off_s = offset_at(ts_s);
    civil_from_days((ts_s + off_s).div_euclid(86_400))
}

pub fn local_day_key(ts_ms: i64, offset_at: &mut dyn FnMut(i64) -> i64) -> String {
    let (y, m, d) = local_ymd(ts_ms, offset_at);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn local_month_key(ts_ms: i64, offset_at: &mut dyn FnMut(i64) -> i64) -> String {
    let (y, m, _) = local_ymd(ts_ms, offset_at);
    format!("{y:04}-{m:02}")
}

fn local_midnight_ms(
    y: i64,
    m: i64,
    d: i64,
    hint_off_s: i64,
    offset_at: &mut dyn FnMut(i64) -> i64,
) -> i64 {
    let mid_local_s = days_from_civil(y, m, d) * 86_400;
    let approx_utc_s = mid_local_s - hint_off_s;
    let off = offset_at(approx_utc_s);
    let candidate_s = mid_local_s - off;
    // Tinh chỉnh thêm 1 bước phòng chuyển đổi DST ngay sát nửa đêm.
    let refined_off = offset_at(candidate_s);
    (mid_local_s - refined_off) * 1000
}

pub fn format_offset_label(offset_s: i64) -> String {
    let sign = if offset_s >= 0 { '+' } else { '-' };
    let mins = offset_s.abs() / 60;
    format!("UTC{sign}{:02}:{:02}", mins / 60, mins % 60)
}

/// Tính ranh giới 24h / hôm nay / tháng này / `months` tháng gần nhất theo hàm offset có thể thay khi test.
pub fn compute_local_bounds(
    now_ms: i64,
    months: u32,
    offset_at: &mut dyn FnMut(i64) -> i64,
) -> LocalBounds {
    let months = months.max(1) as i64;
    let now_s = now_ms.div_euclid(1000);
    let off_now = offset_at(now_s);
    let (y, m, d) = civil_from_days((now_s + off_now).div_euclid(86_400));
    let today_start_ms = local_midnight_ms(y, m, d, off_now, offset_at);
    let month_start_ms = local_midnight_ms(y, m, 1, off_now, offset_at);

    let shift_month = |back: i64| -> (i64, i64) {
        let total = (y * 12 + (m - 1)) - back;
        (total.div_euclid(12), total.rem_euclid(12) + 1)
    };
    let (sy, sm) = shift_month(months - 1);
    let four_months_start_ms = local_midnight_ms(sy, sm, 1, off_now, offset_at);
    let mut recent_month_keys = Vec::with_capacity(months as usize);
    for back in (0..months).rev() {
        let (my, mm) = shift_month(back);
        recent_month_keys.push(format!("{my:04}-{mm:02}"));
    }

    LocalBounds {
        offset_label: format_offset_label(off_now),
        last_24h_start_ms: now_ms - 86_400_000,
        today_start_ms,
        month_start_ms,
        four_months_start_ms,
        recent_month_keys,
    }
}

/// Hàm tra độ lệch giờ máy tại một mốc UTC (giây) qua SQLite `'localtime'`, cache theo giờ.
pub fn sqlite_offset_at(conn: &Connection) -> impl FnMut(i64) -> i64 + '_ {
    let mut cache: HashMap<i64, i64> = HashMap::new();
    move |ts_s: i64| {
        let hour = ts_s.div_euclid(3600);
        if let Some(&off) = cache.get(&hour) {
            return off;
        }
        let off = conn
            .query_row(
                "SELECT CAST(strftime('%s', ?1, 'unixepoch', 'localtime') AS INTEGER) - ?1",
                [ts_s],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);
        cache.insert(hour, off);
        off
    }
}

pub fn window_since_ms(
    window: UsageWindow,
    now_ms: i64,
    months: u32,
    offset_at: &mut dyn FnMut(i64) -> i64,
) -> i64 {
    let b = compute_local_bounds(now_ms, months, offset_at);
    match window {
        UsageWindow::Last24h => b.last_24h_start_ms,
        UsageWindow::Today => b.today_start_ms,
        UsageWindow::Month => b.month_start_ms,
        UsageWindow::FourMonths => b.four_months_start_ms,
    }
}

struct CostAcc {
    usage: Usage,
    billing: Option<Option<Billing>>, // None = chưa có hàng; Some(x) = billing chung (x None nếu lẫn/không rõ)
    cost: Option<f64>,                // None khi có ít nhất một hàng không tính được USD
    value: Option<f64>,               // giá trị theo bảng giá của các hàng có giá
    unpriced: u64,                    // token của hàng không có giá
}

impl CostAcc {
    fn new() -> Self {
        CostAcc {
            usage: Usage::default(),
            billing: None,
            cost: Some(0.0),
            value: None,
            unpriced: 0,
        }
    }

    fn add(
        &mut self,
        usage: &Usage,
        billing: Option<Billing>,
        row_cost: Option<f64>,
        row_value: Option<f64>,
    ) {
        self.usage.add(usage);
        self.billing = Some(match self.billing {
            None => billing,
            Some(prev) if prev == billing => prev,
            Some(_) => None,
        });
        self.cost = match (self.cost, row_cost) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
        match row_value {
            Some(v) => self.value = Some(self.value.unwrap_or(0.0) + v),
            None => self.unpriced += usage.total(),
        }
    }

    fn into_row(self, key: String) -> CostRow {
        CostRow {
            key,
            usage: self.usage,
            billing: self.billing.flatten(),
            cost_usd: self.cost,
            value_usd: self.value,
            unpriced_tokens: self.unpriced,
        }
    }
}

/// (actually billed USD, list-price value USD) of one usage record.
fn price_row(
    cfg: &Config,
    model: Option<&str>,
    usage: &Usage,
) -> (Option<Billing>, Option<f64>, Option<f64>) {
    let billing = model.and_then(|m| cfg.billing_for_model(m));
    let value = model
        .and_then(|m| cfg.price_for(m))
        .map(|p| cost_usd(usage, &p));
    // Real cost only for API-billed models; a subscription is not billed per token.
    let cost = if billing == Some(Billing::Api) {
        value
    } else {
        None
    };
    (billing, cost, value)
}

/// Column-wise sum of cost rows (the SUM line of `agm cost`). `value_usd` is None only if no row had a price.
pub fn cost_totals(rows: &[CostRow]) -> CostRow {
    let mut usage = Usage::default();
    let mut value: Option<f64> = None;
    let mut unpriced = 0;
    for r in rows {
        usage.add(&r.usage);
        if let Some(v) = r.value_usd {
            value = Some(value.unwrap_or(0.0) + v);
        }
        unpriced += r.unpriced_tokens;
    }
    CostRow {
        key: "SUM".into(),
        usage,
        billing: None,
        cost_usd: None,
        value_usd: value,
        unpriced_tokens: unpriced,
    }
}

/// USD cell text: `$1.23`; `≥ $1.23` when some tokens have no price (the value is a lower bound); `?` when none do.
pub fn value_label(value_usd: Option<f64>, unpriced_tokens: u64) -> String {
    match value_usd {
        None => "?".into(),
        Some(v) if unpriced_tokens > 0 => format!("≥ ${v:.2}"),
        Some(v) => format!("${v:.2}"),
    }
}

/// Tổng token theo nhóm kể từ `since_ms` (dùng giờ mặc định của máy tính).
pub fn cost(store: &Store, cfg: &Config, by: GroupBy, since_ms: i64) -> Result<Vec<CostRow>> {
    let mut off = sqlite_offset_at(store.conn());
    cost_with_offset(store, cfg, by, since_ms, &mut off)
}

/// Tổng token theo nhóm kể từ `since_ms` với hàm offset múi giờ tùy chỉnh (test tái lập 100%).
pub fn cost_with_offset(
    store: &Store,
    cfg: &Config,
    by: GroupBy,
    since_ms: i64,
    offset_at: &mut dyn FnMut(i64) -> i64,
) -> Result<Vec<CostRow>> {
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
            GroupBy::Day => local_day_key(ts_ms, offset_at),
            GroupBy::Month => local_month_key(ts_ms, offset_at),
            GroupBy::Source => source,
            GroupBy::Cwd => cwd.unwrap_or_else(|| "?".into()),
        };
        let (billing, row_cost, row_value) = price_row(cfg, model.as_deref(), &usage);
        groups
            .entry(key)
            .or_insert_with(CostAcc::new)
            .add(&usage, billing, row_cost, row_value);
    }
    Ok(groups
        .into_iter()
        .map(|(key, acc)| acc.into_row(key))
        .collect())
}

/// Tổng hợp 24h, hôm nay, tháng này và 4 tháng gần nhất (đủ cả bucket 0).
pub fn usage_summary(store: &Store, cfg: &Config, now_ms: i64) -> Result<UsageSummary> {
    let mut off = sqlite_offset_at(store.conn());
    usage_summary_with_offset(store, cfg, now_ms, &mut off)
}

pub fn usage_summary_with_offset(
    store: &Store,
    cfg: &Config,
    now_ms: i64,
    offset_at: &mut dyn FnMut(i64) -> i64,
) -> Result<UsageSummary> {
    let bounds = compute_local_bounds(now_ms, cfg.policy.stats_months, offset_at);
    let min_since = bounds.last_24h_start_ms.min(bounds.four_months_start_ms);
    let mut stmt = store.conn().prepare(
        "SELECT u.model, u.ts_ms, u.input_uncached, u.cache_read, u.cache_write, u.output, u.reasoning
         FROM usage u WHERE u.ts_ms >= ?1 AND u.ts_ms <= ?2",
    )?;
    let rows = stmt.query_map(params![min_since, now_ms], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, i64>(1)?,
            usage_at(r, 2)?,
        ))
    })?;

    let mut b24 = UsageBucket::default();
    let mut bday = UsageBucket::default();
    let mut bmon = UsageBucket::default();
    let mut months: BTreeMap<String, CostAcc> = bounds
        .recent_month_keys
        .iter()
        .map(|k| (k.clone(), CostAcc::new()))
        .collect();

    let mut usd_unknown = [false; 3]; // 24h, hôm nay, tháng này
    let add_bucket =
        |b: &mut UsageBucket, u: &Usage, row_usd: Option<f64>, row_value: Option<f64>| {
            b.usage.add(u);
            if let Some(c) = row_usd {
                b.api_usd = Some(b.api_usd.unwrap_or(0.0) + c);
            }
            match row_value {
                Some(v) => b.value_usd += v,
                None => b.unpriced_tokens += u.total(),
            }
        };

    for row in rows {
        let (model, ts_ms, usage) = row?;
        let (billing, row_cost, row_value) = price_row(cfg, model.as_deref(), &usage);
        // Trả theo API mà chưa khai giá: tổng USD của ô không còn biết được (không số liệu giả).
        let unpriced_api = billing == Some(Billing::Api) && row_cost.is_none();
        let starts = [
            bounds.last_24h_start_ms,
            bounds.today_start_ms,
            bounds.month_start_ms,
        ];
        for ((start, bucket), unknown) in starts
            .into_iter()
            .zip([&mut b24, &mut bday, &mut bmon])
            .zip(usd_unknown.iter_mut())
        {
            if ts_ms >= start {
                add_bucket(bucket, &usage, row_cost, row_value);
                *unknown |= unpriced_api;
            }
        }
        if ts_ms >= bounds.four_months_start_ms {
            let mkey = local_month_key(ts_ms, offset_at);
            if let Some(acc) = months.get_mut(&mkey) {
                acc.add(&usage, billing, row_cost, row_value);
            }
        }
    }

    for (bucket, unknown) in [&mut b24, &mut bday, &mut bmon]
        .into_iter()
        .zip(usd_unknown)
    {
        if unknown {
            bucket.api_usd = None;
        }
    }

    let recent_months = months
        .into_iter()
        .map(|(key, acc)| {
            let empty = acc.usage.total() == 0;
            let mut row = acc.into_row(key);
            if empty {
                row.cost_usd = None;
            }
            row
        })
        .collect();

    let (covered_since_ms, incomplete_coverage) =
        coverage_status(store, bounds.four_months_start_ms)?;
    Ok(UsageSummary {
        now_ms,
        offset_label: bounds.offset_label,
        four_months_start_ms: bounds.four_months_start_ms,
        four_months_start_day: local_day_key(bounds.four_months_start_ms, offset_at),
        last_24h: b24,
        today: bday,
        this_month: bmon,
        recent_months,
        covered_since_ms,
        incomplete_coverage,
    })
}

/// Kiểm tra độ phủ log tệp (`codex`, `claude`, `agy`); `opencode` đọc CSDL nên luôn đủ.
pub fn coverage_status(store: &Store, required_since_ms: i64) -> Result<(Option<i64>, bool)> {
    let mut max_covered: Option<i64> = None;
    let mut incomplete = false;
    for src in ["codex", "claude", "agy"] {
        match store.covered_since(src)? {
            Some(c) => {
                if c > required_since_ms {
                    incomplete = true;
                }
                max_covered = Some(max_covered.map_or(c, |m| m.max(c)));
            }
            None => incomplete = true,
        }
    }
    Ok((max_covered, incomplete))
}

/// Vẽ thanh ngang Unicode (hoặc ASCII fallback) theo cùng scale `max`.
pub fn render_bar(value: u64, max: u64, width: usize, ascii: bool) -> String {
    let filled = if max == 0 || value == 0 {
        0
    } else {
        ((value as f64 / max as f64) * width as f64)
            .round()
            .clamp(1.0, width as f64) as usize
    };
    let (on, off) = if ascii { ('#', '.') } else { ('█', '░') };
    format!(
        "{}{}",
        on.to_string().repeat(filled),
        off.to_string().repeat(width.saturating_sub(filled))
    )
}

/// Ký tự thanh dọc 1 ô cho khối TUI gọn.
pub fn spark_bar(value: u64, max: u64, ascii: bool) -> char {
    if max == 0 || value == 0 {
        return if ascii { '_' } else { '▁' };
    }
    let idx = ((value as f64 / max as f64) * 7.0).round().clamp(1.0, 7.0) as usize;
    if ascii {
        ['_', '.', '-', '=', '+', '*', '#', '#'][idx]
    } else {
        ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][idx]
    }
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
        let used_percent: f64 = r.get(2)?;
        Ok(QuotaRow {
            provider: r.get(0)?,
            window_minutes: r.get(1)?,
            used_percent,
            left_percent: left_percent(used_percent, resets_at_s, now_s),
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
        .query_map(params![prefix], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match found.as_slice() {
        [] => bail!("không tìm thấy phiên có tiền tố '{prefix}'"),
        [(id, _, path)] => Ok((*id, path.clone())),
        many => {
            let ids: Vec<&str> = many.iter().map(|(_, sid, _)| sid.as_str()).collect();
            bail!(
                "tiền tố '{prefix}' khớp {} phiên: {}",
                ids.len(),
                ids.join(", ")
            )
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
