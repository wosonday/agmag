//! Hạn mức agy qua `agy --print /usage --output-format json` (.agents/decisions/agy-quota-usage.md).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use agmag::agy_quota::{AgyQuotaPoller, parse_usage, record_usage};
use agmag::config::Config;
use agmag::dashboard::{render, snapshot};
use agmag::model::parse_rfc3339_ms;
use agmag::report;
use agmag::store::Store;

/// Đầu ra thật của `agy --print /usage --output-format json` (2026-11-19), bỏ các trường mô tả.
const PROBE: &str = r#"{"conversation_id":"","status":"SUCCESS","response":"Gemini Models\tWeekly Limit Remaining\t10%\t...","duration_seconds":0,"num_turns":0,
"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0},
"command":{"name":"usage","data":{"groups":[
 {"name":"Gemini Models","buckets":[
  {"id":"gemini-weekly","window":"weekly","remaining_fraction":0.10321377217769623,"reset_time":"2026-11-25T03:24:56Z"},
  {"id":"gemini-5h","window":"5h","remaining_fraction":0.1941382735967636,"reset_time":"2026-11-19T14:13:05Z"}]},
 {"name":"Claude and GPT models","buckets":[
  {"id":"3p-weekly","window":"weekly","remaining_fraction":0.05634640157222748,"reset_time":"2026-11-25T23:48:14Z"},
  {"id":"3p-5h","window":"5h","remaining_fraction":0.12948240339756012,"reset_time":"2026-11-19T12:32:45Z"}]},
 {"name":"Gemini 4 Argon","buckets":[
  {"id":"gemini-argon-wtu-weekly","window":"weekly","remaining_fraction":0.48354414105415344,"reset_time":"2026-11-25T03:25:12Z"}]}]}}}"#;

const NOW_MS: i64 = 1_795_000_000_000;

fn window(p: &agmag::agy_quota::UsageParse, provider: &str, minutes: u32) -> (f64, i64) {
    let snap = p
        .snapshots
        .iter()
        .find(|s| s.provider == provider)
        .unwrap_or_else(|| panic!("no {provider}"));
    let w = snap
        .windows
        .iter()
        .find(|w| w.window_minutes == minutes)
        .unwrap_or_else(|| panic!("no {provider} {minutes}"));
    (w.used_percent, w.resets_at_s)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn parses_three_quota_groups_as_used_percent() -> anyhow::Result<()> {
    let p = parse_usage(PROBE, NOW_MS)?;
    assert_eq!(p.unknown, 0);
    assert_eq!(p.snapshots.len(), 3);
    // agy reports what is LEFT; agm stores what is USED.
    let (used, reset) = window(&p, "agy-gemini", 10080);
    assert!(close(used, 89.68), "{used}");
    assert_eq!(
        reset,
        parse_rfc3339_ms("2026-11-25T03:24:56Z").unwrap() / 1000
    );
    assert!(close(window(&p, "agy-gemini", 300).0, 80.59));
    assert!(close(window(&p, "agy-3p", 10080).0, 94.37));
    assert!(close(window(&p, "agy-3p", 300).0, 87.05));
    // `gemini-argon-` must win over the shorter `gemini-` prefix; Argon has a weekly bucket only.
    assert!(close(window(&p, "agy-argon", 10080).0, 51.65));
    assert_eq!(
        p.snapshots
            .iter()
            .find(|s| s.provider == "agy-argon")
            .unwrap()
            .windows
            .len(),
        1
    );
    assert!(
        p.snapshots
            .iter()
            .all(|s| s.ts_ms == NOW_MS && !s.provider.contains(':'))
    );
    Ok(())
}

#[test]
fn error_status_is_an_error_not_a_zero_or_full_quota() {
    let offline = r#"{"conversation_id":"","status":"ERROR","response":"","error":"/usage failed: retrieving quota summary: dial tcp"}"#;
    let err = parse_usage(offline, NOW_MS).unwrap_err().to_string();
    assert!(err.contains("/usage failed"), "{err}");
    assert!(parse_usage("not json", NOW_MS).is_err());
    assert!(
        parse_usage(r#"{"status":"SUCCESS"}"#, NOW_MS).is_err(),
        "SUCCESS without groups is malformed"
    );
}

#[test]
fn unknown_buckets_are_counted_and_skipped() -> anyhow::Result<()> {
    let odd = r#"{"status":"SUCCESS","command":{"data":{"groups":[{"buckets":[
      {"id":"gemini-5h","window":"5h","remaining_fraction":0.5,"reset_time":"2026-11-19T14:13:05Z"},
      {"id":"mystery-weekly","window":"weekly","remaining_fraction":0.5,"reset_time":"2026-11-25T03:24:56Z"},
      {"id":"gemini-daily","window":"daily","remaining_fraction":0.5,"reset_time":"2026-11-25T03:24:56Z"},
      {"id":"3p-5h","window":"5h","remaining_fraction":1.7,"reset_time":"2026-11-25T03:24:56Z"},
      {"id":"3p-weekly","window":"weekly","remaining_fraction":0.5,"reset_time":"soon"}]}]}}}"#;
    let p = parse_usage(odd, NOW_MS)?;
    assert_eq!(p.unknown, 4);
    assert_eq!(p.snapshots.len(), 1);
    assert_eq!(window(&p, "agy-gemini", 300).0, 50.0);
    Ok(())
}

