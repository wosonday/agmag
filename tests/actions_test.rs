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
    let m = actions::send(
        &store,
        &hc,
        "t5",
        "user",
        "HỎI",
        Some("a"),
        "đổi đặc tả?",
        1,
    )?;
    assert_eq!(m.status, "inbox");
    assert!(hc.calls.borrow().is_empty());
    let inbox = store.inbox(true)?;
    assert_eq!(
        (
            inbox[0].terminal.as_deref(),
            inbox[0].source.as_str(),
            inbox[0].text.as_str()
        ),
        (Some("t5"), "message", "[HỎI] [ID a] đổi đặc tả?")
    );
    Ok(())
}

#[test]
fn send_records_message_and_calls_hc_if_idle() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let m = actions::send(
        &store,
        &hc,
        "codex-mgr",
        "w-a-1",
        "GIAO",
        Some("a"),
        "làm .agents/tasks/a.md",
        1,
    )?;
    assert_eq!(m.status, "sent");
    assert_eq!(
        hc.calls.borrow()[0],
        "send w-a-1 if_idle=true force=false [GIAO] [ID a] từ codex-mgr: làm .agents/tasks/a.md"
    );
    let failing = FakeHc {
        fail_send: true,
        ..Default::default()
    };
    let m2 = actions::send(&store, &failing, "codex-mgr", "w-a-1", "GIAO", None, "x", 2)?;
    assert_eq!(m2.status, "failed");
    assert_eq!(store.messages(10)?.len(), 2);
    assert!(actions::send(&store, &hc, "a", "b", "BỪA", None, "x", 3).is_err());
    Ok(())
}

fn hc_with_state(addr: &str, state: &str) -> FakeHc {
    let mut hc = FakeHc::default();
    hc.get_responses.insert(
        addr.into(),
        serde_json::json!({"address": addr, "state": "live", "status": {"app": "claude-code", "state": state}}),
    );
    hc
}

#[test]
fn send_reaches_agent_whose_turn_is_done() -> Result<()> {
    // `done` = the agent finished its turn and is not working: typing is safe, but `hc send --if idle`
    // refuses it ("t3p is done, not idle"), so agm must send without the idle guard.
    let store = Store::open_in_memory()?;
    let hc = hc_with_state("t3p", "done");
    let m = actions::send(&store, &hc, "t1s", "t3p", "GIAO", None, "đọc lại docs", 1)?;
    assert_eq!(m.status, "sent");
    assert!(
        hc.calls.borrow()[0].starts_with("send t3p if_idle=false force=false"),
        "{:?}",
        hc.calls.borrow()
    );
    Ok(())
}

#[test]
fn send_keeps_idle_guard_for_working_blocked_or_unknown_agents() -> Result<()> {
    let store = Store::open_in_memory()?;
    for state in ["working", "blocked", "idle"] {
        let hc = hc_with_state("t5", state);
        actions::send(&store, &hc, "t1s", "t5", "GIAO", None, "x", 1)?;
        assert!(
            hc.calls.borrow()[0].starts_with("send t5 if_idle=true force=false"),
            "{state}: {:?}",
            hc.calls.borrow()
        );
    }
    let unreadable = FakeHc {
        fail_get: true,
        ..Default::default()
    };
    actions::send(&store, &unreadable, "t1s", "t5", "GIAO", None, "x", 2)?;
    assert!(
        unreadable.calls.borrow()[0].starts_with("send t5 if_idle=true"),
        "hc get failed → keep the guard"
    );
    Ok(())
}

