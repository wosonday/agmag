use agmag::daemon::apply_event;
use agmag::hc::{app_from_argv, terminal_row_from};
use agmag::store::Store;
use serde_json::json;

fn ev(kind: &str, terminal: &str, data: serde_json::Value) -> serde_json::Value {
    json!({"jsonrpc": "2.0", "method": "event", "params": {"type": kind, "terminal": terminal, "seq": 1, "data": data}})
}

fn created(addr: &str) -> serde_json::Value {
    ev(
        "created",
        addr,
        json!({"address": addr, "argv": ["opencode", "run"], "cwd": "D:\\Project\\agmag\\.wt\\t5", "created_ms": 100,
               "labels": {"agm.task": "t5", "agm.role": "worker"}, "name": "w-t5-1", "parent": null,
               "reported": false, "state": "live", "template": "worker-mid"}),
    )
}

#[test]
fn created_upserts_terminal_with_task_and_role() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &created("t20"), 1000)?;
    let t = store.terminal("t20")?.unwrap();
    assert_eq!(t.name.as_deref(), Some("w-t5-1"));
    assert_eq!(
        (t.task.as_deref(), t.role.as_deref()),
        (Some("t5"), Some("worker"))
    );
    assert_eq!(t.app.as_deref(), Some("opencode"));
    assert_eq!(t.template.as_deref(), Some("worker-mid"));
    Ok(())
}

#[test]
fn blocked_question_opens_inbox_with_full_text() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &created("t1s"), 1000)?;
    let q = "answer: Tôi nên xử lý thế nào? (A · B · C)";
    let blocked = ev(
        "status",
        "t1s",
        json!({"records": {}, "reported": true,
        "status": {"app": "claude-code", "kind": "question", "msg": q, "state": "blocked"}}),
    );
    apply_event(&store, &blocked, 2000)?;
    apply_event(&store, &blocked, 2001)?; // lặp lại không nhân đôi
    let inbox = store.inbox(true)?;
    assert_eq!(inbox.len(), 1);
    assert_eq!(
        (
            inbox[0].terminal.as_deref(),
            inbox[0].kind.as_deref(),
            inbox[0].text.as_str()
        ),
        (Some("t1s"), Some("question"), q)
    );
    assert_eq!(
        store.terminal("t1s")?.unwrap().app.as_deref(),
        Some("claude-code")
    );
    Ok(())
}

#[test]
fn unblocked_resolves_inbox() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(
        &store,
        &ev(
            "status",
            "t1q",
            json!({"reported": true, "status": {"app": "codex", "kind": "permission", "state": "blocked"}}),
        ),
        1,
    )?;
    assert_eq!(store.inbox(true)?.len(), 1);
    assert_eq!(store.inbox(true)?[0].text, "codex đang chờ: permission");
    apply_event(
        &store,
        &ev(
            "status",
            "t1q",
            json!({"reported": true, "status": {"app": "codex", "state": "working"}}),
        ),
        2,
    )?;
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn turn_and_finished() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &created("t20"), 1)?;
    apply_event(
        &store,
        &ev("turn", "t20", json!({"state": "idle", "turn": 11})),
        2,
    )?;
    assert_eq!(store.terminal("t20")?.unwrap().turn, 11);
    apply_event(
        &store,
        &ev(
            "finished",
            "t20",
            json!({"exit_code": 1, "finished_ms": 99, "reason": "killed", "signal": null}),
        ),
        3,
    )?;
    let t = store.terminal("t20")?.unwrap();
    assert_eq!(
        (t.state.as_str(), t.exit_code, t.finished_ms),
        ("finished", Some(1), Some(99))
    );
    Ok(())
}

#[test]
fn snapshot_line_upserts_all_and_labels_event() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let snap = json!({"channel": null, "seq": 5, "terminals": [
        {"address": "t3", "argv": ["pwsh.exe"], "cwd": "D:\\Project", "created_ms": 1, "labels": {}, "state": "live", "reported": false},
        {"address": "t11", "argv": ["claude"], "cwd": "D:\\Project\\MATHY-AI", "created_ms": 2, "labels": {}, "state": "live",
         "reported": true, "status": {"app": "claude-code", "state": "working"}}
    ]});
    apply_event(&store, &snap, 10)?;
    assert_eq!(store.terminals(true)?.len(), 2);
    assert_eq!(
        store.terminal("t11")?.unwrap().status.as_deref(),
        Some("working")
    );
    apply_event(
        &store,
        &ev(
            "labels",
            "t3",
            json!({"labels": {"agm.task": "x", "where": "MATHY-AI / 1"}}),
        ),
        11,
    )?;
    assert_eq!(store.terminal("t3")?.unwrap().task.as_deref(), Some("x"));
    apply_event(
        &store,
        &json!({"jsonrpc": "2.0", "method": "heartbeat", "params": {"seq": 6}}),
        12,
    )?;
    Ok(())
}

