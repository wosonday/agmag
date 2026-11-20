use agmag::config::{Billing, Config, ModelCfg, Price, ProviderCfg};
use agmag::ingest::ingest_file;
use agmag::model::{
    ContextSnapshot, QuotaSnapshot, QuotaWindow, Role, Source, TurnSummary, Usage, UsageRecord,
};
use agmag::report::{self, GroupBy};
use agmag::store::Store;
use std::path::Path;

fn fixtures() -> anyhow::Result<Store> {
    let store = Store::open_in_memory()?;
    for (source, dir) in [(Source::Codex, "codex"), (Source::Claude, "claude")] {
        ingest_file(
            &store,
            source,
            &Path::new("tests")
                .join("fixtures")
                .join(dir)
                .join("mini.jsonl"),
        )?;
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
        provider: "codex".into(),
        ts_ms: 1,
        windows: vec![QuotaWindow {
            window_minutes: 300,
            used_percent: 1.0,
            resets_at_s: i64::MAX,
        }],
    })?;
    let rows = report::quota(&store, 1795000000)?;
    assert_eq!(rows.len(), 2);
    let short = rows.iter().find(|r| r.window_minutes == 300).unwrap();
    assert_eq!(short.provider, "codex");
    assert_eq!(short.used_percent, 22.0);
    assert!(short.reset_passed);
    assert!(
        !rows
            .iter()
            .find(|r| r.window_minutes == 10080)
            .unwrap()
            .reset_passed
    );
    Ok(())
}

#[test]
fn quota_left_percent_counts_down_and_is_full_after_reset() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    store.insert_quota(&QuotaSnapshot {
        provider: "codex".into(),
        ts_ms: 1,
        windows: vec![
            // 5h window already reset: the stored 2% is stale, so the plan is full again.
            QuotaWindow {
                window_minutes: 300,
                used_percent: 2.0,
                resets_at_s: 100,
            },
            QuotaWindow {
                window_minutes: 10080,
                used_percent: 16.0,
                resets_at_s: 5_000,
            },
        ],
    })?;
    let rows = report::quota(&store, 1_000)?;
    let short = rows.iter().find(|r| r.window_minutes == 300).unwrap();
    let week = rows.iter().find(|r| r.window_minutes == 10080).unwrap();
    assert_eq!(short.used_percent, 2.0, "stored value is kept as logged");
    assert_eq!(short.left_percent, 100.0);
    assert_eq!(week.left_percent, 84.0);
    Ok(())
}

#[test]
fn turns_ambiguous_prefix() -> anyhow::Result<()> {
    let store = fixtures()?;
    let error = report::turns(&store, "", 50).err().unwrap().to_string();
    assert!(
        error.contains("session_1") && error.contains("s-1"),
        "{error}"
    );
    assert!(report::turns(&store, "missing", 50).is_err());
    let turns = report::turns(&store, "s-", 1)?;
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].summary, "chào bạn lần nữa");
    assert_eq!(turns[0].role, Role::Assistant);
    assert!(report::turns(&store, "session_", 0)?.is_empty());
    Ok(())
}

fn usage(output: u64) -> Usage {
    Usage {
        input_uncached: 0,
        cache_read: 0,
        cache_write: 0,
        output,
        reasoning: 0,
    }
}

