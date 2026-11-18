use agmag::model::{
    ContextSnapshot, QuotaSnapshot, QuotaWindow, Role, SessionMeta, Source, TurnSummary, Usage,
    UsageRecord,
};
use agmag::store::{Cursor, Store};

#[test]
fn writes_inside_outer_transaction() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    store.conn().execute_batch("BEGIN")?;
    let sid = store.session_id_for_path(Source::Codex, "nested")?;
    store.upsert_usage(sid, &record(100, 20))?;
    store.insert_quota(&QuotaSnapshot {
        provider: "codex".into(),
        ts_ms: 100,
        windows: vec![QuotaWindow {
            window_minutes: 300,
            used_percent: 12.0,
            resets_at_s: 200,
        }],
    })?;
    store.insert_turn(
        sid,
        &TurnSummary {
            ts_ms: 100,
            role: Role::User,
            summary: "hello".into(),
            byte_offset: 0,
        },
    )?;
    store.conn().execute_batch("COMMIT")?;
    let counts: (i64, i64, i64) = store.conn().query_row(
        "SELECT (SELECT COUNT(*) FROM usage), (SELECT COUNT(*) FROM quota), (SELECT COUNT(*) FROM turns)",
        [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert_eq!(counts, (1, 1, 1));
    Ok(())
}

fn record(ts_ms: i64, output: u64) -> UsageRecord {
    UsageRecord {
        dedupe_key: "msg_1".into(),
        ts_ms,
        model: Some("model-a".into()),
        usage: Usage {
            input_uncached: 11,
            cache_read: 12,
            cache_write: 13,
            output,
            reasoning: 5,
        },
    }
}

#[test]
fn upsert_usage_last_wins() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Claude, "log")?;
    store.upsert_usage(sid, &record(100, 10))?;
    store.upsert_usage(sid, &record(200, 1208))?;
    let totals: (i64, i64) =
        store
            .conn()
            .query_row("SELECT COUNT(*), SUM(output) FROM usage", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
    assert_eq!(totals, (1, 1208));
    Ok(())
}

#[test]
fn unicode_cwd_roundtrip() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "log")?;
    store.set_meta(
        sid,
        &SessionMeta {
            session_id: "session-1".into(),
            cwd: Some(r"D:\Dự án\x".into()),
            cli_version: Some("1.2".into()),
            started_ms: Some(100),
        },
    )?;
    let meta: (String, String, String, i64) = store.conn().query_row(
        "SELECT session_id, cwd, cli_version, started_ms FROM sessions WHERE id=?",
        [sid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    assert_eq!(
        meta,
        ("session-1".into(), r"D:\Dự án\x".into(), "1.2".into(), 100)
    );
    Ok(())
}

#[test]
fn context_keeps_latest() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "log")?;
    for (ts_ms, used_tokens, window_tokens) in [(200, 42, None), (100, 9, Some(1000))] {
        store.set_context(
            sid,
            &ContextSnapshot {
                ts_ms,
                used_tokens,
                window_tokens,
            },
        )?;
    }
    let ctx: (i64, i64, Option<i64>) = store.conn().query_row(
        "SELECT ctx_ts_ms, ctx_used, ctx_window FROM sessions WHERE id=?",
        [sid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert_eq!(ctx, (200, 42, None));
    Ok(())
}

#[test]
fn turn_idempotent() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Claude, "log")?;
    let mut turn = TurnSummary {
        ts_ms: 200,
        role: Role::Assistant,
        summary: "first".into(),
        byte_offset: 42,
    };
    store.insert_turn(sid, &turn)?;
    turn.summary = "duplicate".into();
    turn.ts_ms = 300;
    store.insert_turn(sid, &turn)?;
    let row: (i64, String, String) =
        store
            .conn()
            .query_row("SELECT COUNT(*), role, summary FROM turns", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
    assert_eq!(row, (1, "assistant".into(), "first".into()));
    Ok(())
}