#[test]
fn disabled_five_hour_bucket_is_skipped_quietly_when_weekly_limit_is_hit() -> anyhow::Result<()> {
    // Real shape (2026-11-20) once the weekly limit is exhausted: the 5h bucket is `disabled`, has no
    // `reset_time`, and "does not currently apply". It is expected, not unknown data.
    let hit = r#"{"status":"SUCCESS","command":{"data":{"groups":[{"name":"Gemini Models","buckets":[
      {"id":"gemini-weekly","window":"weekly","remaining_fraction":0,"reset_time":"2026-11-25T03:24:56Z"},
      {"id":"gemini-5h","window":"5h","disabled":true,"remaining_fraction":0.8736,
       "description":"You have hit your weekly limit, the 5-hour limit does not currently apply."}]}]}}}"#;
    let p = parse_usage(hit, NOW_MS)?;
    assert_eq!(
        p.unknown, 0,
        "a disabled bucket must not raise the 'unrecognised' warning"
    );
    assert_eq!(p.disabled, 1);
    let gem = p
        .snapshots
        .iter()
        .find(|s| s.provider == "agy-gemini")
        .unwrap();
    assert_eq!(gem.windows.len(), 1, "only the weekly window is stored");
    assert_eq!(window(&p, "agy-gemini", 10080).0, 100.0);
    Ok(())
}

#[test]
fn recorded_usage_shows_up_in_quota_report_as_percent_left() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let p = record_usage(&store, PROBE, NOW_MS)?;
    assert_eq!(
        p.snapshots.iter().map(|s| s.windows.len()).sum::<usize>(),
        5
    );
    let rows = report::quota(&store, NOW_MS / 1000)?;
    let gem_week = rows
        .iter()
        .find(|r| r.provider == "agy-gemini" && r.window_minutes == 10080)
        .unwrap();
    assert!(
        close(gem_week.left_percent, 10.32),
        "{}",
        gem_week.left_percent
    );
    let raw = render(&snapshot(&store, &Config::builtin(), NOW_MS)?, 110);
    assert!(
        raw.contains("agy-gemini") && raw.contains("agy-argon"),
        "provider names must not be cut:\n{raw}"
    );
    Ok(())
}

fn wait_for_rows(store: &Store, poller: &mut AgyQuotaPoller, now_ms: i64) -> anyhow::Result<usize> {
    for _ in 0..200 {
        poller.tick(store, now_ms)?;
        let n = report::quota(store, now_ms / 1000)?.len();
        if n > 0 {
            return Ok(n);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(0)
}

#[test]
fn poller_fetches_in_background_and_respects_interval() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let mut poller = AgyQuotaPoller::new(
        15,
        Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(PROBE.to_string())
        }),
    );
    assert_eq!(wait_for_rows(&store, &mut poller, NOW_MS)?, 5);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // Within the 15-minute interval: no new call.
    poller.tick(&store, NOW_MS + 14 * 60_000)?;
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // After the interval: one more call.
    poller.tick(&store, NOW_MS + 15 * 60_000)?;
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn poller_is_off_at_zero_and_survives_fetch_errors() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let mut off = AgyQuotaPoller::new(
        0,
        Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(PROBE.to_string())
        }),
    );
    off.tick(&store, NOW_MS)?;
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "agy_quota_poll_min = 0 disables polling"
    );

    let mut failing = AgyQuotaPoller::new(15, Arc::new(|| anyhow::bail!("agy not installed")));
    assert_eq!(wait_for_rows(&store, &mut failing, NOW_MS)?, 0);
    failing.tick(&store, NOW_MS + 15 * 60_000)?; // errors never propagate out of tick
    Ok(())
}

#[test]
fn policy_default_polls_every_15_minutes() {
    assert_eq!(Config::builtin().policy.agy_quota_poll_min, 15);
}