#[test]
fn warnings_and_context_fallback() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "a")?;
    store.set_model(sid, "custom")?;
    store.insert_turn(
        sid,
        &TurnSummary {
            ts_ms: 1,
            role: Role::User,
            summary: "hello".into(),
            byte_offset: 0,
        },
    )?;
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 25,
            window_tokens: None,
        },
    )?;
    let mut cfg = Config::builtin();
    cfg.models.insert(
        "custom".into(),
        ModelCfg {
            provider: "codex".into(),
            context_window: Some(100),
            price: None,
        },
    );
    let rows = report::sessions(&store, &cfg, 0)?;
    assert_eq!(rows[0].warning.as_deref(), Some("unknown format?"));
    assert_eq!(rows[0].context_pct, Some(25.0));
    store.add_line_stats(sid, 0, 2)?;
    assert_eq!(
        report::sessions(&store, &cfg, 0)?[0].warning.as_deref(),
        Some("2 unreadable lines")
    );
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 2,
            used_tokens: 25,
            window_tokens: Some(200),
        },
    )?;
    assert_eq!(
        report::sessions(&store, &cfg, 0)?[0].context_pct,
        Some(12.5)
    );
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
        store.upsert_usage(
            sid,
            &UsageRecord {
                dedupe_key: key.into(),
                ts_ms,
                model: Some(model.into()),
                usage: usage(output),
            },
        )?;
    }
    let cfg = Config::builtin();
    let days = report::cost_with_offset(&store, &cfg, GroupBy::Day, 0, &mut |_| 0)?;
    assert_eq!(
        days.iter()
            .map(|r| (r.key.as_str(), r.usage.output))
            .collect::<Vec<_>>(),
        [("1970-01-01", 3), ("1970-01-02", 18)]
    );
    assert_eq!(days[1].billing, None);
    assert_eq!(days[1].cost_usd, None);
    let models = report::cost(&store, &cfg, GroupBy::Model, 86_400_000)?;
    assert_eq!(
        models
            .iter()
            .find(|r| r.key == "gpt-6.1-sol")
            .unwrap()
            .usage
            .output,
        11
    );
    assert_eq!(
        models.iter().find(|r| r.key == "mystery").unwrap().cost_usd,
        None
    );
    assert_eq!(
        report::cost(&store, &cfg, GroupBy::Source, 0)?[0].key,
        "codex"
    );
    assert_eq!(report::cost(&store, &cfg, GroupBy::Cwd, 0)?[0].key, "?");
    Ok(())
}

#[test]
fn local_bounds_handles_utc_plus7_year_crossing_and_dst() {
    // 2027-01-15 00:30:00 +07:00 = 2027-01-14 17:30:00Z
    let jan15_0030_vn = (report::days_from_civil(2027, 1, 14) * 86_400 + 17 * 3600 + 1800) * 1000;
    let b = report::compute_local_bounds(jan15_0030_vn, 4, &mut |_| 7 * 3600);
    assert_eq!(b.offset_label, "UTC+07:00");
    assert_eq!(
        b.recent_month_keys,
        ["2026-10", "2026-11", "2026-12", "2027-01"]
    );
    // Đầu ngày 2027-01-15 ở UTC+7 là 2027-01-14 17:00:00Z
    assert_eq!(
        b.today_start_ms,
        (report::days_from_civil(2027, 1, 14) * 86_400 + 17 * 3600) * 1000
    );
    // Đầu tháng 2027-01 ở UTC+7 là 2026-12-31 17:00:00Z
    assert_eq!(
        b.month_start_ms,
        (report::days_from_civil(2026, 12, 31) * 86_400 + 17 * 3600) * 1000
    );

    // Giả lập DST: trước 2026-11-01 06:00Z là UTC-4 (-14400), từ đó trở đi là UTC-5 (-18000).
    let dst_switch = report::days_from_civil(2026, 11, 1) * 86_400 + 6 * 3600;
    let now_dst = (dst_switch + 3600) * 1000; // 07:00Z = 02:00 EST
    let mut dst_off = |ts_s: i64| {
        if ts_s < dst_switch {
            -4 * 3600
        } else {
            -5 * 3600
        }
    };
    let bd = report::compute_local_bounds(now_dst, 4, &mut dst_off);
    assert_eq!(bd.offset_label, "UTC-05:00");
    // Nửa đêm 2026-11-01 ở EDT (-4h) là 04:00Z (chưa đổi DST lúc 02:00!)
    assert_eq!(
        bd.today_start_ms,
        (report::days_from_civil(2026, 11, 1) * 86_400 + 4 * 3600) * 1000
    );
}

