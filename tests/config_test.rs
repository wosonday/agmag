use agmag::config::{Billing, Config, Price, context_pct, cost_usd};
use agmag::model::{Source, Usage};

#[test]
fn default_has_subscriptions() {
    let cfg = Config::builtin();
    assert_eq!(
        cfg.billing_for_model("gpt-6.1-sol"),
        Some(Billing::Subscription)
    );
    assert_eq!(
        cfg.billing_for_model("claude-opus-5-5"),
        Some(Billing::Subscription)
    );
    assert_eq!(cfg.providers["agy"].billing, Billing::Subscription);
    assert_eq!(cfg.providers["openrouter"].billing, Billing::Api);
    assert_eq!(cfg.providers["ollama"].billing, Billing::Api);
    for model in cfg.models.values() {
        assert!(model.price.is_none());
    }
    // Chỉ gpt-6.1-sol có cửa sổ mặc định (giá trị Codex tự báo); Claude chờ hook statusline.
    assert_eq!(cfg.model("claude-opus-5-5").unwrap().context_window, None);
}

#[test]
fn alias_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.toml");
    std::fs::write(
        &path,
        r#"
[providers.claude]
billing = "subscription"
[models."claude-opus-5-5"]
provider = "claude"
[aliases]
opus = "claude-opus-5-5"
"#,
    )
    .unwrap();
    let cfg = Config::load(&path).unwrap();
    assert_eq!(cfg.model("opus").unwrap().provider, "claude");
    assert!(std::ptr::eq(
        cfg.model("opus").unwrap(),
        cfg.model("claude-opus-5-5").unwrap()
    ));
    assert_eq!(cfg.billing_for_model("opus"), Some(Billing::Subscription));
}

#[test]
fn unknown_model_has_no_price() {
    let cfg = Config::builtin();
    assert!(cfg.model("nope").is_none());
    assert_eq!(cfg.billing_for_model("nope"), None);
}

#[test]
fn bad_toml_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.toml");
    for text in ["billing = 3", "[providers.codex]\nbilling = 3", "[broken"] {
        std::fs::write(&path, text).unwrap();
        assert!(Config::load(&path).is_err(), "accepted {text:?}");
    }
}

#[test]
fn missing_file_uses_builtin() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::load(&dir.path().join("missing.toml")).unwrap();
    assert_eq!(
        cfg.billing_for_model("gpt-6.1-sol"),
        Some(Billing::Subscription)
    );
}

#[test]
fn read_error_is_not_hidden() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Config::load(dir.path()).is_err());
}

#[test]
fn user_config_loads_context_and_prices() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.toml");
    std::fs::write(
        &path,
        r#"
[providers.openrouter]
billing = "api"
[models.custom]
provider = "openrouter"
context_window = 100000
[models.custom.price]
input = 3.0
cache_read = 0.3
cache_write = 3.75
output = 15.0
"#,
    )
    .unwrap();
    let cfg = Config::load(&path).unwrap();
    let model = cfg.model("custom").unwrap();
    assert_eq!(model.context_window, Some(100000));
    let price = model.price.as_ref().unwrap();
    assert_eq!(
        (
            price.input,
            price.cache_read,
            price.cache_write,
            price.output
        ),
        (3.0, 0.3, 3.75, 15.0)
    );
    assert_eq!(cfg.billing_for_model("custom"), Some(Billing::Api));
    assert!(cfg.model("gpt-6.1-sol").is_none());
}

#[test]
fn alias_takes_precedence() {
    let mut cfg = Config::builtin();
    cfg.aliases
        .insert("gpt-6.1-sol".into(), "claude-opus-5-5".into());
    assert_eq!(cfg.model("gpt-6.1-sol").unwrap().provider, "claude");
}

#[test]
fn source_provider_names() {
    assert_eq!(Config::provider_for_source(Source::Codex), "codex");
    assert_eq!(Config::provider_for_source(Source::Claude), "claude");
}

#[test]
fn context_pct_cases() {
    assert_eq!(context_pct(116029, Some(475000)), Some(24.4));
    assert_eq!(context_pct(5, None), None);
    assert_eq!(context_pct(5, Some(0)), None);
    assert_eq!(context_pct(0, Some(100)), Some(0.0));
    assert_eq!(context_pct(125, Some(100)), Some(125.0));
}