#[test]
fn app_detection() {
    assert_eq!(
        app_from_argv(&["C:\\x\\codex.EXE".into(), "exec".into()]).as_deref(),
        Some("codex")
    );
    assert_eq!(
        app_from_argv(&["claude".into()]).as_deref(),
        Some("claude-code")
    );
    assert_eq!(
        app_from_argv(&["opencode".into(), "run".into()]).as_deref(),
        Some("opencode")
    );
    assert_eq!(
        app_from_argv(&["agy".into()]).as_deref(),
        Some("antigravity")
    );
    assert_eq!(app_from_argv(&["pwsh.exe".into()]).as_deref(), Some("pwsh"));
    let row = terminal_row_from(&json!({"address": "t1", "state": "live"}));
    assert_eq!((row.address.as_str(), row.app), ("t1", None));
}

#[test]
fn app_detection_windows_shims_and_agent_field() {
    for (argv0, want) in [
        ("C:\\npm\\claude.cmd", "claude-code"),
        ("codex.CMD", "codex"),
        ("opencode.ps1", "opencode"),
        ("agy.bat", "antigravity"),
    ] {
        assert_eq!(
            app_from_argv(&[argv0.into()]).as_deref(),
            Some(want),
            "{argv0}"
        );
    }
    // `hc ls` cho trường agent = tên chương trình; cũng phải qua cùng bộ chuẩn hóa.
    assert_eq!(
        terminal_row_from(&json!({"address": "t1", "agent": "claude", "state": "live"}))
            .app
            .as_deref(),
        Some("claude-code")
    );
}

#[test]
fn snapshot_finishes_terminals_missing_from_core() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &created("t20"), 1)?;
    apply_event(&store, &created("t21"), 1)?;
    // Daemon khởi động lại: ảnh chụp chỉ còn t21 → t20 đã chết khi daemon tắt.
    let snap = json!({"channel": null, "seq": 9, "terminals": [{"address": "t21", "argv": ["opencode"], "state": "live", "labels": {}}]});
    apply_event(&store, &snap, 50)?;
    assert_eq!(store.terminal("t20")?.unwrap().state, "finished");
    assert_eq!(store.terminal("t21")?.unwrap().state, "live");
    Ok(())
}

#[test]
fn null_status_and_removed_resolve_inbox() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let blocked = ev(
        "status",
        "t1",
        json!({"reported": true, "status": {"app": "codex", "kind": "permission", "state": "blocked"}}),
    );
    apply_event(&store, &blocked, 1)?;
    apply_event(
        &store,
        &ev("status", "t1", json!({"reported": true, "status": null})),
        2,
    )?;
    assert!(store.inbox(true)?.is_empty());
    apply_event(
        &store,
        &ev(
            "status",
            "t2",
            json!({"reported": true, "status": {"app": "codex", "state": "blocked"}}),
        ),
        3,
    )?;
    apply_event(&store, &ev("removed", "t2", json!({})), 4)?;
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn hc_run_puts_json_flag_before_command() {
    let spec = agmag::hc::RunSpec {
        argv: vec!["worker-mid".into(), "-x làm tiếp".into()],
        name: Some("w-a-1".into()),
        ..Default::default()
    };
    let args = agmag::hc::run_args(&spec);
    let json_at = args.iter().position(|a| a == "--json").unwrap();
    let cmd_at = args.iter().position(|a| a == "worker-mid").unwrap();
    assert!(json_at < cmd_at, "{args:?}");
    assert_eq!(args.last().map(String::as_str), Some("-x làm tiếp"));
    let send = agmag::hc::send_args("t5", "-bắt đầu bằng gạch", true, false);
    let dashdash = send.iter().position(|a| a == "--").unwrap();
    assert!(
        send.iter().position(|a| a == "--json").unwrap() < dashdash,
        "{send:?}"
    );
    assert_eq!(&send[dashdash + 1..], ["t5", "-bắt đầu bằng gạch"]);
}

#[test]
fn empty_app_strings_fall_back_to_argv() {
    let row = terminal_row_from(
        &json!({"address": "t1", "argv": ["opencode"], "status": {"app": "", "state": "idle"}, "state": "live"}),
    );
    assert_eq!(row.app.as_deref(), Some("opencode"));
    assert_eq!(app_from_argv(&["".into()]), None);
}

/// herdr's location is a label in snapshots/events and a top-level field in `hc ls`; both end up in labels.
#[test]
fn herdr_where_is_kept_from_labels_or_hc_ls() {
    let from_event = agmag::hc::terminal_row_from(&serde_json::json!({
        "address": "t11", "state": "live", "labels": {"where": "MATHY-AI / 1"}
    }));
    assert_eq!(
        from_event.labels.get("where").map(String::as_str),
        Some("MATHY-AI / 1")
    );
    let from_ls = agmag::hc::terminal_row_from(&serde_json::json!({
        "address": "t1s", "state": "live", "where": "agmag / agm-ceo"
    }));
    assert_eq!(
        from_ls.labels.get("where").map(String::as_str),
        Some("agmag / agm-ceo")
    );
    // A label already present wins over the top-level copy.
    let both = agmag::hc::terminal_row_from(&serde_json::json!({
        "address": "t2", "where": "x / y", "labels": {"where": "a / b"}
    }));
    assert_eq!(both.labels.get("where").map(String::as_str), Some("a / b"));
}
