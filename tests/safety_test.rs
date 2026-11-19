use std::cell::RefCell;
use std::collections::BTreeMap;

use agmag::actions::{self, Caller};
use agmag::config::Config;
use agmag::git::Git;
use agmag::hc::{Hc, RunSpec};
use agmag::launch::parse_launch_spec;
use agmag::model::Source;
use agmag::rotation;
use agmag::store::{Store, TerminalRow};
use anyhow::{bail, Result};
use serde_json::{json, Value};

#[derive(Default)]
struct SafetyHc {
    calls: RefCell<Vec<String>>,
    responses: BTreeMap<String, Value>,
    fail_get: bool,
}

fn live(address: &str) -> Value {
    json!({"address":address, "state":"live", "created_ms":1,
        "status":{"app":"codex","state":"done"}})
}

impl Hc for SafetyHc {
    fn ls(&self, _: bool) -> Result<Vec<Value>> { Ok(vec![]) }
    fn get(&self, target: &str) -> Result<Value> {
        self.calls.borrow_mut().push(format!("get {target}"));
        if self.fail_get { bail!("unavailable"); }
        Ok(self.responses.get(target).cloned().unwrap_or_else(|| live(target)))
    }
    fn send(&self, target: &str, _: &str, if_idle: bool, _: bool) -> Result<Value> {
        self.calls.borrow_mut().push(format!("send {target} if_idle={if_idle}"));
        Ok(json!({}))
    }
    fn run(&self, _: &RunSpec) -> Result<Value> { bail!("unexpected run") }
    fn kill(&self, target: &str) -> Result<()> {
        self.calls.borrow_mut().push(format!("kill {target}"));
        Ok(())
    }
    fn interrupt(&self, _: &str) -> Result<()> { bail!("unexpected interrupt") }
}

struct NoGit;
impl Git for NoGit {
    fn head(&self, _: &str) -> Result<String> { bail!("unused") }
    fn is_clean(&self, _: &str) -> Result<bool> { bail!("unused") }
    fn touched_since(&self, _: &str, _: &str, _: &str) -> Result<Option<String>> { bail!("unused") }
}

#[test]
fn handoff_requires_successful_fresh_status_read() -> Result<()> {
    for response in [None, Some(json!({"address":"t1"})), Some(json!({
        "address":"t1", "state":"finished", "status":{"app":"codex","state":"done"}
    }))] {
        let tmp = tempfile::tempdir()?;
        let store = Store::open_in_memory()?;
        let cwd = tmp.path().to_string_lossy().to_string();
        store.upsert_terminal(&TerminalRow {
            address: "t1".into(), app: Some("codex".into()), cwd: Some(cwd.clone()),
            state: "live".into(), ..Default::default()
        })?;
        store.set_terminal_launch("t1", true, Some(&parse_launch_spec("codex", &["codex".into()], &cwd)?.to_json()?))?;
        store.set_terminal_status("t1", Some("done"), None, None, 1, 0)?;
        rotation::start(&store, "t1", None, true, 1)?;
        let mut hc = SafetyHc { fail_get: response.is_none(), ..Default::default() };
        if let Some(value) = response { hc.responses.insert("t1".into(), value); }
        rotation::tick(&store, &Config::builtin(), &hc, &NoGit, 2)?;
        assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("send ")), "{:?}", hc.calls.borrow());
    }
    Ok(())
}

fn cleanup_fixture() -> Result<Store> {
    let store = Store::open_in_memory()?;
    for address in ["t1", "t2"] {
        store.upsert_terminal(&TerminalRow {
            address: address.into(), app: Some("codex".into()), state: "live".into(),
            created_ms: 1, ..Default::default()
        })?;
        let sid = store.session_id_for_path(Source::Codex, &format!("codex://{address}"))?;
        store.link_terminal(address, sid, "rotation")?;
    }
    store.set_terminal_handoff_state("t1", "handed_off")?;
    store.conn().execute(
        "INSERT INTO rotations (old_terminal,new_terminal,app,state,updated_ms) VALUES ('t1','t2','codex','done',1)", []
    )?;
    Ok(store)
}

#[test]
fn cleanup_preserves_old_when_successor_live_state_is_stale() -> Result<()> {
    let store = cleanup_fixture()?;
    let items = actions::eligible_cleanups(&store)?;
    let hc = SafetyHc {
        responses: [("t2".into(), json!({"address":"t2","state":"finished","status":{"app":"codex","state":"done"}}))].into(),
        ..Default::default()
    };
    let report = actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 2)?;
    assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill ")), "{:?}", hc.calls.borrow());
    assert_eq!(report.skipped.len(), 1);
    Ok(())
}

