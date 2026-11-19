mod common;

use agmag::config::Config;
use agmag::model::{ContextSnapshot, QuotaSnapshot, QuotaWindow, Source, Usage, UsageRecord};
use agmag::policy::{self, spawn_blocked_by_budget, task_tokens};
use agmag::store::{BudgetRow, Store, TerminalRow};
use anyhow::Result;
use common::FakeHc;

/// Terminal của đầu việc `task`, liên kết với một phiên đã tiêu `output` token.
fn worker(store: &Store, addr: &str, app: &str, task: &str, output: u64) -> i64 {
    store
        .upsert_terminal(&TerminalRow {
            address: addr.into(),
            name: Some(format!("w-{addr}")),
            app: Some(app.into()),
            task: Some(task.into()),
            role: Some("worker".into()),
            state: "live".into(),
            ..Default::default()
        })
        .unwrap();
    let sid = store.session_id_for_path(Source::OpenCode, &format!("opencode://{addr}")).unwrap();
    store
        .upsert_usage(sid, &UsageRecord { dedupe_key: "k".into(), ts_ms: 1, model: Some("gpt-6.1-sol".into()), usage: Usage { output, ..Default::default() } })
        .unwrap();
    store.link_terminal(addr, sid, "auto").unwrap();
    sid
}

fn budget(store: &Store, scope: &str, unit: &str, soft: f64, hard: f64, action: &str) {
    store.set_budget(&BudgetRow { scope: scope.into(), unit: unit.into(), soft, hard, action: action.into() }).unwrap();
}

#[test]
fn task_tokens_sums_linked_sessions() -> Result<()> {
    let store = Store::open_in_memory()?;
    worker(&store, "t1", "opencode", "a", 100);
    worker(&store, "t2", "opencode", "a", 50);
    worker(&store, "t3", "opencode", "b", 7);
    assert_eq!(task_tokens(&store, "a")?, 150);
    Ok(())
}

#[test]
fn task_soft_budget_opens_inbox_once() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    worker(&store, "t1", "opencode", "a", 120);
    budget(&store, "task:a", "tokens", 100.0, 1000.0, "notify");
    policy::tick(&store, &Config::builtin(), &hc, 1)?;
    policy::tick(&store, &Config::builtin(), &hc, 2)?;
    let inbox = store.inbox(true)?;
    assert_eq!(inbox.len(), 1);
    assert!(inbox[0].text.contains("task:a") && inbox[0].text.contains("mềm"), "{}", inbox[0].text);
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn manager_is_notified_when_configured() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    worker(&store, "t1", "opencode", "a", 120);
    budget(&store, "task:a", "tokens", 100.0, 1000.0, "notify");
    let mut cfg = Config::builtin();
    cfg.policy.manager = Some("codex-mgr".into());
    policy::tick(&store, &cfg, &hc, 1)?;
    policy::tick(&store, &cfg, &hc, 2)?;
    let calls = hc.calls.borrow();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].starts_with("send codex-mgr if_idle=true"));
    Ok(())
}

#[test]
fn hard_budget_blocks_spawn() -> Result<()> {
    let store = Store::open_in_memory()?;
    worker(&store, "t1", "opencode", "a", 500);
    assert_eq!(spawn_blocked_by_budget(&store, "a")?, None);
    budget(&store, "task:a", "tokens", 100.0, 400.0, "notify");
    let reason = spawn_blocked_by_budget(&store, "a")?.unwrap();
    assert!(reason.contains("400"), "{reason}");
    Ok(())
}

#[test]
fn hard_action_never_kills_codex_claude() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    worker(&store, "t1", "codex", "a", 300);
    worker(&store, "t2", "opencode", "a", 300);
    budget(&store, "task:a", "tokens", 100.0, 400.0, "kill");
    policy::tick(&store, &Config::builtin(), &hc, 1)?;
    assert_eq!(hc.calls.borrow().as_slice(), ["kill t2"]);
    assert!(store.inbox(true)?.iter().any(|i| i.source == "request" && i.terminal.as_deref() == Some("t1")));
    Ok(())
}

#[test]
fn provider_quota_budget() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    store.insert_quota(&QuotaSnapshot { provider: "codex".into(), ts_ms: 1, windows: vec![QuotaWindow { window_minutes: 10080, used_percent: 86.0, resets_at_s: 9 }] })?;
    budget(&store, "provider:codex:10080", "percent", 80.0, 95.0, "notify");
    policy::tick(&store, &Config::builtin(), &hc, 2)?;
    assert!(store.inbox(true)?[0].text.contains("provider:codex:10080"));
    Ok(())
}

#[test]
fn provider_budget_ignores_quota_whose_window_has_reset() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    // 96% used, window resets at t = 9 s; nothing newer has been logged since.
    store.insert_quota(&QuotaSnapshot { provider: "codex".into(), ts_ms: 1, windows: vec![QuotaWindow { window_minutes: 10080, used_percent: 96.0, resets_at_s: 9 }] })?;
    budget(&store, "provider:codex:10080", "percent", 80.0, 95.0, "notify");
    let b = store.budgets()?.into_iter().next().unwrap();
    assert_eq!(policy::budget_value(&store, &b, 5_000)?, Some(96.0), "before the reset the logged value counts");
    assert_eq!(policy::budget_value(&store, &b, 10_000)?, Some(0.0), "after the reset the old 96% must not count");
    policy::tick(&store, &Config::builtin(), &hc, 10_000)?;
    assert!(store.inbox(true)?.is_empty(), "no budget alarm from a stale, already-reset window");
    Ok(())
}

#[test]
fn ctx_warn_at_50() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    let sid = worker(&store, "t1", "opencode", "a", 1);
    store.set_context(sid, &ContextSnapshot { ts_ms: 1, used_tokens: 240_000, window_tokens: Some(475_000) })?;
    policy::tick(&store, &Config::builtin(), &hc, 2)?;
    let inbox = store.inbox(true)?;
    assert_eq!(inbox.len(), 1);
    assert_eq!((inbox[0].source.as_str(), inbox[0].terminal.as_deref()), ("context", Some("t1")));
    Ok(())
}

#[test]
fn policy_off_does_nothing() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    worker(&store, "t2", "opencode", "a", 300);
    budget(&store, "task:a", "tokens", 100.0, 200.0, "kill");
    store.set_setting("policy", "off")?;
    policy::tick(&store, &Config::builtin(), &hc, 1)?;
    assert!(hc.calls.borrow().is_empty());
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}

#[test]
fn dismissed_budget_alert_does_not_fire_again() -> Result<()> {
    let store = Store::open_in_memory()?;
    let hc = FakeHc::default();
    worker(&store, "t2", "opencode", "a", 300);
    budget(&store, "task:a", "tokens", 100.0, 200.0, "kill");
    policy::tick(&store, &Config::builtin(), &hc, 1)?;
    assert_eq!(hc.calls.borrow().len(), 1);
    for item in store.inbox(true)? {
        store.resolve_inbox(item.id, None, 2)?;
    }
    policy::tick(&store, &Config::builtin(), &hc, 3)?;
    policy::tick(&store, &Config::builtin(), &hc, 4)?;
    assert_eq!(hc.calls.borrow().len(), 1, "{:?}", hc.calls.borrow());
    assert!(store.inbox(true)?.is_empty());
    Ok(())
}
