use agmag::correlate::{link_all, link_codex_rotation};
use agmag::model::{Role, SessionMeta, Source, TurnSummary};
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

#[test]
fn link_all_skips_rotation_successor_and_codex_tag_links_across_worktrees() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut t = TerminalRow {
        address: "t99".into(),
        app: Some("codex".into()),
        cwd: Some(r"D:\repo\.wt\a".into()),
        state: "live".into(),
        created_ms: 1_000_000,
        ..Default::default()
    };
    t.labels.insert("agm.parent_rotation".into(), "7".into());
    store.upsert_terminal(&t)?;

    // Hai phiên khác worktree: một ở repo/.wt/b (khác cwd), một ở repo/.wt/a (đúng cwd + có tag).
    let wrong_wt = session(&store, Source::Codex, "b.jsonl", r"D:\repo\.wt\b", 1_005_000);
    store.insert_turn(wrong_wt, &TurnSummary { ts_ms: 1_006_000, role: Role::User, summary: "[agm-rot:7] Read...".into(), byte_offset: 0 })?;

    let right = session(&store, Source::Codex, "a.jsonl", r"D:\repo\.wt\a", 1_005_000);
    store.insert_turn(right, &TurnSummary { ts_ms: 1_006_000, role: Role::User, summary: "<environment_context>".into(), byte_offset: 0 })?;
    store.insert_turn(right, &TurnSummary { ts_ms: 1_007_000, role: Role::User, summary: "[agm-rot:7] Read...".into(), byte_offset: 100 })?;

    // link_all phải bỏ qua t99 có nhãn agm.parent_rotation.
    assert_eq!(link_all(&store)?, 0);
    assert_eq!(link_codex_rotation(&store, "t99", 7, r"D:\repo\.wt\a", 1_000_000)?, Some(right));
    assert_eq!(store.terminal("t99")?.unwrap().session, Some(right));
    Ok(())
}

fn linked(store: &Store, addr: &str) -> Option<i64> {
    store.terminal(addr).unwrap().unwrap().session
}

/// OpenCode TUI opened long before its first prompt still gets linked (the 10-minute window is for Codex/Claude).
#[test]
fn opencode_session_started_long_after_terminal_links() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let minute = 60_000;
    term(&store, "t3m", "opencode", r"D:\SWAT-model\SpySWAT", 1_000_000);
    let sid = session(&store, Source::OpenCode, "opencode://late", "D:/SWAT-model/SpySWAT", 1_000_000 + 36 * minute);
    assert_eq!(link_all(&store)?, 1);
    assert_eq!(linked(&store, "t3m"), Some(sid));
    // Codex keeps the strict window.
    term(&store, "t9", "codex", r"D:\x", 1_000_000);
    session(&store, Source::Codex, "codex://late", r"D:\x", 1_000_000 + 36 * minute);
    assert_eq!(link_all(&store)?, 0);
    assert_eq!(linked(&store, "t9"), None);
    Ok(())
}

/// `/new` in the same OpenCode terminal: an automatic link follows the newest session; manual links never move.
#[test]
fn opencode_auto_link_follows_newer_session_but_manual_link_stays() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    term(&store, "t46", "opencode", r"D:\Project\agmag", 1_000_000);
    let first = session(&store, Source::OpenCode, "opencode://first", "D:/Project/agmag", 1_010_000);
    link_all(&store)?;
    assert_eq!(linked(&store, "t46"), Some(first));
    let second = session(&store, Source::OpenCode, "opencode://second", "D:/Project/agmag", 2_000_000);
    assert_eq!(link_all(&store)?, 1);
    assert_eq!(linked(&store, "t46"), Some(second));
    assert_eq!(link_all(&store)?, 0, "stable once on the newest session");

    term(&store, "t47", "opencode", r"D:\Project\other", 1_000_000);
    let pinned = session(&store, Source::OpenCode, "opencode://pinned", "D:/Project/other", 1_010_000);
    store.link_terminal("t47", pinned, "manual")?;
    session(&store, Source::OpenCode, "opencode://newer", "D:/Project/other", 3_000_000);
    link_all(&store)?;
    assert_eq!(linked(&store, "t47"), Some(pinned));
    Ok(())
}

/// Two OpenCode terminals in one folder both want the newest session → ambiguous, left for `agm link`.
#[test]
fn two_opencode_terminals_same_folder_stay_unlinked() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    term(&store, "ta", "opencode", r"D:\shared", 1_000_000);
    term(&store, "tb", "opencode", r"D:\shared", 1_100_000);
    session(&store, Source::OpenCode, "opencode://s", "D:/shared", 2_000_000);
    assert_eq!(link_all(&store)?, 0);
    assert_eq!((linked(&store, "ta"), linked(&store, "tb")), (None, None));
    Ok(())
}
