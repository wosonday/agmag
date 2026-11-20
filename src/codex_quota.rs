//! Hạn mức gói của Codex qua `codex app-server` (JSON-RPC stdio, phương thức `account/rateLimits/read`).
//!
//! - Không tốn token, không mở lượt hội thoại: phương thức chỉ đọc hạn mức tài khoản (đã kiểm
//!   2026-11-20, xem `.agents/decisions/codex-quota-source.md`).
//! - Codex báo phần **đã dùng** (`usedPercent`), giống log `token_count.rate_limits`; agm lưu nguyên.
//! - Một provider: `codex`, hai cửa sổ 300 phút (5h) và 10080 phút (7d).
//!
//! Lỗi/không đăng nhập/timeout → không ghi gì, cảnh báo một lần, không bao giờ ghi 0% hay 100%.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
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
    pub disabled: usize,
}

/// Đọc kết quả JSON-RPC của `account/rateLimits/read` (phần `result`).
pub fn parse_usage(stdout: &str, now_ms: i64) -> Result<UsageParse> {
    let v: Value =
        serde_json::from_str(stdout.trim()).context("codex app-server không trả JSON")?;
    // Chấp nhận cả phản hồi JSON-RPC đầy đủ hoặc chỉ phần result.
    let result = if v.get("result").is_some() {
        &v["result"]
    } else {
        &v
    };
    let snap_val = result["rateLimits"]
        .as_object()
        .or_else(|| result.as_object())
        .context("codex app-server thiếu rateLimits")?;
    // Nếu input là snapshot đơn (không bọc), dùng chính nó.
    let snap = if result["rateLimits"].is_object() {
        &result["rateLimits"]
    } else {
        &v
    };
    let _ = snap_val;

    let mut windows: Vec<QuotaWindow> = Vec::new();
    for key in ["primary", "secondary"] {
        let Some(w) = snap.get(key) else { continue };
        if w.is_null() {
            continue;
        }
        let used = w["usedPercent"]
            .as_f64()
            .or_else(|| w["usedPercent"].as_i64().map(|x| x as f64));
        let mins = w["windowDurationMins"].as_i64();
        let reset = w["resetsAt"].as_i64();
        let (Some(used), Some(mins), Some(reset)) = (used, mins, reset) else {
            // unknown bucket
            continue;
        };
        if mins != 300 && mins != 10080 {
            continue;
        }
        if !(0.0..=100.0).contains(&used) {
            continue;
        }
        windows.push(QuotaWindow {
            window_minutes: mins as u32,
            used_percent: used,
            resets_at_s: reset,
        });
    }
    if windows.is_empty() {
        bail!("codex app-server không có cửa sổ hạn mức hợp lệ");
    }
    let snapshots = vec![QuotaSnapshot {
        provider: "codex".into(),
        ts_ms: now_ms,
        windows,
    }];
    Ok(UsageParse {
        snapshots,
        unknown: 0,
        disabled: 0,
    })
}

pub fn record_usage(store: &Store, stdout: &str, now_ms: i64) -> Result<UsageParse> {
    let parsed = parse_usage(stdout, now_ms)?;
    for snap in &parsed.snapshots {
        store.insert_quota(snap)?;
    }
    Ok(parsed)
}

/// Hạn chót chung cho toàn bộ một lần đọc hạn mức: khởi động, đọc phản hồi, thoát.
/// Công khai để test treo dùng chung con số này thay vì hard-code 8 giây.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

pub fn fetch_usage() -> Result<String> {
    let bin = std::env::var("AGM_CODEX_BIN").unwrap_or_else(|_| "codex".into());
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let mut child = Command::new(&bin)
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("chạy codex app-server")?;

    let res = fetch_usage_inner(&mut child, &bin, deadline);
    if res.is_err() {
        // Quá hạn/lỗi: diệt cả cây tiến trình; không join luồng đọc (con có thể còn giữ stdout).
        kill_tree(&mut child);
    } else {
        let _ = child.kill();
        let _ = child.wait();
    }
    res
}

fn fetch_usage_inner(child: &mut Child, bin: &str, deadline: Instant) -> Result<String> {
    let mut stdin = child.stdin.take().context("stdin codex app-server")?;
    let stdout = child.stdout.take().context("stdout codex app-server")?;
    let (tx, rx) = mpsc::channel::<Result<String>>();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(Ok(l)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.into()));
                    break;
                }
            }
        }
    });

    let write = |stdin: &mut dyn Write, obj: &Value| -> Result<()> {
        let line = serde_json::to_string(obj)?;
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    };

    // initialize
    write(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"agm","version":"0.1"}}}),
    )?;
    wait_for_id(&rx, 1, deadline, bin)?;
    // account/rateLimits/read
    write(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":{}}),
    )?;
    let resp_line = wait_for_id(&rx, 2, deadline, bin)?;
    let v: Value =
        serde_json::from_str(resp_line.trim()).context("phản hồi codex không phải JSON")?;

    if let Some(err) = v.get("error") {
        bail!("codex app-server lỗi: {}", err);
    }
    let result = v.get("result").context("codex app-server thiếu result")?;
    Ok(serde_json::to_string(result)?)
}

fn wait_for_id(
    rx: &Receiver<Result<String>>,
    id: i64,
    deadline: Instant,
    bin: &str,
) -> Result<String> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            bail!("codex app-server `{bin}` quá thời hạn chờ phản hồi id={id}");
        }
        let remaining = deadline.saturating_duration_since(now);
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(Ok(line)) => {
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && v.get("id").and_then(|x| x.as_i64()) == Some(id)
                {
                    return Ok(line);
                }
            }
            Ok(Err(e)) => return Err(e).context("đọc stdout codex app-server"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                bail!("codex app-server đã dừng")
            }
        }
    }
}

/// Diệt cả cây tiến trình (shim `.cmd`/wrapper có thể giữ con giữ stdout).
fn kill_tree(child: &mut Child) {
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

pub struct CodexQuotaPoller {
    interval_ms: i64,
    fetch: Fetch,
    last_start_ms: Option<i64>,
    pending: Option<Receiver<Result<String>>>,
    last_error: Option<String>,
}

impl CodexQuotaPoller {
    pub fn new(interval_min: u64, fetch: Fetch) -> Self {
        let interval_ms = i64::try_from(interval_min)
            .unwrap_or(i64::MAX / 60_000)
            .saturating_mul(60_000);
        CodexQuotaPoller {
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
            eprintln!("agm: cảnh báo hạn mức codex: {msg}");
            self.last_error = Some(msg);
        }
    }
}
