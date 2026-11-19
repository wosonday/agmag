mod common;

use agmag::actions::{self, Caller, SpawnReq};
use agmag::config::Config;
use agmag::store::{Store, TerminalRow};
use anyhow::Result;
use common::FakeHc;

fn term(store: &Store, addr: &str, app: &str, role: Option<&str>) {
    store
        .upsert_terminal(&TerminalRow {
            address: addr.into(),
            name: Some(format!("n-{addr}")),
            app: Some(app.into()),
            role: role.map(String::from),
            state: "live".into(),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn send_to_user_goes_to_inbox() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let m = actions::send(&store, &hc, "t5", "user", "HỎI", Some("a"), "đổi đặc tả?", 1)?;
    assert_eq!(m.status, "inbox");
    assert!(hc.calls.borrow().is_empty());
    let inbox = store.inbox(true)?;
    assert_eq!((inbox[0].terminal.as_deref(), inbox[0].source.as_str(), inbox[0].text.as_str()), (Some("t5"), "message", "[HỎI] [ID a] đổi đặc tả?"));
    Ok(())
}

#[test]
fn send_records_message_and_calls_hc_if_idle() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let m = actions::send(&store, &hc, "codex-mgr", "w-a-1", "GIAO", Some("a"), "làm .agents/tasks/a.md", 1)?;
    assert_eq!(m.status, "sent");
    assert_eq!(hc.calls.borrow()[0], "send w-a-1 if_idle=true force=false [GIAO] [ID a] từ codex-mgr: làm .agents/tasks/a.md");
    let failing = FakeHc { fail_send: true, ..Default::default() };
    let m2 = actions::send(&store, &failing, "codex-mgr", "w-a-1", "GIAO", None, "x", 2)?;
    assert_eq!(m2.status, "failed");
    assert_eq!(store.messages(10)?.len(), 2);
    assert!(actions::send(&store, &hc, "a", "b", "BỪA", None, "x", 3).is_err());
    Ok(())
}

#[test]
fn reply_to_blocked_question_forces_and_resolves() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let id = store.open_inbox(Some("t1s"), "blocked", Some("question"), "Chạy test?", 1)?.unwrap();
    actions::reply(&store, &hc, id, "có", 2)?;
    assert_eq!(hc.calls.borrow()[0], "send t1s if_idle=false force=true có");
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn spawn_refuses_over_max_workers() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let mut cfg = Config::builtin();
    cfg.policy.max_workers = 1;
    term(&store, "t1", "opencode", Some("worker"));
    let req = SpawnReq { template: "worker-mid".into(), task: "a".into(), role: "worker".into(), ..Default::default() };
    let err = actions::spawn(&store, &hc, &cfg, &req, 1).unwrap_err().to_string();
    assert!(err.contains("max_workers"), "{err}");
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn spawn_sets_labels_name_and_records_terminal() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let req = SpawnReq {
        template: "worker-mid".into(),
        task: "login-7".into(),
        role: "worker".into(),
        cwd: Some("D:\\w".into()),
        text: Some("Đọc .agents/tasks/login-7.md".into()),
        ..Default::default()
    };
    let addr = actions::spawn(&store, &hc, &Config::builtin(), &req, 1)?;
    assert_eq!(addr, "t99");
    let call = hc.calls.borrow()[0].clone();
    assert!(call.contains("[\"worker-mid\", \"Đọc .agents/tasks/login-7.md\"]") && call.contains("Some(\"w-login-7-1\")"), "{call}");
    assert!(call.contains("\"agm.role\": \"worker\"") && call.contains("\"agm.task\": \"login-7\""), "{call}");
    assert_eq!(store.terminal("t99")?.unwrap().task.as_deref(), Some("login-7"));
    assert!(actions::spawn(&store, &hc, &Config::builtin(), &SpawnReq { task: "BAD ID".into(), ..req }, 2).is_err());
    Ok(())
}

#[test]
fn agent_cannot_kill_codex_or_claude() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    term(&store, "t1", "codex", None);
    term(&store, "t2", "claude-code", None);
    let agent = Caller::Agent(Some("t9".into()));
    for t in ["t1", "t2"] {
        assert!(actions::kill(&store, &hc, &agent, t, 1).is_err());
        assert!(actions::interrupt(&store, &hc, &agent, t, 1).is_err());
    }
    assert!(hc.calls.borrow().is_empty());
    // Đề nghị được chuyển vào hộp thư người dùng.
    assert!(store.inbox(true)?.iter().any(|i| i.source == "request" && i.terminal.as_deref() == Some("t1")));
    Ok(())
}

