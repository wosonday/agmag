use std::fs;

use agmag::hook::statusline;
use agmag::ingest::{ingest_file, path_key};
use agmag::model::Source;
use agmag::store::Store;
use serde_json::json;

fn input(transcript: &str) -> serde_json::Value {
    json!({
        "session_id": "s-1",
        "transcript_path": transcript,
        "model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
        "context_window": {"context_window_size": 1000000, "used_percentage": 34.4},
        "rate_limits": {
            "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
            "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}
        }
    })
}

#[test]
fn records_claude_quota() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    statusline(&store, &input("x.jsonl"), None, 5000)?;
    let rows: Vec<(String, i64, f64, i64)> = store
        .conn()
        .prepare("SELECT provider, window_minutes, used_percent, resets_at_s FROM quota ORDER BY window_minutes")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    assert_eq!(
        rows,
        [
            ("claude".into(), 300, 23.5, 1738425600),
            ("claude".into(), 10080, 41.2, 1738857600)
        ]
    );
    Ok(())
}

#[test]
fn sets_ctx_window_for_transcript_and_links_terminal() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("s-1.jsonl");
    fs::copy("tests/fixtures/claude/mini.jsonl", &path)?;
    let store = Store::open_in_memory()?;
    ingest_file(&store, Source::Claude, &path)?;
    // Claude có thể gửi đường dẫn với dấu '/' và chữ hoa khác: vẫn phải khớp đúng một phiên.
    let odd = path.to_string_lossy().replace('\\', "/").to_uppercase();
    statusline(&store, &input(&odd), Some("t1s"), 5000)?;
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    assert_eq!(n, 1);
    let w: Option<i64> = store
        .conn()
        .query_row("SELECT ctx_window FROM sessions", [], |r| r.get(0))?;
    assert_eq!(w, Some(1000000));
    let t = store.terminal("t1s")?.unwrap();
    assert_eq!(
        (t.link_source.as_deref(), t.app.as_deref()),
        (Some("hook"), Some("claude-code"))
    );
    Ok(())
}

#[test]
fn missing_rate_limits_ok_and_prints_compact_line() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut v = input("x.jsonl");
    v.as_object_mut().unwrap().remove("rate_limits");
    let line = statusline(&store, &v, None, 5000)?;
    assert_eq!(line, "Opus 5.5 · ctx 34%");
    let full = statusline(&store, &input("x.jsonl"), None, 5000)?;
    // Plan quota shows what is LEFT (23.5% used → 76% left; 41.2% used → 59% left); ctx stays "used".
    assert_eq!(full, "Opus 5.5 · ctx 34% · 5h 76% left · 7d 59% left");
    Ok(())
}

#[test]
fn path_key_normalizes_on_windows() {
    if cfg!(windows) {
        assert_eq!(
            path_key(std::path::Path::new("C:/Users/A/x.jsonl")),
            path_key(std::path::Path::new(r"c:\users\a\X.JSONL"))
        );
    }
}