#[test]
fn summary_4_months_groups_local_days_and_checks_coverage() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://s")?;
    let mut cfg = Config::builtin();
    cfg.providers.insert(
        "openrouter".into(),
        ProviderCfg {
            billing: Billing::Api,
        },
    );
    cfg.models.insert(
        "api-m".into(),
        ModelCfg {
            provider: "openrouter".into(),
            context_window: None,
            price: Some(Price {
                input: 1.0,
                cache_read: 0.5,
                cache_write: 0.0,
                output: 2.0,
            }),
        },
    );
    // 2026-11-18 18:00Z = 2026-11-19 01:00+07:00 (hôm nay ở UTC+7 nhưng hôm qua ở UTC!)
    let ts_vn_today = (report::days_from_civil(2026, 11, 18) * 86_400 + 18 * 3600) * 1000;
    // 2026-09-10 12:00Z
    let ts_sep = (report::days_from_civil(2026, 9, 10) * 86_400 + 12 * 3600) * 1000;
    store.upsert_usage(
        sid,
        &UsageRecord {
            dedupe_key: "1".into(),
            ts_ms: ts_vn_today,
            model: Some("api-m".into()),
            usage: Usage {
                input_uncached: 1_000_000,
                output: 500_000,
                ..Default::default()
            },
        },
    )?;
    store.upsert_usage(
        sid,
        &UsageRecord {
            dedupe_key: "2".into(),
            ts_ms: ts_sep,
            model: Some("api-m".into()),
            usage: usage(200_000),
        },
    )?;

    let now_ms = ts_vn_today + 3_600_000; // 2026-11-19 02:00+07:00
    let mut vn = |_: i64| 7 * 3600;
    let sum = report::usage_summary_with_offset(&store, &cfg, now_ms, &mut vn)?;
    assert_eq!(sum.offset_label, "UTC+07:00");
    assert_eq!(sum.last_24h.usage.total(), 1_500_000);
    assert_eq!(sum.today.usage.total(), 1_500_000);
    assert_eq!(sum.this_month.usage.total(), 1_500_000);
    assert_eq!(sum.this_month.api_usd, Some(2.0));
    assert_eq!(
        sum.recent_months
            .iter()
            .map(|r| (r.key.as_str(), r.usage.total()))
            .collect::<Vec<_>>(),
        [
            ("2026-08", 0),
            ("2026-09", 200_000),
            ("2026-10", 0),
            ("2026-11", 1_500_000)
        ]
    );
    assert!(sum.incomplete_coverage);

    for src in ["codex", "claude", "agy"] {
        store.update_covered_since(src, sum.four_months_start_ms)?;
    }
    let sum2 = report::usage_summary_with_offset(&store, &cfg, now_ms, &mut vn)?;
    assert!(!sum2.incomplete_coverage);

    let by_m = report::cost_with_offset(
        &store,
        &cfg,
        GroupBy::Month,
        sum.four_months_start_ms,
        &mut vn,
    )?;
    assert_eq!(
        by_m.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["2026-09", "2026-11"]
    );
    assert_eq!(report::render_bar(50, 100, 10, false), "█████░░░░░");
    assert_eq!(report::render_bar(50, 100, 10, true), "#####.....");
    Ok(())
}

#[test]
fn summary_api_usd_is_unknown_when_an_api_model_has_no_price() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://s")?;
    let mut cfg = Config::builtin();
    cfg.providers.insert(
        "openrouter".into(),
        ProviderCfg {
            billing: Billing::Api,
        },
    );
    let model = |price| ModelCfg {
        provider: "openrouter".into(),
        context_window: None,
        price,
    };
    cfg.models.insert(
        "api-priced".into(),
        model(Some(Price {
            input: 1.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.0,
        })),
    );
    cfg.models.insert("api-unpriced".into(), model(None));
    let now_ms = (report::days_from_civil(2026, 11, 18) * 86_400 + 12 * 3600) * 1000;
    let mut utc = |_: i64| 0;
    let rec = |key: &str, m: &str, u: Usage| UsageRecord {
        dedupe_key: key.into(),
        ts_ms: now_ms - 60_000,
        model: Some(m.into()),
        usage: u,
    };

    store.upsert_usage(
        sid,
        &rec(
            "a",
            "api-priced",
            Usage {
                input_uncached: 1_000_000,
                ..Default::default()
            },
        ),
    )?;
    let sum = report::usage_summary_with_offset(&store, &cfg, now_ms, &mut utc)?;
    assert_eq!(sum.today.api_usd, Some(1.0));

    // Một model trả theo API nhưng chưa khai giá: tổng USD không còn biết được — không hiện $1.00 như thể đủ.
    store.upsert_usage(sid, &rec("b", "api-unpriced", usage(10)))?;
    let sum = report::usage_summary_with_offset(&store, &cfg, now_ms, &mut utc)?;
    assert_eq!(
        (
            sum.last_24h.api_usd,
            sum.today.api_usd,
            sum.this_month.api_usd
        ),
        (None, None, None)
    );
    Ok(())
}

