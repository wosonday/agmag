//! Hạn mức gói của Antigravity CLI (`agy`) qua `agy --print /usage --output-format json`.
//!
//! - Không tốn token hay hạn mức, không tạo hội thoại (đã kiểm 2026-11-20, xem
//!   `.agents/decisions/agy-quota-usage.md`). Mỗi lần gọi mất khoảng 3 giây, nên daemon gọi thưa
//!   (`[policy].agy_quota_poll_min`, mặc định 15 phút) và trên luồng riêng.
//! - agy báo phần **còn lại** (`remaining_fraction`); agm lưu phần **đã dùng** như Codex/Claude.
//! - Ba nhóm hạn mức độc lập → ba provider: `agy-gemini`, `agy-3p` (Claude/GPT qua agy, KHÁC hạn mức
//!   Claude Code), `agy-argon`. Tên không chứa `:` vì phạm vi ngân sách tách bằng `:`.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::model::{parse_rfc3339_ms, QuotaSnapshot, QuotaWindow};
use crate::store::Store;

/// Tiền tố `bucket.id` → provider. Tiền tố dài trước (`gemini-argon-` trước `gemini-`).
const PROVIDERS: [(&str, &str); 3] = [("gemini-argon-", "agy-argon"), ("gemini-", "agy-gemini"), ("3p-", "agy-3p")];

/// Kết quả đọc một lần `/usage`.
#[derive(Debug, Default)]
pub struct UsageParse {
    pub snapshots: Vec<QuotaSnapshot>,
    /// Bucket bỏ qua vì id, cửa sổ, tỷ lệ hoặc thời điểm reset không nhận ra.
    pub unknown: usize,
    /// Bucket agy đánh dấu `disabled` (vd hết hạn mức tuần → "the 5-hour limit does not currently apply";
    /// không có `reset_time`). Bỏ qua có chủ đích, không cảnh báo.
    pub disabled: usize,
}

fn provider_for(id: &str) -> Option<&'static str> {
    PROVIDERS.iter().find(|(prefix, _)| id.starts_with(prefix)).map(|(_, p)| *p)
}

fn window_minutes(window: &str) -> Option<u32> {
    match window {
        "5h" => Some(300),
        "weekly" => Some(10080),
        _ => None,
    }
}

/// Đọc stdout của `agy --print /usage --output-format json`. `status != "SUCCESS"` hoặc JSON sai dạng → lỗi
/// (không bao giờ đoán 0% hay 100%).
pub fn parse_usage(stdout: &str, now_ms: i64) -> Result<UsageParse> {
    let v: Value = serde_json::from_str(stdout.trim()).context("agy /usage không trả JSON")?;
    if v["status"].as_str() != Some("SUCCESS") {
        bail!("agy /usage lỗi: {}", v["error"].as_str().unwrap_or("không rõ lỗi"));
    }
    let Some(groups) = v["command"]["data"]["groups"].as_array() else {
        bail!("agy /usage thiếu command.data.groups");
    };
    let mut by_provider: BTreeMap<&str, Vec<QuotaWindow>> = BTreeMap::new();
    let (mut unknown, mut disabled) = (0, 0);
    for bucket in groups.iter().filter_map(|g| g["buckets"].as_array()).flatten() {
        if bucket["disabled"].as_bool() == Some(true) {
            disabled += 1;
            continue;
        }
        let provider = bucket["id"].as_str().and_then(provider_for);
        let minutes = bucket["window"].as_str().and_then(window_minutes);
        let remaining = bucket["remaining_fraction"].as_f64().filter(|r| r.is_finite() && (0.0..=1.0).contains(r));
        let reset_ms = bucket["reset_time"].as_str().and_then(parse_rfc3339_ms);
        let (Some(provider), Some(window_minutes), Some(remaining), Some(reset_ms)) = (provider, minutes, remaining, reset_ms)
        else {
            unknown += 1;
            continue;
        };
        by_provider.entry(provider).or_default().push(QuotaWindow {
            window_minutes,
            used_percent: ((1.0 - remaining) * 100.0).clamp(0.0, 100.0),
            resets_at_s: reset_ms / 1000,
        });
    }
    let snapshots = by_provider
        .into_iter()
        .map(|(provider, windows)| QuotaSnapshot { provider: provider.into(), ts_ms: now_ms, windows })
        .collect();
    Ok(UsageParse { snapshots, unknown, disabled })
}