#[test]
fn agent_can_kill_opencode_worker() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    term(&store, "t3", "opencode", Some("worker"));
    actions::kill(&store, &hc, &Caller::Agent(Some("t9".into())), "t3", 1)?;
    assert_eq!(hc.calls.borrow()[0], "kill t3");
    Ok(())
}

#[test]
fn user_kill_requires_confirmation() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    term(&store, "t1", "codex", None);
    assert!(actions::kill(&store, &hc, &Caller::User { confirmed: false }, "t1", 1).is_err());
    actions::kill(&store, &hc, &Caller::User { confirmed: true }, "t1", 2)?;
    assert_eq!(hc.calls.borrow().as_slice(), ["kill t1"]);
    Ok(())
}

#[test]
fn caller_detection() {
    // Không có terminal hc + có TTY → người dùng; terminal là agent đã báo 7501 → agent; không TTY → agent.
    assert!(matches!(actions::classify_caller(None, false, true, true), Caller::User { confirmed: true }));
    assert!(matches!(actions::classify_caller(Some("t1s"), true, true, true), Caller::Agent(_)));
    assert!(matches!(actions::classify_caller(Some("t1b"), false, true, false), Caller::User { confirmed: false }));
    assert!(matches!(actions::classify_caller(None, false, false, true), Caller::Agent(None)));
}

#[test]
fn agent_cannot_kill_unknown_or_unrecorded_app() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let agent = Caller::Agent(Some("t9".into()));
    // Terminal không có trong CSDL (daemon không chạy) → không rõ app → từ chối.
    assert!(actions::kill(&store, &hc, &agent, "t77", 1).is_err());
    // Shim npm chưa báo 7501: app đoán là "claude.cmd"/"node" → không phải opencode → từ chối.
    term(&store, "t8", "node", Some("worker"));
    assert!(actions::kill(&store, &hc, &agent, "t8", 1).is_err());
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn fresh_hc_app_overrides_stale_db_app() -> Result<()> {
    let store = Store::open_in_memory()?;
    // CSDL nói opencode, nhưng hc get hiện báo claude-code → phải từ chối.
    term(&store, "t3", "opencode", Some("worker"));
    let hc = FakeHc { apps: vec![("t3".into(), "claude-code".into())], ..Default::default() };
    assert!(actions::kill(&store, &hc, &Caller::Agent(None), "t3", 1).is_err());
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn detect_caller_fails_closed() -> Result<()> {
    let store = Store::open_in_memory()?;
    // HC_TERMINAL_ID có nhưng hc get lỗi → coi là agent, kể cả có TTY và --yes.
    let failing = FakeHc { fail_get: true, ..Default::default() };
    assert!(matches!(actions::detect_caller(&store, &failing, Some("t9"), true, true), Caller::Agent(_)));
    // Terminal là app agent (dù CSDL ghi reported=0) → agent.
    let hc = FakeHc { apps: vec![("t9".into(), "claude-code".into())], ..Default::default() };
    assert!(matches!(actions::detect_caller(&store, &hc, Some("t9"), true, true), Caller::Agent(_)));
    // Shell của người dùng (pwsh, không báo 7501) + TTY → người dùng.
    store.upsert_terminal(&TerminalRow { address: "t1b".into(), app: Some("pwsh".into()), state: "live".into(), ..Default::default() })?;
    assert!(matches!(actions::detect_caller(&store, &FakeHc::default(), Some("t1b"), true, true), Caller::User { confirmed: true }));
    // Không có HC_TERMINAL_ID + TTY → người dùng; không TTY → agent.
    assert!(matches!(actions::detect_caller(&store, &FakeHc::default(), None, true, false), Caller::User { confirmed: false }));
    assert!(matches!(actions::detect_caller(&store, &FakeHc::default(), None, false, true), Caller::Agent(None)));
    Ok(())
}

#[test]
fn reply_only_to_agent_questions_and_done_dismisses() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let req = store.open_inbox(Some("t11"), "request", Some("kill"), "t9 đề nghị kill t11", 1)?.unwrap();
    // Trả lời một "đề nghị" không được gõ chữ vào phiên Claude t11.
    assert!(actions::reply(&store, &hc, req, "đồng ý", 2).is_err());
    assert!(hc.calls.borrow().is_empty());
    let budget = store.open_inbox(None, "budget", Some("soft"), "Ngân sách task:a vượt mềm", 1)?.unwrap();
    actions::dismiss(&store, req, 3)?;
    actions::dismiss(&store, budget, 3)?;
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn spawn_successor_precheck_allows_no_task_and_checks_budget() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path().to_string_lossy().to_string();
    let store = Store::open_in_memory()?;
    let spec = agmag::launch::parse_launch_spec("codex", &["codex".into()], &cwd)?;
    let name = actions::spawn_successor_precheck(&store, None, Some("main-codex"), "t1", 5, &spec)?;
    assert_eq!(name, "main-codex-r1");
    store.set_budget(&agmag::store::BudgetRow { scope: "task:x".into(), unit: "tokens".into(), soft: 0.0, hard: 0.0, action: "notify".into() })?;
    assert!(actions::spawn_successor_precheck(&store, Some("x"), None, "t1", 5, &spec).is_err());
    Ok(())
}

