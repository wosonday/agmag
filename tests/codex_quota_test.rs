//! Parse phản hồi `account/rateLimits/read` của `codex app-server` (0 token, không mở lượt hội thoại).

use agmag::codex_quota::{parse_usage, record_usage};
use agmag::store::Store;

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
    wins.sort_by(|a, b| a.0.cmp(&b.0));
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