/// Đọc rồi ghi vào bảng `quota` (dùng chung với Codex/Claude: `agm quota`, TUI, ngân sách `provider:`).
pub fn record_usage(store: &Store, stdout: &str, now_ms: i64) -> Result<UsageParse> {
    let parsed = parse_usage(stdout, now_ms)?;
    for snap in &parsed.snapshots {
        store.insert_quota(snap)?;
    }
    Ok(parsed)
}

/// Chạy `agy --print /usage` một lần trong thư mục tạm ngoài repo; trả stdout (kể cả JSON lỗi).
pub fn fetch_usage() -> Result<String> {
    let dir = crate::ask::ask_workdir();
    std::fs::create_dir_all(&dir)?;
    let out = Command::new(std::env::var("AGM_AGY_BIN").unwrap_or_else(|_| "agy".into()))
        .current_dir(&dir)
        .args(["--print", "/usage", "--output-format", "json", "--print-timeout", "20s"])
        .output()
        .context("chạy agy --print /usage")?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if stdout.trim().is_empty() {
        bail!(
            "agy /usage ({}) không in gì: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>()
        );
    }
    Ok(stdout)
}

/// Hàm lấy stdout của `/usage`; thay được trong test.
pub type Fetch = Arc<dyn Fn() -> Result<String> + Send + Sync>;

/// Gọi `/usage` định kỳ trên luồng riêng để vòng lặp daemon (sự kiện `hc`) không bị chặn vài giây.
pub struct AgyQuotaPoller {
    interval_ms: i64,
    fetch: Fetch,
    last_start_ms: Option<i64>,
    pending: Option<Receiver<Result<String>>>,
    last_error: Option<String>,
}

impl AgyQuotaPoller {
    /// `interval_min = 0` → tắt.
    pub fn new(interval_min: u64, fetch: Fetch) -> Self {
        let interval_ms = i64::try_from(interval_min).unwrap_or(i64::MAX / 60_000).saturating_mul(60_000);
        AgyQuotaPoller { interval_ms, fetch, last_start_ms: None, pending: None, last_error: None }
    }

    /// Một nhịp: ghi kết quả của lượt trước (nếu đã xong), rồi bắt đầu lượt mới khi đến hạn.
    /// Lỗi lấy/đọc `/usage` chỉ cảnh báo (mỗi loại lỗi một lần), không làm daemon dừng.
    pub fn tick(&mut self, store: &Store, now_ms: i64) -> Result<()> {
        if self.interval_ms <= 0 {
            return Ok(());
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(res) => {
                    self.pending = None;
                    match res.and_then(|out| record_usage(store, &out, now_ms)) {
                        Ok(_) => self.last_error = None,
                        Err(e) => self.warn(format!("{e:#}")),
                    }
                }
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => self.pending = None,
            }
        }
        let due = self.last_start_ms.is_none_or(|last| now_ms - last >= self.interval_ms);
        if due && self.pending.is_none() {
            let (tx, rx) = mpsc::channel();
            let fetch = self.fetch.clone();
            std::thread::spawn(move || {
                let _ = tx.send(fetch());
            });
            self.pending = Some(rx);
            self.last_start_ms = Some(now_ms);
        }
        Ok(())
    }

    fn warn(&mut self, msg: String) {
        if self.last_error.as_deref() != Some(msg.as_str()) {
            eprintln!("agm: cảnh báo hạn mức agy: {msg}");
            self.last_error = Some(msg);
        }
    }
}
