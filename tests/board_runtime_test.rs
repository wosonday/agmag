//! Task 4: board runtime — caller guard, project lookup, data load, effects, `cd -t` parsing.

mod common;

use common::FakeHc;

use agmag::board;
use agmag::board::state::{BoardState, Effect, Panel};
use agmag::board::theme::{self, ThemeName};
use agmag::config::Config;
use agmag::correlate::norm_cwd;
use agmag::dashboard::{TuiCmd, parse_command};
use agmag::project::{ProjectKey, RootCache};
use agmag::store::{Store, TerminalRow};

fn term(address: &str, name: &str, cwd: &str) -> TerminalRow {
    TerminalRow {
        address: address.into(),
        name: Some(name.into()),
        cwd: Some(cwd.into()),
        state: "live".into(),
        created_ms: 1,
        ..Default::default()
    }
}

#[test]
fn require_human_refuses_non_tty() {
    // Piped stdio is not a human terminal, exactly like `agm tui`.
    let tmp = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_agm"))
        .arg("board")
        .env("AGM_HOME", tmp.path().join("home"))
        .env("AGM_CODEX_DIR", tmp.path().join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.path().join("claude"))
        .env("AGM_OPENCODE_DB", tmp.path().join("opencode.db"))
        .env("AGM_AGY_DIR", tmp.path().join("agy"))
        .env_remove("HC_TERMINAL_ID")
        .output()
        .unwrap();
    assert!(!out.status.success(), "board must refuse a piped caller");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("agm board"), "stderr: {stderr}");
}

#[test]
fn resolve_project_unknown_and_ambiguous() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let a = tmp.path().join("a").join("shop");
    let b = tmp.path().join("b").join("shop");
    let unique = tmp.path().join("unique");
    std::fs::create_dir_all(&a)?;
    std::fs::create_dir_all(&b)?;
    std::fs::create_dir_all(&unique)?;

    let store = Store::open_in_memory()?;
    store.upsert_terminal(&term("t1", "one", a.to_str().unwrap()))?;
    store.upsert_terminal(&term("t2", "two", b.to_str().unwrap()))?;
    store.upsert_terminal(&term("t3", "three", unique.to_str().unwrap()))?;
    let cfg = Config::builtin();
    let mut cache = RootCache::new();

    // Two repositories called `shop`: an error listing both, before any raw mode is entered.
    let err = board::resolve_project(&store, &cfg, &mut cache, "shop").unwrap_err();
    let text = format!("{err:#}");
    assert!(text.starts_with("ambiguous: "), "{text}");
    assert!(text.contains("shop ("), "{text}");

    let err = board::resolve_project(&store, &cfg, &mut cache, "zzz").unwrap_err();
    assert_eq!(format!("{err:#}"), "no project matching 'zzz'");

    let found = board::resolve_project(&store, &cfg, &mut cache, "uniq")?;
    assert_eq!(found.name, "unique");
    Ok(())
}

#[test]
fn load_data_without_project_lists_projects() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path().join("shop");
    std::fs::create_dir_all(&dir)?;
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&term("t1", "ceo", dir.to_str().unwrap()))?;

    let cfg = Config::builtin();
    let mut cache = RootCache::new();
    let mut state = BoardState::new(None, ThemeName::default());
    let data = board::load_data(&store, &cfg, &mut cache, &mut state, 1_000_000)?;

    assert_eq!(data.projects.len(), 1);
    assert_eq!(data.projects[0].key.name, "shop");
    assert!(data.view.is_none());
    assert!(data.edges.is_empty());
    assert!(data.conversation.is_empty());
    assert_eq!(data.now_ms, 1_000_000);
    assert_eq!(state.panel, Panel::Projects);
    assert!(state.sel_agent.is_none() && state.sel_inbox.is_none());
    Ok(())
}