// Chờ người dùng: Learn by Doing — triển khai cost_usd; test này được phép FAIL.
#[test]
fn cost_basic() {
    let input_only = Usage {
        input_uncached: 1_000_000,
        cache_read: 0,
        cache_write: 0,
        output: 0,
        reasoning: 0,
    };
    let input_price = Price {
        input: 3.0,
        cache_read: 0.0,
        cache_write: 0.0,
        output: 0.0,
    };
    assert_eq!(cost_usd(&input_only, &input_price), 3.0);

    let cached_output = Usage {
        input_uncached: 0,
        cache_read: 1_000_000,
        cache_write: 0,
        output: 1_000_000,
        reasoning: 500_000,
    };
    let price = Price {
        input: 3.0,
        cache_read: 0.3,
        cache_write: 3.75,
        output: 15.0,
    };
    assert_eq!(cost_usd(&cached_output, &price), 15.3);
}

#[test]
fn policy_defaults() {
    let p = Config::builtin().policy;
    assert_eq!(
        (
            p.max_workers,
            p.ctx_warn,
            p.ctx_rotate,
            p.handoff_timeout_min
        ),
        (3, 50.0, 70.0, 10)
    );
    assert_eq!(p.manager, None);
}

/// `[policy].quota_stale_min` (ngưỡng dữ liệu hạn mức cũ) và `[policy].codex_quota_poll_min`
/// phải đọc được từ config và có giá trị mặc định khi thiếu.
#[test]
fn quota_stale_policy_parses() {
    let cfg: Config = toml::from_str(
        r#"
[policy]
quota_stale_min = 90
codex_quota_poll_min = 5
"#,
    )
    .unwrap();
    assert_eq!(cfg.policy.quota_stale_min, 90);
    assert_eq!(cfg.policy.codex_quota_poll_min, 5);

    let d = Config::builtin().policy;
    assert_eq!(d.quota_stale_min, 60, "default stale threshold (60 phút)");
    assert_eq!(d.codex_quota_poll_min, 15, "default codex poll (15 phút)");
}

#[test]
fn gpt_sol_has_window() {
    assert_eq!(
        Config::builtin()
            .model("gpt-6.1-sol")
            .unwrap()
            .context_window,
        Some(475000)
    );
}

/// OpenCode keys are `providerID/id`: the bare id's entry is reused; an exact `provider/id` entry wins.
#[test]
fn opencode_provider_keys_resolve_models() {
    let cfg: Config = toml::from_str(
        r#"
[providers.codex]
billing = "subscription"
[providers.openrouter]
billing = "api"
[models."gpt-6.1-sol"]
provider = "codex"
context_window = 475000
[models."openrouter/gpt-6.1-sol"]
provider = "openrouter"
context_window = 200000
"#,
    )
    .unwrap();
    assert_eq!(
        cfg.model("openai/gpt-6.1-sol").unwrap().context_window,
        Some(475000)
    );
    assert_eq!(
        cfg.model("openrouter/gpt-6.1-sol").unwrap().context_window,
        Some(200000)
    );
    assert!(cfg.model("opencode-go/deepseek-v4-pro").is_none());
    assert_eq!(
        agmag::config::split_provider("nvidia-kimi/moonshotai/kimi-k3.1-pro"),
        Some(("nvidia-kimi", "moonshotai/kimi-k3.1-pro"))
    );
    assert_eq!(agmag::config::split_provider("gpt-6.1-sol"), None);
    assert_eq!(agmag::config::split_provider("/x"), None);
}

/// Future OpenCode models need no [models] entry: the provider prefix alone decides billing.
#[test]
fn undeclared_opencode_model_billed_by_provider_prefix() {
    let cfg = Config::builtin();
    // Declared model wins over the prefix (ChatGPT login through OpenCode = Codex subscription).
    assert_eq!(
        cfg.billing_for_model("openai/gpt-6.1-sol"),
        Some(Billing::Subscription)
    );
    assert_eq!(
        cfg.billing_for_model("opencode-go/deepseek-v4-pro"),
        Some(Billing::Subscription)
    );
    assert_eq!(
        cfg.billing_for_model("opencode-go/some-model-from-2027"),
        Some(Billing::Subscription)
    );
    assert_eq!(
        cfg.billing_for_model("openrouter/qwen/qwen4-max"),
        Some(Billing::Api)
    );
    // Unknown provider (e.g. NVIDIA, fees unverified) stays unknown rather than guessed.
    assert_eq!(
        cfg.billing_for_model("nvidia-kimi/moonshotai/kimi-k3.1-pro"),
        None
    );
    assert_eq!(cfg.billing_for_model("nope"), None);
}

