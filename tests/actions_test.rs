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