#[test]
fn cleanup_batch_requires_confirmation_handles_live_and_exited_and_keeps_history() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    // Cặp 1: t1 (live, handed_off) -> t2 (live, linked)
    term(&store, "t1", "claude-code", None);
    store.set_terminal_handoff_state("t1", "handed_off")?;
    term(&store, "t2", "claude-code", None);
    let sid1 = store.session_id_for_path(agmag::model::Source::Claude, "c1.jsonl")?;
    let sid2 = store.session_id_for_path(agmag::model::Source::Claude, "c2.jsonl")?;
    store.upsert_usage(sid1, &agmag::model::UsageRecord { dedupe_key: "u1".into(), ts_ms: 10, model: None, usage: agmag::model::Usage { output: 99, ..Default::default() } })?;
    store.link_terminal("t1", sid1, "hook")?;
    store.link_terminal("t2", sid2, "hook")?;
    store.conn().execute(
        "INSERT INTO rotations (old_terminal, new_terminal, app, state, handoff_path, updated_ms)
         VALUES ('t1', 't2', 'claude-code', 'done', '.agents/handoff/1-10.md', 10)",
        [],
    )?;
    // Cặp 2: t3 (exited, handed_off) -> t4 (cũng không còn live) vẫn dọn được bản ghi cũ
    term(&store, "t3", "codex", None);
    store.set_terminal_handoff_state("t3", "handed_off")?;
    store.finish_terminal("t3", Some(0), 20)?;
    store.open_inbox(Some("t3"), "rotation", Some("needs_attention"), "chú ý", 21)?;
    store.conn().execute(
        "INSERT INTO rotations (old_terminal, new_terminal, app, state, handoff_path, updated_ms)
         VALUES ('t3', 't4', 'codex', 'done', '.agents/handoff/2-20.md', 20)",
        [],
    )?;

    let list = actions::eligible_cleanups(&store)?;
    assert_eq!(list.len(), 2);

    // Chưa xác nhận → từ chối
    assert!(actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: false }, &list, 30).is_err());

    // Xác nhận một lần → dọn cả hai (t1 bị kill qua hc, t3 chỉ remove khỏi DB)
    let rep = actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &list, 30)?;
    assert_eq!(rep.done, 2);
    assert!(rep.skipped.is_empty() && rep.errors.is_empty());
    assert_eq!(hc.calls.borrow().as_slice(), ["kill t1"]);
    assert_eq!(store.terminal("t3")?.unwrap().state, "removed");
    assert!(store.inbox(true)?.is_empty());
    // Lịch sử session, usage, rotations vẫn còn nguyên.
    let kept: (i64, i64) = store.conn().query_row(
        "SELECT (SELECT COUNT(*) FROM usage WHERE session = ?1), (SELECT COUNT(*) FROM rotations)",
        [sid1],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(kept, (1, 2));
    Ok(())
}

#[test]
fn set_managed_launch_blocks_agents_and_unconfirmed_users() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path().to_string_lossy().to_string();
    let store = Store::open_in_memory()?;
    term(&store, "t11", "claude-code", None);
    let spec = agmag::launch::parse_launch_spec(
        "claude-code",
        &["claude".into(), "--dangerously-skip-permissions".into()],
        &cwd,
    )?;

    // Agent không thể tự bật `manage` để gắn cờ bỏ qua quyền cho phiên kế nhiệm.
    assert!(agmag::actions::set_managed_launch(&store, &Caller::Agent(Some("t11".into())), "t11", &spec, 10).is_err());
    assert!(!store.terminal("t11")?.unwrap().managed);
    assert!(store.inbox(true)?.iter().any(|i| i.source == "request" && i.kind.as_deref() == Some("manage")));

    // Người dùng chưa xác nhận cũng bị từ chối.
    assert!(agmag::actions::set_managed_launch(&store, &Caller::User { confirmed: false }, "t11", &spec, 11).is_err());
    assert!(!store.terminal("t11")?.unwrap().managed);

    // Chỉ User { confirmed: true } mới bật được.
    agmag::actions::set_managed_launch(&store, &Caller::User { confirmed: true }, "t11", &spec, 12)?;
    assert!(store.terminal("t11")?.unwrap().managed);
    Ok(())
}