#[test]
fn cleanup_preserves_old_when_live_reads_fail() -> Result<()> {
    let store = cleanup_fixture()?;
    let items = actions::eligible_cleanups(&store)?;
    let hc = SafetyHc { fail_get: true, ..Default::default() };
    actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 2)?;
    assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill ")), "{:?}", hc.calls.borrow());
    Ok(())
}

#[test]
fn cleanup_requires_known_matching_live_identity() -> Result<()> {
    for target in ["t1", "t2"] {
        for field in ["state", "app", "wrong_app", "created_ms", "missing_created_ms", "address"] {
            let store = cleanup_fixture()?;
            let items = actions::eligible_cleanups(&store)?;
            let mut value = live(target);
            match field {
                "state" => { value.as_object_mut().unwrap().remove("state"); }
                "app" => { value["status"].as_object_mut().unwrap().remove("app"); }
                "wrong_app" => value["status"]["app"] = json!("pwsh"),
                "created_ms" => value["created_ms"] = json!(999),
                "missing_created_ms" => { value.as_object_mut().unwrap().remove("created_ms"); }
                "address" => value["address"] = json!("different-terminal"),
                _ => unreachable!(),
            }
            let hc = SafetyHc { responses: [(target.into(), value)].into(), ..Default::default() };
            actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 2)?;
            assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill ")), "{target}/{field}: {:?}", hc.calls.borrow());
        }
    }
    Ok(())
}

#[test]
fn cleanup_rechecks_frozen_rotation_relationship() -> Result<()> {
    for mutation in [
        "UPDATE rotations SET state='needs_attention'",
        "UPDATE rotations SET new_terminal='t3'",
        "UPDATE rotations SET old_terminal='t3'",
        "UPDATE rotations SET app='claude-code'",
        "DELETE FROM rotations",
    ] {
        let store = cleanup_fixture()?;
        let items = actions::eligible_cleanups(&store)?;
        store.conn().execute(mutation, [])?;
        let hc = SafetyHc::default();
        actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 2)?;
        assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill ")), "{mutation}: {:?}", hc.calls.borrow());
    }
    Ok(())
}

#[test]
fn cleanup_kills_only_with_fresh_old_and_successor_identity() -> Result<()> {
    let store = cleanup_fixture()?;
    let items = actions::eligible_cleanups(&store)?;
    let hc = SafetyHc::default();
    let report = actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 2)?;
    assert_eq!(report.done, 1);
    let calls = hc.calls.borrow();
    assert!(calls.iter().any(|c| c == "get t2"), "{calls:?}");
    assert_eq!(calls.iter().filter(|c| c.starts_with("kill ")).collect::<Vec<_>>(), ["kill t1"]);
    Ok(())
}

#[test]
fn exited_only_cleanup_never_contacts_hc_and_keeps_history() -> Result<()> {
    let store = cleanup_fixture()?;
    store.finish_terminal("t1", Some(0), 2)?;
    store.remove_terminal("t2")?;
    store.open_inbox(Some("t1"), "rotation", Some("done"), "handoff", 1)?;
    let items = actions::eligible_cleanups(&store)?;
    let hc = SafetyHc { fail_get: true, ..Default::default() };
    let report = actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 3)?;
    assert_eq!(report.done, 1);
    assert!(hc.calls.borrow().is_empty());
    assert_eq!(store.conn().query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get::<_, i64>(0))?, 1);
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn newly_finished_old_cleanup_resolves_inbox_without_killing() -> Result<()> {
    let store = cleanup_fixture()?;
    store.open_inbox(Some("t1"), "rotation", Some("done"), "handoff", 1)?;
    store.open_inbox(Some("t1"), "context", None, "context", 1)?;
    let items = actions::eligible_cleanups(&store)?;
    let mut value = live("t1");
    value["state"] = json!("finished");
    let hc = SafetyHc { responses: [("t1".into(), value)].into(), ..Default::default() };
    let report = actions::execute_cleanup_batch(&store, &hc, &Caller::User { confirmed: true }, &items, 3)?;
    assert_eq!(report.done, 1);
    assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill ")));
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn cleanup_rejects_agents_and_unconfirmed_users_without_contacting_hc() -> Result<()> {
    for caller in [Caller::Agent(Some("t9".into())), Caller::User { confirmed: false }] {
        let store = cleanup_fixture()?;
        let items = actions::eligible_cleanups(&store)?;
        let hc = SafetyHc::default();
        assert!(actions::execute_cleanup_batch(&store, &hc, &caller, &items, 3).is_err());
        assert!(hc.calls.borrow().is_empty());
    }
    Ok(())
}