/// Claude Code/Codex có ngưỡng riêng (mặc định 40/50); thợ OpenCode giữ `ctx_warn`/`ctx_rotate` chung.
#[test]
fn interactive_ctx_thresholds_default_and_parse() {
    let p = Config::builtin().policy;
    assert_eq!(
        (p.interactive_ctx_warn, p.interactive_ctx_rotate),
        (40.0, 50.0)
    );
    assert_eq!(p.ctx_thresholds(Some("claude-code")), (40.0, 50.0));
    assert_eq!(p.ctx_thresholds(Some("codex")), (40.0, 50.0));
    assert_eq!(p.ctx_thresholds(Some("opencode")), (50.0, 70.0));
    assert_eq!(p.ctx_thresholds(None), (50.0, 70.0));

    let cfg: Config = toml::from_str(
        "[policy]\nctx_warn = 60\nctx_rotate = 80\ninteractive_ctx_warn = 35\ninteractive_ctx_rotate = 45\n",
    )
    .unwrap();
    assert_eq!(cfg.policy.ctx_thresholds(Some("codex")), (35.0, 45.0));
    assert_eq!(cfg.policy.ctx_thresholds(Some("opencode")), (60.0, 80.0));
}

/// providers.toml cũ chỉ có `ctx_warn`/`ctx_rotate` vẫn nạp được và giữ mặc định 40/50 cho Claude/Codex.
#[test]
fn old_policy_without_interactive_keys_keeps_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.toml");
    std::fs::write(
        &path,
        "[policy]\nmax_workers = 2\nctx_warn = 50\nctx_rotate = 70\n",
    )
    .unwrap();
    let p = Config::load(&path).unwrap().policy;
    assert_eq!(p.ctx_thresholds(Some("claude-code")), (40.0, 50.0));
    assert_eq!(p.ctx_thresholds(Some("opencode")), (50.0, 70.0));
}

#[test]
fn delegated_actions_reject_empty_and_hard_prohibited_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.toml");
    for (entry, needle) in [
        ("", "empty"),
        ("  ", "empty"),
        ("push", "push"),
        (" merge ", "merge"),
        ("PUSH", "push"),
        ("Push", "push"),
        (" push\\n", "push"),
        ("grant_extension", "grant_extension"),
    ] {
        std::fs::write(
            &path,
            format!("[policy]\ndelegated_actions = [\"{entry}\"]\n"),
        )
        .unwrap();
        let err = Config::load(&path).expect_err(entry);
        let msg = format!("{err:#}");
        assert!(msg.contains("Invalid [policy]"), "{entry:?}: {msg}");
        assert!(msg.contains(needle), "{entry:?}: {msg}");
    }
    std::fs::write(
        &path,
        "[policy]\ndelegated_actions = [\"continue_review\", \"spawn_worker\"]\n",
    )
    .unwrap();
    let cfg = Config::load(&path).unwrap();
    assert_eq!(cfg.policy.delegated_actions.len(), 2);
}

/// Ngưỡng phải hữu hạn, trong (0, 100] và warn ≤ rotate; nếu không daemon sẽ tạo rồi hủy vòng xoay mỗi tick.
#[test]
fn invalid_ctx_thresholds_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("providers.toml");
    for policy in [
        "interactive_ctx_rotate = 35",
        "ctx_warn = 80",
        "interactive_ctx_warn = -1",
        "interactive_ctx_warn = 0",
        "ctx_rotate = 101",
        "interactive_ctx_rotate = nan",
        "ctx_warn = inf",
    ] {
        std::fs::write(&path, format!("[policy]\n{policy}\n")).unwrap();
        let err = Config::load(&path).expect_err(policy);
        let msg = format!("{err:#}");
        assert!(msg.contains("Invalid [policy]"), "{policy}: {msg}");
        assert!(
            msg.contains("must be a number in (0, 100]") || msg.contains("must not exceed"),
            "{policy}: {msg}"
        );
    }
    for policy in [
        "interactive_ctx_warn = 30\ninteractive_ctx_rotate = 35",
        "interactive_ctx_warn = 50\ninteractive_ctx_rotate = 50",
        "ctx_warn = 100\nctx_rotate = 100",
    ] {
        std::fs::write(&path, format!("[policy]\n{policy}\n")).unwrap();
        assert!(Config::load(&path).is_ok(), "{policy}");
    }
}
