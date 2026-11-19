use agmag::config::Config;
use agmag::dashboard::{parse_command, render, snapshot, TuiCmd};
use agmag::model::{ContextSnapshot, QuotaSnapshot, QuotaWindow, Source, Usage, UsageRecord};
use agmag::store::{BudgetRow, Store, TerminalRow};

fn seeded() -> anyhow::Result<Store> {
    let store = Store::open_in_memory()?;
    store.insert_quota(&QuotaSnapshot {
        provider: "codex".into(),
        ts_ms: 1,
        windows: vec![QuotaWindow { window_minutes: 300, used_percent: 22.0, resets_at_s: 2_000 }, QuotaWindow { window_minutes: 10080, used_percent: 86.0, resets_at_s: 900_000 }],
    })?;
    store.upsert_terminal(&TerminalRow {
        address: "t5".into(),
        name: Some("w-login-7-1".into()),
        app: Some("opencode".into()),
        task: Some("login-7".into()),
        role: Some("worker".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_status("t5", Some("blocked"), Some("question"), Some("Chạy test?"), 1, 1)?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://a")?;
    store.upsert_usage(sid, &UsageRecord { dedupe_key: "k".into(), ts_ms: 1, model: Some("gpt-6.1-sol".into()), usage: Usage { output: 340_000, ..Default::default() } })?;
    store.set_context(sid, &ContextSnapshot { ts_ms: 1, used_tokens: 237_500, window_tokens: Some(475_000) })?;
    store.link_terminal("t5", sid, "auto")?;
    store.set_budget(&BudgetRow { scope: "task:login-7".into(), unit: "tokens".into(), soft: 300_000.0, hard: 3_000_000.0, action: "notify".into() })?;
    store.open_inbox(Some("t5"), "blocked", Some("question"), "Chạy test?", 2)?;
    Ok(store)
}

#[test]
fn snapshot_sections() -> anyhow::Result<()> {
    let store = seeded()?;
    let s = snapshot(&store, &Config::builtin(), 1_000_000)?;
    assert_eq!(s.quota.len(), 2);
    let t = &s.terminals[0];
    assert_eq!((t.address.as_str(), t.tokens, t.context_pct), ("t5", Some(340_000), Some(50.0)));
    assert_eq!(s.budgets[0].value, Some(340_000.0));
    assert_eq!(s.inbox.len(), 1);
    assert!(s.policy_on);
    Ok(())
}

#[test]
fn render_contains_all_sections() -> anyhow::Result<()> {
    let store = seeded()?;
    let text = render(&snapshot(&store, &Config::builtin(), 1_000_000)?, 100);
    for needle in ["HẠN MỨC GÓI", "codex", "86%", "TERMINALS", "w-login-7-1", "BLOCKED", "50%", "NGÂN SÁCH", "task:login-7", "HỘP THƯ", "#1", "Chạy test?"] {
        assert!(text.contains(needle), "thiếu '{needle}' trong:\n{text}");
    }
    Ok(())
}

#[test]
fn render_shows_red_warning_for_live_rotation_pair() -> anyhow::Result<()> {
    let store = seeded()?;
    store.upsert_terminal(&TerminalRow { address: "t6".into(), app: Some("claude-code".into()), cwd: Some(r"D:\repo".into()), state: "live".into(), ..Default::default() })?;
    store.upsert_terminal(&TerminalRow { address: "t7".into(), app: Some("claude-code".into()), cwd: Some(r"D:\repo".into()), state: "live".into(), ..Default::default() })?;
    store.set_terminal_handoff_state("t6", "handed_off")?;
    store.conn().execute(
        "INSERT INTO rotations (old_terminal, new_terminal, app, cwd, state, updated_ms) VALUES ('t6', 't7', 'claude-code', 'D:\\repo', 'done', 10)",
        [],
    )?;
    let text = render(&snapshot(&store, &Config::builtin(), 1_000_000)?, 120);
    assert!(text.contains("CẢNH BÁO: t6 (đã bàn giao) và t7 cùng đang chạy"), "{text}");
    assert!(text.contains("[ĐÃ BÀN GIAO]"), "{text}");
    Ok(())
}

#[test]
fn parse_commands() {
    assert_eq!(parse_command("r 3 có, chạy đi"), Ok(TuiCmd::Reply(3, "có, chạy đi".into())));
    assert_eq!(parse_command("s t5 GIAO làm tiếp"), Ok(TuiCmd::Send("t5".into(), "GIAO".into(), "làm tiếp".into())));
    assert_eq!(parse_command("s t5 BÁO CÁO xong"), Ok(TuiCmd::Send("t5".into(), "BÁO CÁO".into(), "xong".into())));
    assert_eq!(parse_command("k t5"), Ok(TuiCmd::Kill("t5".into())));
    assert_eq!(parse_command("i t5"), Ok(TuiCmd::Interrupt("t5".into())));
    assert_eq!(parse_command("p off"), Ok(TuiCmd::Policy(false)));
    assert_eq!(parse_command("q"), Ok(TuiCmd::Quit));
    assert_eq!(parse_command(""), Ok(TuiCmd::Refresh));
    assert!(parse_command("r x có").is_err());
    assert!(parse_command("zzz").is_err());
    assert_eq!(parse_command("s t5  GIAO  làm"), Ok(TuiCmd::Send("t5".into(), "GIAO".into(), "làm".into())));
    assert!(parse_command("s t5 GIAOX làm").is_err());
    assert_eq!(parse_command("d 4"), Ok(TuiCmd::Done(4)));
    assert_eq!(parse_command("c"), Ok(TuiCmd::Cleanup(None)));
    assert_eq!(parse_command("c 1,3"), Ok(TuiCmd::Cleanup(Some("1,3".into()))));
    assert_eq!(parse_command("c all"), Ok(TuiCmd::Cleanup(Some("all".into()))));
}
