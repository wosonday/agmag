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
        assert!(model.context_window.is_none());
    }
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
#[ignore = "chờ người dùng viết cost_usd"]
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