#[test]
fn reply_to_blocked_question_forces_and_resolves() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let id = store
        .open_inbox(Some("t1s"), "blocked", Some("question"), "Chạy test?", 1)?
        .unwrap();
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
    let req = SpawnReq {
        template: "worker-mid".into(),
        task: "a".into(),
        role: "worker".into(),
        ..Default::default()
    };
    let err = actions::spawn(&store, &hc, &cfg, &req, 1)
        .unwrap_err()
        .to_string();
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
        text: vec!["Đọc".into(), ".agents/tasks/login-7.md".into()],
        ..Default::default()
    };
    let addr = actions::spawn(&store, &hc, &Config::builtin(), &req, 1)?;
    assert_eq!(addr, "t99");
    let call = hc.calls.borrow()[0].clone();
    assert!(
        call.contains("[\"worker-mid\", \"Đọc .agents/tasks/login-7.md\"]")
            && call.contains("Some(\"w-login-7-1\")"),
        "{call}"
    );
    assert!(
        call.contains("\"agm.role\": \"worker\"") && call.contains("\"agm.task\": \"login-7\""),
        "{call}"
    );
    assert_eq!(
        store.terminal("t99")?.unwrap().task.as_deref(),
        Some("login-7")
    );
    assert!(
        actions::spawn(
            &store,
            &hc,
            &Config::builtin(),
            &SpawnReq {
                task: "BAD ID".into(),
                ..req
            },
            2
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn spawn_program_preserves_each_argument() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let req = SpawnReq {
        template: "python".into(),
        task: "probe".into(),
        role: "probe".into(),
        text: vec![
            "-c".into(),
            "import sys;print(sys.argv)".into(),
            "--prompt".into(),
            "Bạn là thợ".into(),
            "".into(),
        ],
        ..Default::default()
    };
    actions::spawn(&store, &hc, &Config::builtin(), &req, 1)?;
    assert!(hc.calls.borrow()[0].starts_with("run [\"python\", \"-c\", \"import sys;print(sys.argv)\", \"--prompt\", \"Bạn là thợ\", \"\"]"));
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
    assert!(
        store
            .inbox(true)?
            .iter()
            .any(|i| i.source == "request" && i.terminal.as_deref() == Some("t1"))
    );
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
    assert!(matches!(
        actions::classify_caller(None, false, true, true),
        Caller::User { confirmed: true }
    ));
    assert!(matches!(
        actions::classify_caller(Some("t1s"), true, true, true),
        Caller::Agent(_)
    ));
    assert!(matches!(
        actions::classify_caller(Some("t1b"), false, true, false),
        Caller::User { confirmed: false }
    ));
    assert!(matches!(
        actions::classify_caller(None, false, false, true),
        Caller::Agent(None)
    ));
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
    let hc = FakeHc {
        apps: vec![("t3".into(), "claude-code".into())],
        ..Default::default()
    };
    assert!(actions::kill(&store, &hc, &Caller::Agent(None), "t3", 1).is_err());
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn detect_caller_fails_closed() -> Result<()> {
    let store = Store::open_in_memory()?;
    // HC_TERMINAL_ID có nhưng hc get lỗi → coi là agent, kể cả có TTY và --yes.
    let failing = FakeHc {
        fail_get: true,
        ..Default::default()
    };
    assert!(matches!(
        actions::detect_caller(&store, &failing, Some("t9"), true, true),
        Caller::Agent(_)
    ));
    // Terminal là app agent (dù CSDL ghi reported=0) → agent.
    let hc = FakeHc {
        apps: vec![("t9".into(), "claude-code".into())],
        ..Default::default()
    };
    assert!(matches!(
        actions::detect_caller(&store, &hc, Some("t9"), true, true),
        Caller::Agent(_)
    ));
    // Shell của người dùng (pwsh, không báo 7501) + TTY → người dùng.
    store.upsert_terminal(&TerminalRow {
        address: "t1b".into(),
        app: Some("pwsh".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    assert!(matches!(
        actions::detect_caller(&store, &FakeHc::default(), Some("t1b"), true, true),
        Caller::User { confirmed: true }
    ));
    // Không có HC_TERMINAL_ID + TTY → người dùng; không TTY → agent.
    assert!(matches!(
        actions::detect_caller(&store, &FakeHc::default(), None, true, false),
        Caller::User { confirmed: false }
    ));
    assert!(matches!(
        actions::detect_caller(&store, &FakeHc::default(), None, false, true),
        Caller::Agent(None)
    ));
    Ok(())
}

#[test]
fn reply_only_to_agent_questions_and_done_dismisses() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let req = store
        .open_inbox(
            Some("t11"),
            "request",
            Some("kill"),
            "t9 đề nghị kill t11",
            1,
        )?
        .unwrap();
    // Trả lời một "đề nghị" không được gõ chữ vào phiên Claude t11.
    assert!(actions::reply(&store, &hc, req, "đồng ý", 2).is_err());
    assert!(hc.calls.borrow().is_empty());
    let budget = store
        .open_inbox(None, "budget", Some("soft"), "Ngân sách task:a vượt mềm", 1)?
        .unwrap();
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
    store.set_budget(&agmag::store::BudgetRow {
        scope: "task:x".into(),
        unit: "tokens".into(),
        soft: 0.0,
        hard: 0.0,
        action: "notify".into(),
    })?;
    assert!(actions::spawn_successor_precheck(&store, Some("x"), None, "t1", 5, &spec).is_err());
    Ok(())
}

#[test]
fn cleanup_batch_requires_confirmation_handles_live_and_exited_and_keeps_history() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc {
        get_responses: ["t1", "t2"]
            .into_iter()
            .map(|t| {
                (
                    t.into(),
                    serde_json::json!({
                        "address":t, "state":"live", "created_ms":1, "status":{"app":"claude-code"}
                    }),
                )
            })
            .collect(),
        ..Default::default()
    };
    // Cặp 1: t1 (live, handed_off) -> t2 (live, linked)
    term(&store, "t1", "claude-code", None);
    store.set_terminal_handoff_state("t1", "handed_off")?;
    term(&store, "t2", "claude-code", None);
    store.conn().execute(
        "UPDATE terminals SET created_ms=1 WHERE address IN ('t1','t2')",
        [],
    )?;
    let sid1 = store.session_id_for_path(agmag::model::Source::Claude, "c1.jsonl")?;
    let sid2 = store.session_id_for_path(agmag::model::Source::Claude, "c2.jsonl")?;
    store.upsert_usage(
        sid1,
        &agmag::model::UsageRecord {
            dedupe_key: "u1".into(),
            ts_ms: 10,
            model: None,
            usage: agmag::model::Usage {
                output: 99,
                ..Default::default()
            },
        },
    )?;
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
    assert!(
        actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: false }, &list, 30)
            .is_err()
    );

    // Xác nhận một lần → dọn cả hai (t1 bị kill qua hc, t3 chỉ remove khỏi DB)
    let rep =
        actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &list, 30)?;
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
    assert!(
        agmag::actions::set_managed_launch(
            &store,
            &Caller::Agent(Some("t11".into())),
            "t11",
            &spec,
            10
        )
        .is_err()
    );
    assert!(!store.terminal("t11")?.unwrap().managed);
    assert!(
        store
            .inbox(true)?
            .iter()
            .any(|i| i.source == "request" && i.kind.as_deref() == Some("manage"))
    );

    // Người dùng chưa xác nhận cũng bị từ chối.
    assert!(
        agmag::actions::set_managed_launch(
            &store,
            &Caller::User { confirmed: false },
            "t11",
            &spec,
            11
        )
        .is_err()
    );
    assert!(!store.terminal("t11")?.unwrap().managed);

    // Chỉ User { confirmed: true } mới bật được.
    agmag::actions::set_managed_launch(
        &store,
        &Caller::User { confirmed: true },
        "t11",
        &spec,
        12,
    )?;
    assert!(store.terminal("t11")?.unwrap().managed);
    Ok(())
}

