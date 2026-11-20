//! Parse phản hồi `account/rateLimits/read` của `codex app-server` (0 token, không mở lượt hội thoại).

use agmag::codex_quota::{fetch_usage, parse_usage, record_usage};
use agmag::store::Store;
use std::sync::Mutex;
use std::time::Instant;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Phản hồi `result` đúng dạng schema `GetAccountRateLimitsResponse` của codex-cli 0.170.0
/// (`rateLimits` = single-bucket view; `primary`/`secondary` = `RateLimitWindow`).
const RATE_LIMITS: &str = r#"{
  "rateLimits": {
    "primary": { "usedPercent": 42, "windowDurationMins": 300, "resetsAt": 1795000000 },
    "secondary": { "usedPercent": 16, "windowDurationMins": 10080, "resetsAt": 1795400000 }
  }
}"#;

#[test]
fn parse_rate_limits_two_windows() {
    let now_ms = 1_800_000_000_000;
    let parsed = parse_usage(RATE_LIMITS, now_ms).expect("phản hồi hợp lệ");
    assert_eq!(parsed.snapshots.len(), 1);
    let snap = &parsed.snapshots[0];
    assert_eq!(snap.provider, "codex");
    assert_eq!(snap.ts_ms, now_ms);
    let mut wins: Vec<(u32, f64)> = snap
        .windows
        .iter()
        .map(|w| (w.window_minutes, w.used_percent))
        .collect();
    wins.sort_by_key(|w| w.0);
    assert_eq!(wins, [(300, 42.0), (10080, 16.0)]);
}

#[test]
fn parse_jsonrpc_envelope_and_bare_result() {
    let now_ms = 42;
    // Bọc JSON-RPC đầy đủ (những gì `fetch_usage` nhận được qua stdout).
    let wrapped = format!(r#"{{"jsonrpc":"2.0","id":2,"result":{RATE_LIMITS}}}"#);
    assert_eq!(
        parse_usage(&wrapped, now_ms).expect("envelope").snapshots[0]
            .windows
            .len(),
        2
    );
    // Chỉ phần result (như `fetch_usage` đã bóc) vẫn parse được.
    assert_eq!(
        parse_usage(RATE_LIMITS, now_ms).expect("bare").snapshots[0]
            .windows
            .len(),
        2
    );
}

#[test]
fn unknown_buckets_are_skipped_not_guessed() {
    let now_ms = 42;
    // Cửa sổ lạ (không phải 300/10080) và phần trăm ngoài 0..=100 → bỏ, không đoán.
    let junk = r#"{
      "rateLimits": {
        "primary": { "usedPercent": 42, "windowDurationMins": 60, "resetsAt": 1 },
        "secondary": { "usedPercent": 500, "windowDurationMins": 10080, "resetsAt": 2 }
      }
    }"#;
    let err = parse_usage(junk, now_ms).unwrap_err();
    assert!(
        err.to_string().contains("hạn mức hợp lệ"),
        "không có cửa sổ hợp lệ phải là lỗi: {err:#}"
    );
}

#[test]
fn bad_input_records_nothing() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    // JSON rác → lỗi, không ghi gì.
    assert!(record_usage(&store, "không phải json", 42).is_err());
    // Thiếu `rateLimits` → lỗi, không ghi gì.
    assert!(record_usage(&store, r#"{"foo":1}"#, 42).is_err());
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM quota", [], |r| r.get(0))?;
    assert_eq!(n, 0, "lỗi không được phép ghi 0% hay 100% vào bảng quota");
    Ok(())
}

#[test]
fn fetch_usage_hanging_binary_times_out_and_kills_tree() -> anyhow::Result<()> {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir()?;
    #[cfg(windows)]
    let (bin, content) = {
        let p = dir.path().join("fake-codex.cmd");
        // Treo giữ stdout mở, bỏ qua mọi đối số.
        (
            p,
            "@echo off\r\npowershell -NoProfile -Command \"Start-Sleep -Seconds 999\"\r\n",
        )
    };
    #[cfg(not(windows))]
    let (bin, content) = {
        let p = dir.path().join("fake-codex");
        (p, "#!/bin/sh\nsleep 999\n")
    };
    std::fs::write(&bin, content)?;
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
    }

    let old = std::env::var_os("AGM_CODEX_BIN");
    // SAFETY: serialized by ENV_LOCK; test process only.
    unsafe { std::env::set_var("AGM_CODEX_BIN", &bin) };
    let start = Instant::now();
    let err = fetch_usage().expect_err("binary treo phải trả lỗi");
    let elapsed = start.elapsed();
    match old {
        Some(v) => unsafe { std::env::set_var("AGM_CODEX_BIN", v) },
        None => unsafe { std::env::remove_var("AGM_CODEX_BIN") },
    }

    assert!(
        elapsed.as_secs() <= 8,
        "phải quá hạn trong ≤ 8 s, thực tế {elapsed:?}: {err:#}"
    );
    let msg = format!("{err:#}");
    assert!(
        msg.contains("quá thời hạn") || msg.contains("timed out") || msg.contains("timeout"),
        "lỗi phải báo quá thời hạn: {msg}"
    );
    Ok(())
}
