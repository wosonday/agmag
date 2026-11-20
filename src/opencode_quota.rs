//! Hạn mức gói OpenCode Go qua `opencode api GET /api/experimental/usage/limits`.
//!
//! - Không tốn token, không mở lượt hội thoại: endpoint đọc hạn mức tài khoản.
//! - `percent` trong API là phần **đã dùng**; agm lưu nguyên (kẹp 0–100).
//! - Provider: `opencode-go`; cửa sổ 300 (rolling/5h), 10080 (weekly/7d), 43200 (monthly/30d).
//! - `data: null` / thiếu `go` / thiếu trường / meter lạ → bỏ qua phần đó, đếm unknown,
//!   **không ghi 0% hay 100%** thay dữ liệu thiếu. JSON hỏng → lỗi.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::model::{QuotaSnapshot, QuotaWindow};
use crate::store::Store;

#[derive(Debug, Default)]
pub struct UsageParse {
    pub snapshots: Vec<QuotaSnapshot>,
    pub unknown: usize,
    /// Cờ `limited` theo cửa sổ (phút) cho các meter đọc được.
    pub limited: std::collections::BTreeMap<u32, bool>,
}

/// Đọc JSON trả về của `GET /api/experimental/usage/limits`.
pub fn parse_usage(stdout: &str, now_ms: i64) -> Result<UsageParse> {
    let v: Value =
        serde_json::from_str(stdout.trim()).context("opencode usage/limits không trả JSON")?;
    let data = &v["data"];
    // Chưa đăng nhập → không ghi gì.
    if data.is_null() {
        return Ok(UsageParse::default());
    }
    let Some(go) = data.get("go").and_then(|g| g.as_object()) else {
        // Thiếu `go` → không có dữ liệu hợp lệ, đếm unknown.
        return Ok(UsageParse {
            unknown: 1,
            ..UsageParse::default()
        });
    };

    let mut windows: Vec<QuotaWindow> = Vec::new();
    let mut limited: std::collections::BTreeMap<u32, bool> = std::collections::BTreeMap::new();
    let mut unknown = 0usize;
    let known: [(&str, u32); 3] = [("rolling", 300), ("weekly", 10080), ("monthly", 43200)];

    for key in go.keys() {
        if !known.iter().any(|(name, _)| name == key) {
            unknown += 1;
        }
    }
    for (name, mins) in known {
        let Some(meter) = go.get(name) else {
            continue;
        };
        if meter.is_null() {
            unknown += 1;
            continue;
        }
        let percent = meter.get("percent").and_then(|p| p.as_f64());
        let resets_at_ms = meter.get("resetsAt").and_then(|r| r.as_i64());
        let is_limited = meter.get("limited").and_then(|l| l.as_bool());
        let (Some(percent), Some(resets_at_ms), Some(is_limited)) =
            (percent, resets_at_ms, is_limited)
        else {
            unknown += 1;
            continue;
        };
        let used_percent = percent.clamp(0.0, 100.0);
        windows.push(QuotaWindow {
            window_minutes: mins,
            used_percent,
            resets_at_s: resets_at_ms / 1000,
        });
        limited.insert(mins, is_limited);
    }

    let snapshots = if windows.is_empty() {
        Vec::new()
    } else {
        vec![QuotaSnapshot {
            provider: "opencode-go".into(),
            ts_ms: now_ms,
            windows,
        }]
    };
    Ok(UsageParse {
        snapshots,
        unknown,
        limited,
    })
}

pub fn record_usage(store: &Store, stdout: &str, now_ms: i64) -> Result<UsageParse> {
    let parsed = parse_usage(stdout, now_ms)?;
    for snap in &parsed.snapshots {
        store.insert_quota(snap)?;
    }
    // Lưu cờ limited theo cửa sổ để hiển thị (quota table không có cột này).
    for (&mins, &is_limited) in &parsed.limited {
        let key = format!("quota_limited:opencode-go:{mins}");
        store.set_setting(&key, if is_limited { "1" } else { "0" })?;
    }
    Ok(parsed)
}

