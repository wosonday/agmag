use agmag::model::{ContextSnapshot, Source};
use agmag::store::{BudgetRow, MessageRow, Store, TerminalRow};

fn term(addr: &str) -> TerminalRow {
    TerminalRow {
        address: addr.into(),
        name: Some(format!("w-{addr}")),
        app: Some("opencode".into()),
        cwd: Some(r"D:\Dự án\wt".into()),
        state: "live".into(),
        created_ms: 1000,
        ..Default::default()
    }
}

#[test]
fn migrates_v1_to_v2() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("agm.db");
    {
        let c = rusqlite::Connection::open(&path)?;
        c.execute_batch(include_str!("../src/store/schema.sql"))?;
        c.execute_batch("PRAGMA user_version = 1;")?;
    }
    let store = Store::open(&path)?;
    let v: i64 = store.conn().query_row("PRAGMA user_version", [], |r| r.get(0))?;
    assert_eq!(v, 2);
    let n: i64 = store.conn().query_row("SELECT COUNT(*) FROM inbox", [], |r| r.get(0))?;
    assert_eq!(n, 0);
    Ok(())
}

#[test]
fn terminal_roundtrip_and_status() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut t = term("t7");
    t.labels.insert("agm.task".into(), "login-7".into());
    store.upsert_terminal(&t)?;
    store.set_terminal_status("t7", Some("blocked"), Some("question"), Some("Chạy test?"), 3, 2000)?;
    let got = store.terminal("t7")?.unwrap();
    assert_eq!(got.cwd.as_deref(), Some(r"D:\Dự án\wt"));
    assert_eq!(got.labels["agm.task"], "login-7");
    assert_eq!((got.status.as_deref(), got.kind.as_deref(), got.turn), (Some("blocked"), Some("question"), 3));
    store.finish_terminal("t7", Some(0), 3000)?;
    assert_eq!(store.terminal("t7")?.unwrap().state, "finished");
    assert!(store.terminals(true)?.is_empty());
    assert_eq!(store.terminals(false)?.len(), 1);
    Ok(())
}

#[test]
fn inbox_open_dedupes() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let a = store.open_inbox(Some("t7"), "blocked", Some("question"), "Chạy test?", 1)?;
    let b = store.open_inbox(Some("t7"), "blocked", Some("question"), "Chạy test?", 2)?;
    assert!(a.is_some() && b.is_none());
    assert_eq!(store.inbox(true)?.len(), 1);
    store.resolve_inbox_for_terminal("t7", "blocked", 3)?;
    assert!(store.inbox(true)?.is_empty());
    // Đã đóng thì cùng nội dung được mở lại.
    assert!(store.open_inbox(Some("t7"), "blocked", Some("question"), "Chạy test?", 4)?.is_some());
    let id = store.inbox(true)?[0].id;
    store.resolve_inbox(id, Some("có"), 5)?;
    assert_eq!(store.inbox(false)?.iter().filter(|r| r.reply.as_deref() == Some("có")).count(), 1);
    Ok(())
}

#[test]
fn message_roundtrip() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let id = store.add_message(&MessageRow {
        ts_ms: 10,
        from: "codex-mgr".into(),
        to: "w-a-1".into(),
        kind: "GIAO".into(),
        task: Some("a".into()),
        text: "làm t1".into(),
        status: "queued".into(),
        ..Default::default()
    })?;
    store.set_message_status(id, "sent", Some("turn 4"))?;
    let m = &store.messages(10)?[0];
    assert_eq!((m.status.as_str(), m.detail.as_deref(), m.text.as_str()), ("sent", Some("turn 4"), "làm t1"));
    Ok(())
}

#[test]
fn settings_budgets_actions() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    assert_eq!(store.setting("policy")?, None);
    store.set_setting("policy", "off")?;
    assert_eq!(store.setting("policy")?.as_deref(), Some("off"));
    store.set_budget(&BudgetRow { scope: "task:a".into(), unit: "tokens".into(), soft: 100.0, hard: 200.0, action: "notify".into() })?;
    store.set_budget(&BudgetRow { scope: "task:a".into(), unit: "tokens".into(), soft: 150.0, hard: 300.0, action: "notify".into() })?;
    assert_eq!(store.budgets()?.len(), 1);
    assert_eq!(store.budgets()?[0].soft, 150.0);
    assert!(store.rm_budget("task:a")?);
    store.log_action("user", "kill", "t7", "lý do", true, 9)?;
    let n: i64 = store.conn().query_row("SELECT COUNT(*) FROM actions", [], |r| r.get(0))?;
    assert_eq!(n, 1);
    Ok(())
}

#[test]
fn context_keeps_known_window() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Claude, "a.jsonl")?;
    store.set_context(sid, &ContextSnapshot { ts_ms: 1, used_tokens: 10, window_tokens: Some(1000) })?;
    // Lần quét log sau không biết cửa sổ (None) — không được xóa giá trị hook đã ghi.
    store.set_context(sid, &ContextSnapshot { ts_ms: 2, used_tokens: 20, window_tokens: None })?;
    let w: Option<i64> = store.conn().query_row("SELECT ctx_window FROM sessions", [], |r| r.get(0))?;
    assert_eq!(w, Some(1000));
    Ok(())
}

#[test]
fn link_terminal_to_session() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&term("t9"))?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://ses_a")?;
    store.link_terminal("t9", sid, "auto")?;
    let t = store.terminal("t9")?.unwrap();
    assert_eq!((t.session, t.link_source.as_deref()), (Some(sid), Some("auto")));
    // upsert lại (sự kiện mới) không được xóa liên kết.
    store.upsert_terminal(&term("t9"))?;
    assert_eq!(store.terminal("t9")?.unwrap().session, Some(sid));
    Ok(())
}

#[test]
fn migration_normalizes_v1_path_keys() -> anyhow::Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("agm.db");
    {
        let c = rusqlite::Connection::open(&path)?;
        c.execute_batch(include_str!("../src/store/schema.sql"))?;
        c.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO sessions(source, path) VALUES ('codex', 'C:/Users/A/Rollout-1.jsonl'), ('opencode', 'opencode://ses_X');
             INSERT INTO cursors VALUES ('C:/Users/A/Rollout-1.jsonl', 10, 10, 0, 0);",
        )?;
    }
    let store = Store::open(&path)?;
    let p: Vec<String> = store.conn().prepare("SELECT path FROM sessions ORDER BY id")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    assert_eq!(p, [r"c:\users\a\rollout-1.jsonl", "opencode://ses_X"]);
    assert!(store.cursor(r"c:\users\a\rollout-1.jsonl")?.is_some());
    Ok(())
}

#[test]
fn terminal_lookup_prefers_exact_address_and_keeps_labels() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut a = term("t1");
    a.name = Some("t2".into()); // tên trùng địa chỉ của terminal khác
    a.labels.insert("agm.task".into(), "x".into());
    store.upsert_terminal(&a)?;
    store.upsert_terminal(&term("t2"))?;
    assert_eq!(store.terminal("t2")?.unwrap().address, "t2");
    // Sự kiện sau không mang nhãn → không được xóa nhãn đã có.
    let mut again = term("t1");
    again.labels.clear();
    store.upsert_terminal(&again)?;
    assert_eq!(store.terminal("t1")?.unwrap().labels.get("agm.task").map(String::as_str), Some("x"));
    Ok(())
}
