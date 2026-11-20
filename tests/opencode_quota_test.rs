use agmag::opencode_quota::parse_usage;
use agmag::store::Store;

const SAMPLE: &str = r#"{
  "data": {
    "workspace": { "id": "org_x", "name": "x" },
    "go": {
      "rolling": { "percent": 0, "limited": false, "resetsAt": 1795156077000 },
      "weekly":  { "percent": 40, "limited": false, "resetsAt": 1795392000000 },
      "monthly": { "percent": 20, "limited": false, "resetsAt": 1797728748000 }
    },
    "budgets": []
  }
}"#;

#[test]
fn parse_three_meters_correct_windows() {
    let now_ms = 1_800_000_000_000_i64;
    let p = parse_usage(SAMPLE, now_ms).expect("valid sample");
    assert_eq!(p.snapshots.len(), 1);
    let snap = &p.snapshots[0];
    assert_eq!(snap.provider, "opencode-go");
    assert_eq!(snap.ts_ms, now_ms);
    let mut wins: Vec<(u32, f64, i64)> = snap
        .windows
        .iter()
        .map(|w| (w.window_minutes, w.used_percent, w.resets_at_s))
        .collect();
    wins.sort_by_key(|w| w.0);
    assert_eq!(
        wins,
        vec![
            (300, 0.0, 1_795_156_077),
            (10080, 40.0, 1_795_392_000),
            (43200, 20.0, 1_797_728_748),
        ]
    );
    assert_eq!(p.unknown, 0);
}

#[test]
fn data_null_records_nothing() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let p = agmag::opencode_quota::parse_usage(r#"{"data":null}"#, 42)?;
    assert!(p.snapshots.is_empty() || p.snapshots.iter().all(|s| s.windows.is_empty()));
    agmag::opencode_quota::record_usage(&store, r#"{"data":null}"#, 42)?;
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM quota", [], |r| r.get(0))?;
    assert_eq!(n, 0);
    Ok(())
}

#[test]
fn missing_go_and_unknown_meter_counted_not_written_as_fake() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    // missing go
    let p = parse_usage(r#"{"data":{"workspace":{}}}"#, 1)?;
    assert!(p.snapshots.is_empty() || p.snapshots.iter().all(|s| s.windows.is_empty()));
    assert!(p.unknown >= 1);
    agmag::opencode_quota::record_usage(&store, r#"{"data":{"workspace":{}}}"#, 1)?;
    // unknown meter name and missing fields on known meter
    let weird = r#"{"data":{"go":{"rolling":{"percent":10,"limited":false,"resetsAt":1000},"yearly":{"percent":1,"limited":false,"resetsAt":2000},"weekly":{"percent":5,"limited":false}}}}"#;
    let p2 = parse_usage(weird, 2)?;
    assert_eq!(
        p2.snapshots[0].windows.len(),
        1,
        "only rolling valid; weekly missing resetsAt, yearly unknown"
    );
    assert!(p2.unknown >= 2);
    let n: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM quota WHERE provider='opencode-go'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(n, 0);
    Ok(())
}

#[test]
fn percent_out_of_range_clamped_and_bad_json_errors() -> anyhow::Result<()> {
    let clamped = r#"{"data":{"go":{"rolling":{"percent":150,"limited":true,"resetsAt":5000},"weekly":{"percent":-5,"limited":false,"resetsAt":6000},"monthly":{"percent":20,"limited":false,"resetsAt":7000}}}}"#;
    let p = parse_usage(clamped, 3)?;
    let mut wins: Vec<(u32, f64)> = p.snapshots[0]
        .windows
        .iter()
        .map(|w| (w.window_minutes, w.used_percent))
        .collect();
    wins.sort_by_key(|w| w.0);
    assert_eq!(wins, vec![(300, 100.0), (10080, 0.0), (43200, 20.0)]);

    let store = Store::open_in_memory()?;
    assert!(agmag::opencode_quota::record_usage(&store, "not json", 4).is_err());
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM quota", [], |r| r.get(0))?;
    assert_eq!(n, 0);
    Ok(())
}

use std::sync::Mutex;
use std::time::Instant;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn fetch_usage_hanging_binary_times_out_and_kills_tree() -> anyhow::Result<()> {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir()?;
    #[cfg(windows)]
    let (bin, content) = {
        let p = dir.path().join("fake-opencode.cmd");
        (
            p,
            "@echo off\r\npowershell -NoProfile -Command \"Start-Sleep -Seconds 999\"\r\n",
        )
    };
    #[cfg(not(windows))]
    let (bin, content) = {
        let p = dir.path().join("fake-opencode");
        (p, "#!/bin/sh\nsleep 999\n")
    };
    std::fs::write(&bin, content)?;
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
    }

    let old = std::env::var_os("AGM_OPENCODE_BIN");
    unsafe { std::env::set_var("AGM_OPENCODE_BIN", &bin) };
    let start = Instant::now();
    let err = agmag::opencode_quota::fetch_usage().expect_err("binary treo phải trả lỗi");
    let elapsed = start.elapsed();
    match old {
        Some(v) => unsafe { std::env::set_var("AGM_OPENCODE_BIN", v) },
        None => unsafe { std::env::remove_var("AGM_OPENCODE_BIN") },
    }

    assert!(
        elapsed.as_secs() <= 8,
        "phải quá hạn trong ≤ 8 s (hạn chót 5 s + dư), thực tế {elapsed:?}: {err:#}"
    );
    let msg = format!("{err:#}");
    assert!(
        msg.contains("quá thời hạn") || msg.contains("timeout") || msg.contains("timed out"),
        "lỗi phải báo quá thời hạn: {msg}"
    );
    Ok(())
}