/// Hạn chót chung cho toàn bộ một lần đọc hạn mức (5 s, có dư khi test).
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

pub fn fetch_usage() -> Result<String> {
    let bin = std::env::var("AGM_OPENCODE_BIN").unwrap_or_else(|_| "opencode".into());
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let bins: Vec<String> = if std::env::var_os("AGM_OPENCODE_BIN").is_some() {
        vec![bin]
    } else if cfg!(windows) {
        vec!["opencode".into(), "opencode.cmd".into()]
    } else {
        vec![bin]
    };
    let mut last_err = None;
    for b in bins {
        match fetch_with(&b, deadline) {
            Ok(text) => return Ok(text),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("không tìm thấy opencode")))
}

fn fetch_with(bin: &str, deadline: Instant) -> Result<String> {
    let mut child = Command::new(bin)
        .args(["api", "GET", "/api/experimental/usage/limits"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("chạy `{bin} api GET /api/experimental/usage/limits`"))?;
    let mut stdout = child
        .stdout
        .take()
        .context("stdout opencode usage/limits")?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut buf = String::new();
        let read = stdout.read_to_string(&mut buf).map(|_| buf);
        let _ = tx.send(read);
    });
    let remaining = deadline.saturating_duration_since(Instant::now());
    let text = match rx.recv_timeout(remaining) {
        Ok(Ok(text)) => text,
        Ok(Err(err)) => {
            kill_tree(&mut child);
            return Err(err.into());
        }
        Err(_) => {
            // Không join luồng đọc: tiến trình con còn giữ stdout có thể chặn mãi.
            kill_tree(&mut child);
            bail!(
                "`{bin} api GET /api/experimental/usage/limits` quá thời hạn sau {}s",
                FETCH_TIMEOUT.as_secs()
            );
        }
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                bail!(
                    "`{bin} api GET /api/experimental/usage/limits` quá thời hạn sau {}s",
                    FETCH_TIMEOUT.as_secs()
                );
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(err) => {
                kill_tree(&mut child);
                return Err(err.into());
            }
        }
    };
    let _ = reader.join();
    if !status.success() {
        bail!("`{bin} api GET /api/experimental/usage/limits` thoát với {status}");
    }
    Ok(text)
}

/// Diệt cả cây tiến trình (shim `.cmd`/wrapper có thể giữ con giữ stdout).
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", pid.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub type Fetch = Arc<dyn Fn() -> Result<String> + Send + Sync>;

pub struct OpenCodeQuotaPoller {
    interval_ms: i64,
    fetch: Fetch,
    last_start_ms: Option<i64>,
    pending: Option<Receiver<Result<String>>>,
    last_error: Option<String>,
}

impl OpenCodeQuotaPoller {
    pub fn new(interval_min: u64, fetch: Fetch) -> Self {
        let interval_ms = i64::try_from(interval_min)
            .unwrap_or(i64::MAX / 60_000)
            .saturating_mul(60_000);
        OpenCodeQuotaPoller {
            interval_ms,
            fetch,
            last_start_ms: None,
            pending: None,
            last_error: None,
        }
    }

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
        let due = self
            .last_start_ms
            .is_none_or(|last| now_ms - last >= self.interval_ms);
        if due && self.pending.is_none() {
            let (tx, rx) = mpsc::channel();
            let fetch = self.fetch.clone();
            thread::spawn(move || {
                let _ = tx.send(fetch());
            });
            self.pending = Some(rx);
            self.last_start_ms = Some(now_ms);
        }
        Ok(())
    }

    fn warn(&mut self, msg: String) {
        if self.last_error.as_deref() != Some(msg.as_str()) {
            eprintln!("agm: cảnh báo hạn mức opencode-go: {msg}");
            self.last_error = Some(msg);
        }
    }
}