#[test]
fn load_data_hides_idle_projects_but_keeps_the_scoped_one() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let live_dir = tmp.path().join("live");
    let ghost_dir = tmp.path().join("ghost");
    let scoped_dir = tmp.path().join("scoped");
    std::fs::create_dir_all(&live_dir)?;
    std::fs::create_dir_all(&ghost_dir)?;
    std::fs::create_dir_all(&scoped_dir)?;

    let mut ghost = term("t2", "ghost", ghost_dir.to_str().unwrap());
    ghost.state = "exited".into();
    let mut scoped_idle = term("t3", "scoped", scoped_dir.to_str().unwrap());
    scoped_idle.state = "exited".into();

    let store = Store::open_in_memory()?;
    store.upsert_terminal(&term("t1", "ceo", live_dir.to_str().unwrap()))?;
    store.upsert_terminal(&ghost)?;
    store.upsert_terminal(&scoped_idle)?;

    let cfg = Config::builtin();
    let mut cache = RootCache::new();

    // No scope: both exited projects are idle and must leave the display list.
    let mut state = BoardState::new(None, ThemeName::default());
    let data = board::load_data(&store, &cfg, &mut cache, &mut state, 1_000_000)?;
    assert_eq!(data.projects.len(), 1, "{:?}", data.projects);
    assert_eq!(data.projects[0].key.name, "live");

    // Scoped to an idle project: it stays in the column, and still opens its board view.
    let scoped = ProjectKey {
        root: norm_cwd(scoped_dir.to_str().unwrap()),
        name: "scoped".into(),
    };
    let mut state = BoardState::new(Some(scoped), ThemeName::default());
    let data = board::load_data(&store, &cfg, &mut cache, &mut state, 1_000_000)?;
    let names: Vec<_> = data.projects.iter().map(|p| p.key.name.as_str()).collect();
    assert_eq!(names, vec!["live", "scoped"], "{names:?}");
    assert!(data.view.is_some());
    Ok(())
}

#[test]
fn apply_effect_errors_become_notes() {
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc::default();
    let mut state = BoardState::new(None, ThemeName::default());

    // `agm done 404` errors; the board keeps running with an error note.
    board::apply_effect(&store, &hc, Effect::Done { id: 404 }, &mut state, 1_000);
    let note = state.note.clone().expect("done note");
    assert!(!note.ok, "{note:?}");
    assert!(note.text.starts_with("error: "), "{note:?}");

    // Reply to an already-resolved item errors too.
    let id = store
        .open_inbox(Some("t1"), "message", Some("HỎI"), "need a decision", 1_000)
        .unwrap()
        .expect("inbox item");
    store.resolve_inbox(id, None, 1_001).unwrap();
    board::apply_effect(
        &store,
        &hc,
        Effect::Reply {
            id,
            text: "ok".into(),
        },
        &mut state,
        1_002,
    );
    let note = state.note.clone().expect("reply note");
    assert!(!note.ok, "{note:?}");
    assert!(note.text.starts_with("error: "), "{note:?}");

    // Sending to an unknown terminal is recorded and reported as failed, not swallowed.
    let failing = FakeHc {
        fail_send: true,
        ..Default::default()
    };
    board::apply_effect(
        &store,
        &failing,
        Effect::Send {
            to: "nope".into(),
            kind: "GIAO".into(),
            text: "go".into(),
        },
        &mut state,
        1_003,
    );
    let note = state.note.clone().expect("send note");
    assert!(!note.ok, "{note:?}");
    assert!(note.text.contains("nope: failed"), "{note:?}");
}

#[test]
fn apply_effect_save_theme_persists() {
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc::default();
    let mut state = BoardState::new(None, ThemeName::default());

    board::apply_effect(
        &store,
        &hc,
        Effect::SaveTheme(ThemeName::Nord),
        &mut state,
        1_000,
    );

    assert_eq!(
        store.setting(theme::SETTING_KEY).unwrap().as_deref(),
        Some("nord")
    );
    let note = state.note.expect("theme note");
    assert!(note.ok, "{note:?}");
    assert!(note.text.contains("nord"), "{note:?}");
}

#[test]
fn parse_cd_t() {
    // `cd -t <project>` keeps the reviewed text project view.
    assert_eq!(
        parse_command("cd -t shop"),
        Ok(TuiCmd::CdText("shop".into()))
    );
    assert_eq!(
        parse_command("cd -t  shop "),
        Ok(TuiCmd::CdText("shop".into()))
    );
    assert!(parse_command("cd -t").is_err(), "missing project");
    // Plain `cd` still enters the board scope.
    assert_eq!(
        parse_command("cd shop"),
        Ok(TuiCmd::Cd(Some("shop".into())))
    );
    assert_eq!(parse_command("cd"), Ok(TuiCmd::Cd(None)));
}
