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
    assert_eq!((t.task.as_deref(), t.role.as_deref()), (Some("t5"), Some("worker")));
    assert_eq!(t.app.as_deref(), Some("opencode"));
    assert_eq!(t.template.as_deref(), Some("worker-mid"));
    Ok(())
}

#[test]
fn blocked_question_opens_inbox_with_full_text() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &created("t1s"), 1000)?;
    let q = "answer: Tôi nên xử lý thế nào? (A · B · C)";
    let blocked = ev("status", "t1s", json!({"records": {}, "reported": true,
        "status": {"app": "claude-code", "kind": "question", "msg": q, "state": "blocked"}}));
    apply_event(&store, &blocked, 2000)?;
    apply_event(&store, &blocked, 2001)?; // lặp lại không nhân đôi
    let inbox = store.inbox(true)?;
    assert_eq!(inbox.len(), 1);
    assert_eq!((inbox[0].terminal.as_deref(), inbox[0].kind.as_deref(), inbox[0].text.as_str()), (Some("t1s"), Some("question"), q));
    assert_eq!(store.terminal("t1s")?.unwrap().app.as_deref(), Some("claude-code"));
    Ok(())
}

#[test]
fn unblocked_resolves_inbox() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &ev("status", "t1q", json!({"reported": true, "status": {"app": "codex", "kind": "permission", "state": "blocked"}})), 1)?;
    assert_eq!(store.inbox(true)?.len(), 1);
    assert_eq!(store.inbox(true)?[0].text, "codex đang chờ: permission");
    apply_event(&store, &ev("status", "t1q", json!({"reported": true, "status": {"app": "codex", "state": "working"}})), 2)?;
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn turn_and_finished() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    apply_event(&store, &created("t20"), 1)?;
    apply_event(&store, &ev("turn", "t20", json!({"state": "idle", "turn": 11})), 2)?;
    assert_eq!(store.terminal("t20")?.unwrap().turn, 11);
    apply_event(&store, &ev("finished", "t20", json!({"exit_code": 1, "finished_ms": 99, "reason": "killed", "signal": null})), 3)?;
    let t = store.terminal("t20")?.unwrap();
    assert_eq!((t.state.as_str(), t.exit_code, t.finished_ms), ("finished", Some(1), Some(99)));
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
    assert_eq!(store.terminal("t11")?.unwrap().status.as_deref(), Some("working"));
    apply_event(&store, &ev("labels", "t3", json!({"labels": {"agm.task": "x", "where": "MATHY-AI / 1"}})), 11)?;
    assert_eq!(store.terminal("t3")?.unwrap().task.as_deref(), Some("x"));
    apply_event(&store, &json!({"jsonrpc": "2.0", "method": "heartbeat", "params": {"seq": 6}}), 12)?;
    Ok(())
}

#[test]
fn app_detection() {
    assert_eq!(app_from_argv(&["C:\\x\\codex.EXE".into(), "exec".into()]).as_deref(), Some("codex"));
    assert_eq!(app_from_argv(&["claude".into()]).as_deref(), Some("claude-code"));
    assert_eq!(app_from_argv(&["opencode".into(), "run".into()]).as_deref(), Some("opencode"));
    assert_eq!(app_from_argv(&["agy".into()]).as_deref(), Some("antigravity"));
    assert_eq!(app_from_argv(&["pwsh.exe".into()]).as_deref(), Some("pwsh"));
    let row = terminal_row_from(&json!({"address": "t1", "state": "live"}));
    assert_eq!((row.address.as_str(), row.app), ("t1", None));
}
