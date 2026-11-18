use agmag::correlate::link_all;
use agmag::model::{SessionMeta, Source};
use agmag::store::{Store, TerminalRow};

fn term(store: &Store, addr: &str, app: &str, cwd: &str, created: i64) {
    store
        .upsert_terminal(&TerminalRow {
            address: addr.into(),
            app: Some(app.into()),
            cwd: Some(cwd.into()),
            state: "live".into(),
            created_ms: created,
            ..Default::default()
        })
        .unwrap();
}

fn session(store: &Store, source: Source, path: &str, cwd: &str, started: i64) -> i64 {
    let sid = store.session_id_for_path(source, path).unwrap();
    store
        .set_meta(sid, &SessionMeta { session_id: path.into(), cwd: Some(cwd.into()), cli_version: None, started_ms: Some(started) })
        .unwrap();
    sid
}

#[test]
fn unique_cwd_time_match_links() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    term(&store, "t5", "opencode", r"D:\Project\agmag\.wt\t5", 1_000_000);
    let sid = session(&store, Source::OpenCode, "opencode://a", "D:/project/agmag/.wt/t5/", 1_030_000);
    session(&store, Source::Codex, "c.jsonl", r"D:\Project\agmag\.wt\t5", 1_030_000); // khác app
    session(&store, Source::OpenCode, "opencode://old", r"D:\Project\agmag\.wt\t5", 1_000_000 - 3_600_000); // quá cũ
    assert_eq!(link_all(&store)?, 1);
    let t = store.terminal("t5")?.unwrap();
    assert_eq!((t.session, t.link_source.as_deref()), (Some(sid), Some("auto")));
    Ok(())
}

#[test]
fn ambiguous_not_linked() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    term(&store, "t5", "codex", r"D:\w", 1_000_000);
    session(&store, Source::Codex, "a.jsonl", r"D:\w", 1_010_000);
    session(&store, Source::Codex, "b.jsonl", r"D:\w", 1_020_000);
    assert_eq!(link_all(&store)?, 0);
    assert_eq!(store.terminal("t5")?.unwrap().session, None);
    Ok(())
}

#[test]
fn hook_link_wins_and_linked_session_not_reused() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    term(&store, "t1", "claude-code", r"D:\w", 1_000_000);
    term(&store, "t2", "claude-code", r"D:\w", 1_000_000);
    let a = session(&store, Source::Claude, "a.jsonl", r"D:\w", 1_010_000);
    let b = session(&store, Source::Claude, "b.jsonl", r"D:\w", 1_020_000);
    store.link_terminal("t1", a, "hook")?;
    // t2 chỉ còn một ứng viên (b) vì a đã thuộc t1.
    assert_eq!(link_all(&store)?, 1);
    assert_eq!(store.terminal("t2")?.unwrap().session, Some(b));
    assert_eq!(store.terminal("t1")?.unwrap().link_source.as_deref(), Some("hook"));
    Ok(())
}

#[test]
fn one_to_one_required_both_ways() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    // A tạo trước, chưa có phiên; B tạo sau 1 phút, phiên của B bắt đầu — cả hai cửa sổ đều chứa nó.
    term(&store, "tA", "codex", r"D:\w", 1_000_000);
    term(&store, "tB", "codex", r"D:\w", 1_060_000);
    session(&store, Source::Codex, "b.jsonl", r"D:\w", 1_070_000);
    assert_eq!(link_all(&store)?, 0);
    Ok(())
}

#[test]
fn session_of_finished_terminal_not_reused() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    term(&store, "old", "codex", r"D:\w", 1_000_000);
    let s = session(&store, Source::Codex, "a.jsonl", r"D:\w", 1_010_000);
    store.link_terminal("old", s, "auto")?;
    store.finish_terminal("old", Some(0), 1_050_000)?;
    term(&store, "new", "codex", r"D:\w", 1_060_000);
    assert_eq!(link_all(&store)?, 0);
    Ok(())
}