#[test]
fn parse_done_id_selectors() {
    use agmag::actions::parse_done_selectors;
    assert_eq!(parse_done_selectors(&["5".into()]).unwrap().ids, vec![5]);
    assert!(parse_done_selectors(&["5".into()]).unwrap().legacy_single);
    assert_eq!(
        parse_done_selectors(&["3-7".into()]).unwrap().ids,
        vec![3, 4, 5, 6, 7]
    );
    assert!(!parse_done_selectors(&["3-7".into()]).unwrap().legacy_single);
    assert_eq!(
        parse_done_selectors(&["1,4-6".into()]).unwrap().ids,
        vec![1, 4, 5, 6]
    );
    assert_eq!(
        parse_done_selectors(&["150,151".into(), "160-162".into()])
            .unwrap()
            .ids,
        vec![150, 151, 160, 161, 162]
    );
    assert!(
        !parse_done_selectors(&["150,151".into(), "160-162".into()])
            .unwrap()
            .legacy_single
    );
}

#[test]
fn parse_done_id_selectors_rejects_bad_input() {
    use agmag::actions::parse_done_selectors;
    for bad in ["9-3", "a", "1-", ""] {
        assert!(parse_done_selectors(&[bad.into()]).is_err(), "{bad}");
    }
    // range too large (> 10000 items)
    assert!(parse_done_selectors(&["1-10002".into()]).is_err());
    // boundary: exactly 10000 items is allowed, 10001 is not
    assert_eq!(
        parse_done_selectors(&["1-10000".into()]).unwrap().ids.len(),
        10_000
    );
    assert!(parse_done_selectors(&["1-10001".into()]).is_err());
    // huge bounds must be a clean error, never an overflow panic or a giant loop
    assert!(parse_done_selectors(&["0-9223372036854775807".into()]).is_err());
    assert_eq!(
        parse_done_selectors(&["9223372036854775806-9223372036854775807".into()])
            .unwrap()
            .ids,
        vec![i64::MAX - 1, i64::MAX]
    );
    assert!(parse_done_selectors(&["1-99999999999999999999".into()]).is_err());
}

