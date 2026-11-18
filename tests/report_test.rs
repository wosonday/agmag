use agmag::config::{Billing, Config, ModelCfg, Price, ProviderCfg};
use agmag::ingest::ingest_file;
use agmag::model::{ContextSnapshot, QuotaSnapshot, QuotaWindow, Role, Source, TurnSummary, Usage, UsageRecord};
use agmag::report::{self, GroupBy};
use agmag::store::Store;
use std::path::Path;

fn fixtures() -> anyhow::Result<Store> {
    let store = Store::open_in_memory()?;
    for (source, dir) in [(Source::Codex, "codex"), (Source::Claude, "claude")] {
        ingest_file(&store, source, &Path::new("tests").join("fixtures").join(dir).join("mini.jsonl"))?;
    }
    Ok(store)
}

#[test]
fn sessions_rows() -> anyhow::Result<()> {
    let store = fixtures()?;
    let rows = report::sessions(&store, &Config::builtin(), 0)?;
    assert_eq!(rows.len(), 2);
    assert!(rows[0].last_ms >= rows[1].last_ms);
    let codex = rows.iter().find(|r| r.source == Source::Codex).unwrap();
    assert_eq!(codex.context_pct, Some(24.4));
    assert_eq!(codex.unknown_lines, 1);
    assert_eq!(codex.warning, None);
    let claude = rows.iter().find(|r| r.source == Source::Claude).unwrap();
    assert_eq!(claude.context_pct, None);
    assert_eq!(claude.cwd.as_deref(), Some(r"D:\Dự án\x"));
    assert_eq!(claude.usage.output, 1208);
    assert!(report::sessions(&store, &Config::builtin(), i64::MAX)?.is_empty());
    Ok(())
}

#[test]
fn cost_by_model_subscription_has_no_usd() -> anyhow::Result<()> {
    let store = fixtures()?;
    let rows = report::cost(&store, &Config::builtin(), GroupBy::Model, 0)?;
    let row = rows.iter().find(|r| r.key == "gpt-6.1-sol").unwrap();
    assert_eq!(row.billing, Some(Billing::Subscription));
    assert_eq!(row.cost_usd, None);
    assert_eq!(row.usage.cache_read, 13312);
    assert_eq!(serde_json::to_value(row)?["billing"], "subscription");
    Ok(())
}

#[test]
fn quota_latest() -> anyhow::Result<()> {
    let store = fixtures()?;
    store.insert_quota(&QuotaSnapshot {
        provider: "codex".into(), ts_ms: 1,
        windows: vec![QuotaWindow { window_minutes: 300, used_percent: 1.0, resets_at_s: i64::MAX }],
    })?;
    let rows = report::quota(&store, 1795000000)?;
    assert_eq!(rows.len(), 2);
    let short = rows.iter().find(|r| r.window_minutes == 300).unwrap();
    assert_eq!(short.provider, "codex");
    assert_eq!(short.used_percent, 22.0);
    assert!(short.reset_passed);
    assert!(!rows.iter().find(|r| r.window_minutes == 10080).unwrap().reset_passed);
    Ok(())
}

#[test]
fn turns_ambiguous_prefix() -> anyhow::Result<()> {
    let store = fixtures()?;
    let error = report::turns(&store, "", 50).err().unwrap().to_string();
    assert!(error.contains("session_1") && error.contains("s-1"), "{error}");
    assert!(report::turns(&store, "missing", 50).is_err());
    let turns = report::turns(&store, "s-", 1)?;
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].summary, "chào bạn lần nữa");
    assert_eq!(turns[0].role, Role::Assistant);
    assert!(report::turns(&store, "session_", 0)?.is_empty());
    Ok(())
}

fn usage(output: u64) -> Usage {
    Usage { input_uncached: 0, cache_read: 0, cache_write: 0, output, reasoning: 0 }
}

#[test]
fn warnings_and_context_fallback() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "a")?;
    store.set_model(sid, "custom")?;
    store.insert_turn(sid, &TurnSummary { ts_ms: 1, role: Role::User, summary: "hello".into(), byte_offset: 0 })?;
    store.set_context(sid, &ContextSnapshot { ts_ms: 1, used_tokens: 25, window_tokens: None })?;
    let mut cfg = Config::builtin();
    cfg.models.insert("custom".into(), ModelCfg { provider: "codex".into(), context_window: Some(100), price: None });
    let rows = report::sessions(&store, &cfg, 0)?;
    assert_eq!(rows[0].warning.as_deref(), Some("định dạng lạ?"));
    assert_eq!(rows[0].context_pct, Some(25.0));
    store.add_line_stats(sid, 0, 2)?;
    assert_eq!(report::sessions(&store, &cfg, 0)?[0].warning.as_deref(), Some("không đọc được 2 dòng"));
    store.set_context(sid, &ContextSnapshot { ts_ms: 2, used_tokens: 25, window_tokens: Some(200) })?;
    assert_eq!(report::sessions(&store, &cfg, 0)?[0].context_pct, Some(12.5));
    Ok(())
}

#[test]
fn cost_groups_utc_and_filters_usage_not_session_time() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "a")?;
    for (key, ts_ms, model, output) in [
        ("a", 0, "gpt-6.1-sol", 3),
        ("b", 86_400_000, "mystery", 7),
        ("c", 86_400_001, "gpt-6.1-sol", 11),
    ] {
        store.upsert_usage(sid, &UsageRecord { dedupe_key: key.into(), ts_ms, model: Some(model.into()), usage: usage(output) })?;
    }
    let cfg = Config::builtin();
    let days = report::cost(&store, &cfg, GroupBy::Day, 0)?;
    assert_eq!(days.iter().map(|r| (r.key.as_str(), r.usage.output)).collect::<Vec<_>>(), [("1970-01-01", 3), ("1970-01-02", 18)]);
    assert_eq!(days[1].billing, None);
    assert_eq!(days[1].cost_usd, None);
    let models = report::cost(&store, &cfg, GroupBy::Model, 86_400_000)?;
    assert_eq!(models.iter().find(|r| r.key == "gpt-6.1-sol").unwrap().usage.output, 11);
    assert_eq!(models.iter().find(|r| r.key == "mystery").unwrap().cost_usd, None);
    assert_eq!(report::cost(&store, &cfg, GroupBy::Source, 0)?[0].key, "codex");
    assert_eq!(report::cost(&store, &cfg, GroupBy::Cwd, 0)?[0].key, "?");
    Ok(())
}

#[test]
fn cost_never_calls_human_pricing_for_subscription_or_missing_price() -> anyhow::Result<()> {
    let store = fixtures()?;
    let mut cfg = Config::builtin();
    cfg.models.get_mut("gpt-6.1-sol").unwrap().price = Some(Price { input: 1.0, cache_read: 1.0, cache_write: 1.0, output: 1.0 });
    assert!(report::cost(&store, &cfg, GroupBy::Model, 0)?.iter().all(|r| r.cost_usd.is_none()));
    cfg.providers.insert("codex".into(), ProviderCfg { billing: Billing::Api });
    cfg.models.get_mut("gpt-6.1-sol").unwrap().price = None;
    assert!(report::cost(&store, &cfg, GroupBy::Model, 0)?.iter().all(|r| r.cost_usd.is_none()));
    Ok(())
}