#[test]
fn subscription_has_list_value_but_no_billed_cost() -> anyhow::Result<()> {
    let store = fixtures()?;
    let mut cfg = Config::builtin();
    cfg.models.get_mut("gpt-6.1-sol").unwrap().price = Some(Price {
        input: 1.0,
        cache_read: 1.0,
        cache_write: 1.0,
        output: 1.0,
    });
    let rows = report::cost(&store, &cfg, GroupBy::Model, 0)?;
    // Subscription: never "billed", but its worth at list price is shown.
    assert!(rows.iter().all(|r| r.cost_usd.is_none()));
    let sol = rows
        .iter()
        .find(|r| r.key == "gpt-6.1-sol")
        .expect("gpt row");
    assert!(sol.value_usd.unwrap() > 0.0 && sol.unpriced_tokens == 0);
    // API-billed without a user price falls back to the built-in list price, and then it is billed.
    cfg.providers.insert(
        "codex".into(),
        ProviderCfg {
            billing: Billing::Api,
        },
    );
    cfg.models.get_mut("gpt-6.1-sol").unwrap().price = None;
    let rows = report::cost(&store, &cfg, GroupBy::Model, 0)?;
    let sol = rows
        .iter()
        .find(|r| r.key == "gpt-6.1-sol")
        .expect("gpt row");
    assert!(sol.cost_usd.is_some() && sol.cost_usd == sol.value_usd);
    Ok(())
}

#[test]
fn price_lookup_strips_provider_date_and_effort_and_frees_zen() {
    use agmag::config::model_id_candidates;
    let cfg = Config::builtin();
    let opus = cfg
        .price_for("claude-opus-5-5")
        .expect("built-in Opus 5.5 price");
    assert_eq!(
        (opus.input, opus.output, opus.cache_read, opus.cache_write),
        (4.0, 20.0, 0.2, 5.0)
    );
    for name in [
        "anthropic/claude-opus-5-5",
        "claude-opus-5-5-high",
        "openrouter/claude-opus-5-5",
    ] {
        assert_eq!(cfg.price_for(name).as_ref(), Some(&opus), "{name}");
    }
    assert!(
        cfg.price_for("claude-haiku-4-5-20251001").is_some(),
        "dated Claude id"
    );
    assert!(
        cfg.price_for("gemini-4-argon-medium").is_some(),
        "agy effort suffix"
    );
    assert!(cfg.price_for("opencode-go/deepseek-v4-pro").is_some());
    assert_eq!(cfg.price_for("opencode/beam-free"), Some(Price::FREE));
    assert_eq!(
        cfg.price_for("openrouter/google/gemma-4-31b-it:free"),
        Some(Price::FREE)
    );
    assert_eq!(
        cfg.price_for("codex-auto-review"),
        None,
        "no known price → unknown, not guessed"
    );
    assert_eq!(cfg.price_for("nvidia-kimi/moonshotai/kimi-k3.1-pro"), None);
    assert_eq!(
        model_id_candidates("anthropic/claude-haiku-4-5-20251001")[..3],
        [
            "anthropic/claude-haiku-4-5-20251001",
            "anthropic/claude-haiku-4-5",
            "claude-haiku-4-5-20251001"
        ]
    );
}

#[test]
fn user_price_overrides_builtin() -> anyhow::Result<()> {
    let mut cfg = Config::builtin();
    cfg.models.insert(
        "claude-opus-5-5".into(),
        agmag::config::ModelCfg {
            provider: "claude".into(),
            context_window: None,
            price: Some(Price {
                input: 9.0,
                cache_read: 9.0,
                cache_write: 9.0,
                output: 9.0,
            }),
        },
    );
    assert_eq!(
        cfg.price_for("claude-opus-5-5-high").map(|p| p.input),
        Some(9.0)
    );
    Ok(())
}

#[test]
fn cost_totals_and_value_labels() {
    use agmag::report::{CostRow, cost_totals, value_label};
    let row = |key: &str, out: u64, value: Option<f64>, unpriced: u64| CostRow {
        key: key.into(),
        usage: Usage {
            output: out,
            ..Default::default()
        },
        billing: None,
        cost_usd: None,
        value_usd: value,
        unpriced_tokens: unpriced,
    };
    let rows = [
        row("a", 100, Some(1.5), 0),
        row("b", 50, None, 50),
        row("c", 10, Some(0.25), 0),
    ];
    let sum = cost_totals(&rows);
    assert_eq!(
        (
            sum.key.as_str(),
            sum.usage.output,
            sum.value_usd,
            sum.unpriced_tokens
        ),
        ("SUM", 160, Some(1.75), 50)
    );
    assert_eq!(value_label(sum.value_usd, sum.unpriced_tokens), "≥ $1.75");
    assert_eq!(value_label(Some(1.75), 0), "$1.75");
    assert_eq!(value_label(None, 50), "?");
    assert_eq!(cost_totals(&[]).value_usd, None);
}
