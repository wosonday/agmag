use agmag::config::Config;
use agmag::model::ContextSnapshot;
use agmag::model::Source;
use agmag::opencode_models::ModelCatalog;
use agmag::store::Store;

const CATALOG: &str = r#"{"location":"redacted","data":[
  {"providerID":"opencode-go","id":"fixture-kimi","limit":{"context":1000000}},
  {"providerID":"other","id":"fixture-kimi","limit":{"context":200000}},
  {"providerID":"nested","id":"vendor/model","limit":{"context":32000}},
  {"providerID":"opencode-go","id":"zero","limit":{"context":0}},
  {"providerID":"opencode-go","id":"missing","limit":{}}
]}"#;

#[test]
fn catalog_context_respects_config_and_provider_identity() -> anyhow::Result<()> {
    let mut cfg: Config = toml::from_str(
        r#"
        [models."opencode-go/fixture-kimi"]
        provider = "opencode-go"
        context_window = 400000
        [models.fixture-kimi]
        provider = "opencode-go"
        context_window = 500000
        "#,
    )?;
    cfg.opencode_models = Some(ModelCatalog::from_json(CATALOG)?);
    assert_eq!(
        cfg.context_window(&Source::OpenCode, "opencode-go/fixture-kimi"),
        Some(400000)
    );
    cfg.models.remove("opencode-go/fixture-kimi");
    assert_eq!(
        cfg.context_window(&Source::OpenCode, "opencode-go/fixture-kimi"),
        Some(500000)
    );
    cfg.models.clear();
    assert_eq!(
        cfg.context_window(&Source::OpenCode, "opencode-go/fixture-kimi"),
        Some(1000000)
    );
    assert_eq!(
        cfg.context_window(&Source::OpenCode, "other/fixture-kimi"),
        Some(200000)
    );
    assert_eq!(
        cfg.context_window(&Source::OpenCode, "nested/vendor/model"),
        Some(32000)
    );
    for name in [
        "absent/fixture-kimi",
        "fixture-kimi",
        "opencode-go/zero",
        "opencode-go/missing",
    ] {
        assert_eq!(cfg.context_window(&Source::OpenCode, name), None, "{name}");
    }
    assert_eq!(
        cfg.context_window(&Source::Claude, "opencode-go/fixture-kimi"),
        None
    );
    Ok(())
}

#[test]
fn sessions_use_catalog_context_without_guessing_missing_models() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut cfg = Config::builtin();
    cfg.opencode_models = Some(ModelCatalog::from_json(CATALOG)?);
    for (path, model) in [
        ("known", "opencode-go/fixture-kimi"),
        ("unknown", "no/model"),
    ] {
        let sid = store.session_id_for_path(Source::OpenCode, path)?;
        store.set_model(sid, model)?;
        store.set_context(
            sid,
            &ContextSnapshot {
                ts_ms: 1,
                used_tokens: 250000,
                window_tokens: None,
            },
        )?;
    }
    let rows = agmag::report::sessions(&store, &cfg, 0)?;
    assert_eq!(
        rows.iter().find(|r| r.path == "known").unwrap().context_pct,
        Some(25.0)
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.path == "unknown")
            .unwrap()
            .context_pct,
        None
    );
    Ok(())
}

#[test]
fn policy_warns_from_catalog_only_window() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut cfg = Config::builtin();
    cfg.opencode_models = Some(ModelCatalog::from_json(CATALOG)?);
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://fixture")?;
    store.set_model(sid, "opencode-go/fixture-kimi")?;
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 750000,
            window_tokens: None,
        },
    )?;
    store.upsert_terminal(&agmag::store::TerminalRow {
        address: "t-fixture".into(),
        app: Some("opencode".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.link_terminal("t-fixture", sid, "auto")?;
    let hc = common::FakeHc::default();
    agmag::policy::tick(&store, &cfg, &hc, 2)?;
    let inbox = store.inbox(true)?;
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].source, "context");
    assert_eq!(
        agmag::policy::session_context_pct(&store, &cfg, sid)?,
        Some(75.0)
    );
    Ok(())
}

#[test]
fn zero_token_opencode_session_has_no_warning_but_codex_still_does() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let cfg = Config::builtin();
    for (source, path) in [
        (Source::OpenCode, "opencode://empty"),
        (Source::Codex, "codex://empty"),
    ] {
        let sid = store.session_id_for_path(source, path)?;
        store.insert_turn(
            sid,
            &agmag::model::TurnSummary {
                ts_ms: 1,
                role: agmag::model::Role::Assistant,
                summary: "error".into(),
                byte_offset: 0,
            },
        )?;
    }
    let rows = agmag::report::sessions(&store, &cfg, 0)?;
    let opencode = rows.iter().find(|r| r.path == "opencode://empty").unwrap();
    assert_eq!(opencode.warning, None);
    assert_eq!(opencode.context_pct, None);
    let codex = rows.iter().find(|r| r.path == "codex://empty").unwrap();
    assert_eq!(codex.warning.as_deref(), Some("unknown format?"));
    Ok(())
}

#[test]
fn catalog_rejects_bad_json_and_ignores_unusable_entries() -> anyhow::Result<()> {
    assert!(ModelCatalog::from_json("not json").is_err());
    let empty = ModelCatalog::from_json(r#"{"data":[{"providerID":"p","id":"m"}]}"#)?;
    assert_eq!(empty.context("p/m"), None);
    assert!(empty.is_empty());
    Ok(())
}

mod common;