#[test]
fn cursor_roundtrip() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    assert!(store.cursor("missing")?.is_none());
    let mut c = Cursor {
        path: "log".into(),
        offset: 42,
        size: 100,
        mtime_ms: -12,
        head_hash: u64::MAX,
    };
    store.save_cursor(&c)?;
    c.offset = 80;
    store.save_cursor(&c)?;
    let saved = store.cursor("log")?.unwrap();
    assert_eq!(
        (
            saved.path,
            saved.offset,
            saved.size,
            saved.mtime_ms,
            saved.head_hash
        ),
        (c.path, 80, 100, -12, u64::MAX)
    );
    Ok(())
}

#[test]
fn open_creates_file() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("a").join("b").join("agm.db");
    let store = Store::open(&path)?;
    assert!(path.is_file());
    assert_eq!(
        store
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        store
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))?,
        "wal"
    );
    drop(store);
    Store::open(&path)?;
    Ok(())
}

#[test]
fn sessions_are_keyed_by_path_and_stats_accumulate() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "a")?;
    assert_eq!(store.session_id_for_path(Source::Claude, "a")?, sid);
    assert_ne!(store.session_id_for_path(Source::Codex, "b")?, sid);
    store.set_model(sid, "model-b")?;
    store.add_line_stats(sid, 2, 3)?;
    store.add_line_stats(sid, 5, 7)?;
    let row: (String, String, i64, i64, Option<i64>) = store.conn().query_row(
        "SELECT source, model, unknown_lines, bad_lines, last_ms FROM sessions WHERE id=?",
        [sid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    assert_eq!(row, ("codex".into(), "model-b".into(), 7, 10, None));
    Ok(())
}

#[test]
fn quota_duplicates_are_ignored_per_window() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut snap = QuotaSnapshot {
        provider: "codex".into(),
        ts_ms: 123,
        windows: vec![
            QuotaWindow {
                window_minutes: 300,
                used_percent: 12.5,
                resets_at_s: 456,
            },
            QuotaWindow {
                window_minutes: 10080,
                used_percent: 25.0,
                resets_at_s: 789,
            },
        ],
    };
    store.insert_quota(&snap)?;
    snap.windows[0].used_percent = 99.0;
    store.insert_quota(&snap)?;
    let row: (i64, f64) =
        store
            .conn()
            .query_row("SELECT COUNT(*), SUM(used_percent) FROM quota", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
    assert_eq!(row, (2, 37.5));
    Ok(())
}

#[test]
fn last_ms_tracks_maximum_of_stored_usage_and_turns() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Claude, "a")?;
    store.upsert_usage(sid, &record(300, 10))?;
    store.insert_turn(
        sid,
        &TurnSummary {
            ts_ms: 200,
            role: Role::User,
            summary: "hello".into(),
            byte_offset: 0,
        },
    )?;
    let last = || {
        store
            .conn()
            .query_row("SELECT last_ms FROM sessions WHERE id=?", [sid], |r| {
                r.get::<_, i64>(0)
            })
    };
    assert_eq!(last()?, 300);
    store.upsert_usage(sid, &record(100, 20))?;
    assert_eq!(last()?, 200);
    store.insert_turn(
        sid,
        &TurnSummary {
            ts_ms: 400,
            role: Role::Tool,
            summary: "done".into(),
            byte_offset: 100,
        },
    )?;
    assert_eq!(last()?, 400);
    Ok(())
}

#[test]
fn dedupe_is_scoped_to_session_and_no_future_tables_exist() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    for path in ["a", "b"] {
        let sid = store.session_id_for_path(Source::Codex, path)?;
        store.upsert_usage(sid, &record(100, 10))?;
    }
    assert_eq!(
        store
            .conn()
            .query_row("SELECT COUNT(*) FROM usage", [], |r| r.get::<_, i64>(0))?,
        2
    );
    let mut stmt = store.conn().prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    let tables = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        tables,
        ["actions", "budgets", "cursors", "inbox", "messages", "quota", "rotations", "sessions", "settings", "terminals", "turns", "usage"]
    );
    Ok(())
}