#[test]
fn dismiss_done_bulk_range_skips_blocked_done_missing() -> anyhow::Result<()> {
    use agmag::actions::{DoneFilters, dismiss_done, parse_done_selectors};
    use agmag::store::Store;

    let store = Store::open_in_memory()?;
    let a = store
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r1", 1)?
        .unwrap();
    let b = store
        .open_inbox(Some("t1"), "message", Some("HỎI"), "q1", 2)?
        .unwrap();
    let c = store
        .open_inbox(Some("t1"), "blocked", Some("question"), "wait", 3)?
        .unwrap();
    let d = store
        .open_inbox(Some("t2"), "context", Some("warn"), "ctx", 4)?
        .unwrap();
    store.resolve_inbox(b, None, 5)?; // already done

    let sel = parse_done_selectors(&[format!("{a}-{d}")])?;
    let report = dismiss_done(
        &store,
        sel.ids,
        DoneFilters::default(),
        false,
        sel.legacy_single,
        10,
    )?;
    assert_eq!(report.done, vec![a, d]);
    assert!(
        report
            .skipped
            .iter()
            .any(|s| s.id == b && s.reason == "already done")
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|s| s.id == c && s.reason == "blocked (use --source blocked)")
    );
    // missing in range? include an id beyond
    let sel2 = parse_done_selectors(&[format!("{d}-{}", d + 1)])?;
    let report2 = dismiss_done(
        &store,
        sel2.ids,
        DoneFilters::default(),
        false,
        sel2.legacy_single,
        11,
    )?;
    assert!(
        report2
            .skipped
            .iter()
            .any(|s| s.id == d + 1 && s.reason == "missing")
    );
    Ok(())
}

#[test]
fn dismiss_done_filters_kind_terminal_source_and_blocked_rules() -> anyhow::Result<()> {
    use agmag::actions::{DoneFilters, dismiss_done, normalize_done_kind};
    use agmag::store::Store;

    let store = Store::open_in_memory()?;
    let r1 = store
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r1", 1)?
        .unwrap();
    let r2 = store
        .open_inbox(Some("t2"), "message", Some("BÁO CÁO"), "r2", 2)?
        .unwrap();
    let q1 = store
        .open_inbox(Some("t1"), "message", Some("HỎI"), "q1", 3)?
        .unwrap();
    let bl = store
        .open_inbox(Some("t1"), "blocked", Some("question"), "wait", 4)?
        .unwrap();

    // --kind REPORT and Vietnamese same result
    let f_en = DoneFilters {
        kind: normalize_done_kind("REPORT"),
        ..Default::default()
    };
    let rep_en = dismiss_done(&store, vec![], f_en, false, false, 10)?;
    assert_eq!(rep_en.done.len(), 2);
    assert!(rep_en.done.contains(&r1) && rep_en.done.contains(&r2));

    // recreate for Vietnamese check
    let store2 = Store::open_in_memory()?;
    let r1b = store2
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r1", 1)?
        .unwrap();
    let r2b = store2
        .open_inbox(Some("t2"), "message", Some("BÁO CÁO"), "r2", 2)?
        .unwrap();
    let _q1b = store2
        .open_inbox(Some("t1"), "message", Some("HỎI"), "q1", 3)?
        .unwrap();
    let f_vi = DoneFilters {
        kind: normalize_done_kind("BÁO CÁO"),
        ..Default::default()
    };
    let rep_vi = dismiss_done(&store2, vec![], f_vi, false, false, 10)?;
    assert_eq!(rep_vi.done, vec![r1b, r2b]);

    // terminal + kind AND
    let _ = (r1, r2, q1, bl);
    let store3 = Store::open_in_memory()?;
    let a = store3
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r1", 1)?
        .unwrap();
    let _b = store3
        .open_inbox(Some("t2"), "message", Some("BÁO CÁO"), "r2", 2)?
        .unwrap();
    let f_and = DoneFilters {
        kind: normalize_done_kind("REPORT"),
        terminal: Some("t1".into()),
        ..Default::default()
    };
    let rep_and = dismiss_done(&store3, vec![], f_and, false, false, 10)?;
    assert_eq!(rep_and.done, vec![a]);

    // --source blocked closes blocked; filter-only without explicit should skip (handled above) but explicit closes
    let store4 = Store::open_in_memory()?;
    let bl2 = store4
        .open_inbox(Some("t1"), "blocked", Some("question"), "wait", 1)?
        .unwrap();
    let f_src = DoneFilters {
        source: Some("blocked".into()),
        ..Default::default()
    };
    let rep_src = dismiss_done(&store4, vec![], f_src, true, false, 10)?;
    assert_eq!(rep_src.done, vec![bl2]);

    // single ID of blocked item closes it (legacy single)
    let store5 = Store::open_in_memory()?;
    let bl3 = store5
        .open_inbox(Some("t1"), "blocked", Some("question"), "wait", 1)?
        .unwrap();
    let rep_single = dismiss_done(&store5, vec![bl3], DoneFilters::default(), false, true, 10)?;
    assert_eq!(rep_single.done, vec![bl3]);
    assert!(store5.inbox_item(bl3)?.unwrap().resolved_ms.is_some());
    Ok(())
}

