mod common;

use std::cell::RefCell;

use agmag::config::Config;
use agmag::git::Git;
use agmag::model::{ContextSnapshot, Source};
use agmag::rotation;
use agmag::store::{Store, TerminalRow};
use anyhow::Result;
use common::FakeHc;

#[derive(Default)]
struct FakeGit {
    touched: Option<String>,
    clean: bool,
    asked: RefCell<Vec<String>>,
}

impl Git for FakeGit {
    fn head(&self, _cwd: &str) -> Result<String> {
        Ok("base".into())
    }
    fn is_clean(&self, _cwd: &str) -> Result<bool> {
        Ok(self.clean)
    }
    fn touched_since(&self, _cwd: &str, since: &str, path: &str) -> Result<Option<String>> {
        self.asked.borrow_mut().push(format!("{since} {path}"));
        Ok(self.touched.clone())
    }
}

const MIN: i64 = 60_000;

fn setup(app: &str, status: &str) -> Result<Store> {
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        name: Some("w-a-1".into()),
        app: Some(app.into()),
        cwd: Some(r"D:\w".into()),
        task: Some("a".into()),
        role: Some("worker".into()),
        template: Some("worker-mid".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_status("t1", Some(status), None, None, 5, 0)?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://s")?;
    store.set_context(sid, &ContextSnapshot { ts_ms: 1, used_tokens: 380_000, window_tokens: Some(475_000) })?; // 80%
    store.link_terminal("t1", sid, "auto")?;
    Ok(store)
}

fn state(store: &Store) -> String {
    store.conn().query_row("SELECT state FROM rotations", [], |r| r.get(0)).unwrap()
}

fn go_idle(store: &Store, turn: i64) {
    store.set_terminal_status("t1", Some("idle"), None, None, turn, 0).unwrap();
}

#[test]
fn rotation_waits_while_working() -> Result<()> {
    let store = setup("opencode", "working")?;
    let hc = FakeHc::default();
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 0)?;
    assert_eq!(state(&store), "pending");
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn rotation_requests_handoff_when_idle() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 0)?;
    assert_eq!(state(&store), "requested");
    let calls = hc.calls.borrow();
    assert!(calls[0].starts_with("send t1 if_idle=true") && calls[0].contains(".agents/log/a.md"), "{calls:?}");
    Ok(())
}

#[test]
fn rotation_kills_and_respawns_after_commit() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit { touched: Some("abc123".into()), clean: true, ..Default::default() };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, MIN)?; // chưa có lượt mới → chờ
    assert_eq!(state(&store), "requested");
    go_idle(&store, 6);
    rotation::tick(&store, &Config::builtin(), &hc, &git, 2 * MIN)?;
    assert_eq!(state(&store), "done");
    assert_eq!(git.asked.borrow()[0], "base .agents/log/a.md");
    let calls = hc.calls.borrow();
    assert_eq!(calls[1], "kill t1");
    assert!(calls[2].starts_with("run [\"worker-mid\"") && calls[2].contains("w-a-2") && calls[2].contains(r"D:\\w"), "{calls:?}");
    let new: String = store.conn().query_row("SELECT new_terminal FROM rotations", [], |r| r.get(0))?;
    assert_eq!(new, "t99");
    Ok(())
}

#[test]
fn rotation_fails_to_inbox_without_commit() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit { touched: None, clean: true, ..Default::default() };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    go_idle(&store, 6);
    rotation::tick(&store, &Config::builtin(), &hc, &git, MIN)?;
    assert_eq!(state(&store), "failed");
    assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill")));
    assert!(store.inbox(true)?.iter().any(|i| i.source == "rotation" && i.terminal.as_deref() == Some("t1")));
    Ok(())
}

#[test]
fn rotation_timeout_to_inbox() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit { touched: Some("x".into()), clean: true, ..Default::default() };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    store.set_terminal_status("t1", Some("working"), None, None, 5, 0)?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, 11 * MIN)?;
    assert_eq!(state(&store), "failed");
    assert!(store.inbox(true)?.iter().any(|i| i.source == "rotation"));
    Ok(())
}

#[test]
fn rotation_only_for_opencode_workers() -> Result<()> {
    for app in ["codex", "claude-code"] {
        let store = setup(app, "idle")?;
        let hc = FakeHc::default();
        rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 0)?;
        let n: i64 = store.conn().query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get(0))?;
        assert_eq!(n, 0, "{app}");
        assert!(hc.calls.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn rotation_respects_policy_off() -> Result<()> {
    let store = setup("opencode", "idle")?;
    store.set_setting("policy", "off")?;
    let hc = FakeHc::default();
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 0)?;
    let n: i64 = store.conn().query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get(0))?;
    assert_eq!(n, 0);
    Ok(())
}