#[test]
fn spawn_normalizes_role_and_cap_applies() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let mut cfg = Config::builtin();
    cfg.policy.max_workers = 1;
    // Vai trò lưu trong DB có thể chưa chuẩn hóa vẫn được tính.
    term(&store, "t1", "opencode", Some(" Worker "));
    let req = SpawnReq {
        template: "worker-mid".into(),
        task: "a".into(),
        role: " Worker ".into(),
        ..Default::default()
    };
    let err = actions::spawn(&store, &hc, &cfg, &req, 1)
        .unwrap_err()
        .to_string();
    assert!(err.contains("max_workers"), "{err}");
    assert!(hc.calls.borrow().is_empty());

    let store = Store::open_in_memory()?;
    let addr = actions::spawn(&store, &hc, &cfg, &req, 1)?;
    assert_eq!(
        store.terminal(&addr)?.unwrap().role.as_deref(),
        Some("worker")
    );
    assert!(hc.calls.borrow()[0].contains("\"agm.role\": \"worker\""));

    let empty = SpawnReq {
        role: "  ".into(),
        ..req
    };
    let err = actions::spawn(&store, &hc, &cfg, &empty, 2)
        .unwrap_err()
        .to_string();
    assert!(err.contains("không được rỗng"), "{err}");
    Ok(())
}

#[test]
fn reply_to_mcp_agent_without_terminal_is_explained() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let id = store
        .open_inbox(Some("agent"), "message", Some("note"), "xin chào", 1)?
        .unwrap();
    let err = actions::reply(&store, &hc, id, "ok", 2)
        .unwrap_err()
        .to_string();
    assert!(err.contains("không có terminal để trả lời"), "{err}");
    assert!(err.contains(&format!("agm done {id}")), "{err}");
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn exit_code_distinguishes_spawned_without_brief_from_other_errors() {
    let plain = anyhow::anyhow!("hc run lỗi");
    assert_eq!(actions::exit_code(&plain), 1);
    // Vẫn nhận ra khi lỗi đã bị bọc thêm context.
    let undelivered = anyhow::Error::new(actions::BriefUndelivered {
        address: "t7".into(),
        detail: "agent chưa sẵn sàng".into(),
    })
    .context("agm_spawn");
    assert_eq!(
        actions::exit_code(&undelivered),
        actions::EXIT_BRIEF_UNDELIVERED
    );
    assert_ne!(actions::EXIT_BRIEF_UNDELIVERED, 1);
    assert_ne!(actions::EXIT_BRIEF_UNDELIVERED, 2); // clap dùng 2 cho lỗi cú pháp
}
