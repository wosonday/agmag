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

fn interactive_hc(app: &str, status: &str) -> FakeHc {
    FakeHc {
        get_responses: [(
            "t1".into(),
            serde_json::json!({
                "address":"t1", "state":"live", "status":{"app":app,"state":status}
            }),
        )]
        .into(),
        ..Default::default()
    }
}

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
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 380_000,
            window_tokens: Some(475_000),
        },
    )?; // 80%
    store.link_terminal("t1", sid, "auto")?;
    Ok(store)
}

fn state(store: &Store) -> String {
    store
        .conn()
        .query_row("SELECT state FROM rotations", [], |r| r.get(0))
        .unwrap()
}

fn go_idle(store: &Store, turn: i64) {
    store
        .set_terminal_status("t1", Some("idle"), None, None, turn, 0)
        .unwrap();
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
    assert!(
        calls[0].starts_with("send t1 if_idle=true") && calls[0].contains(".agents/log/a.md"),
        "{calls:?}"
    );
    Ok(())
}

#[test]
fn rotation_kills_and_respawns_after_commit() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit {
        touched: Some("abc123".into()),
        clean: true,
        ..Default::default()
    };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, MIN)?; // chưa có lượt mới → chờ
    assert_eq!(state(&store), "requested");
    go_idle(&store, 6);
    rotation::tick(&store, &Config::builtin(), &hc, &git, 2 * MIN)?;
    assert_eq!(state(&store), "done");
    assert_eq!(git.asked.borrow()[0], "base .agents/log/a.md");
    let calls = hc.calls.borrow();
    assert_eq!(calls[1], "kill t1");
    assert!(
        calls[2].starts_with("run [\"worker-mid\"")
            && calls[2].contains("w-a-2")
            && calls[2].contains(r"D:\\w"),
        "{calls:?}"
    );
    let new: String = store
        .conn()
        .query_row("SELECT new_terminal FROM rotations", [], |r| r.get(0))?;
    assert_eq!(new, "t99");
    Ok(())
}

#[test]
fn rotation_fails_to_inbox_without_commit() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit {
        touched: None,
        clean: true,
        ..Default::default()
    };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    go_idle(&store, 6);
    rotation::tick(&store, &Config::builtin(), &hc, &git, MIN)?;
    // Lượt đầu chưa thấy commit → vẫn chờ (agent có thể cần thêm lượt), không kill.
    assert_eq!(state(&store), "requested");
    rotation::tick(&store, &Config::builtin(), &hc, &git, 11 * MIN)?;
    assert_eq!(state(&store), "failed");
    assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("kill")));
    assert!(
        store
            .inbox(true)?
            .iter()
            .any(|i| i.source == "rotation" && i.terminal.as_deref() == Some("t1"))
    );
    Ok(())
}

#[test]
fn rotation_timeout_to_inbox() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit {
        touched: Some("x".into()),
        clean: true,
        ..Default::default()
    };
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
        let n: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get(0))?;
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
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get(0))?;
    assert_eq!(n, 0);
    Ok(())
}

#[test]
fn rotation_at_max_workers_replaces_old_worker() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let mut cfg = Config::builtin();
    cfg.policy.max_workers = 1; // thợ cũ là thợ duy nhất
    let hc = FakeHc::default();
    let git = FakeGit {
        touched: Some("abc".into()),
        clean: true,
        ..Default::default()
    };
    rotation::tick(&store, &cfg, &hc, &git, 0)?;
    go_idle(&store, 6);
    rotation::tick(&store, &cfg, &hc, &git, MIN)?;
    assert_eq!(state(&store), "done", "{:?}", hc.calls.borrow());
    assert_eq!(store.terminal("t1")?.unwrap().state, "finished");
    Ok(())
}

#[test]
fn rotation_prechecks_before_kill() -> Result<()> {
    let store = setup("opencode", "idle")?;
    // Ngân sách cứng đã vượt → không được mở phiên mới → KHÔNG được kill phiên cũ.
    store.set_budget(&agmag::store::BudgetRow {
        scope: "task:a".into(),
        unit: "tokens".into(),
        soft: 0.0,
        hard: 0.0,
        action: "notify".into(),
    })?;
    let hc = FakeHc::default();
    let git = FakeGit {
        touched: Some("abc".into()),
        clean: true,
        ..Default::default()
    };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    go_idle(&store, 6);
    rotation::tick(&store, &Config::builtin(), &hc, &git, MIN)?;
    assert_eq!(state(&store), "failed");
    assert!(
        !hc.calls.borrow().iter().any(|c| c.starts_with("kill")),
        "{:?}",
        hc.calls.borrow()
    );
    assert_eq!(store.terminal("t1")?.unwrap().state, "live");
    Ok(())
}

#[test]
fn failed_rotation_is_not_retried_automatically() -> Result<()> {
    let store = setup("opencode", "idle")?;
    let hc = FakeHc::default();
    let git = FakeGit {
        touched: None,
        clean: true,
        ..Default::default()
    };
    rotation::tick(&store, &Config::builtin(), &hc, &git, 0)?;
    go_idle(&store, 6);
    rotation::tick(&store, &Config::builtin(), &hc, &git, 11 * MIN)?; // thất bại (quá hạn)
    let sends_before = hc.calls.borrow().len();
    rotation::tick(&store, &Config::builtin(), &hc, &git, 12 * MIN)?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, 13 * MIN)?;
    let n: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get(0))?;
    assert_eq!(n, 1);
    assert_eq!(hc.calls.borrow().len(), sends_before);
    // Người dùng chủ động xoay lại thì được.
    assert!(rotation::start(&store, "t1", None, true, 14 * MIN)?);
    Ok(())
}

#[test]
fn rotation_fails_early_without_cwd() -> Result<()> {
    let store = setup("opencode", "idle")?;
    store
        .conn()
        .execute("UPDATE terminals SET cwd = NULL", [])?;
    let hc = FakeHc::default();
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 0)?;
    assert_eq!(state(&store), "failed");
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn rotation_does_not_send_handoff_while_done_only_when_idle() -> Result<()> {
    let store = setup("opencode", "done")?;
    let hc = FakeHc::default();
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 0)?;
    // `done` có thể vừa chuyển sang lượt mới; chỉ gửi khi `idle` (kèm --if idle).
    assert_eq!(state(&store), "pending");
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn claude_managed_rotation_done_status_handoff_no_kill_and_inherits_launch() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo)?;
    // Tạo repo git thật để kiểm `ensure_handoff_excluded` qua `git rev-parse --git-path info/exclude`.
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()?
            .success()
    );
    let cwd = repo.to_string_lossy().to_string();

    let store = Store::open_in_memory()?;
    let spec = agmag::launch::parse_launch_spec(
        "claude-code",
        &[
            "claude".into(),
            "--model".into(),
            "opus".into(),
            "--add-dir".into(),
            "extra".into(),
        ],
        &cwd,
    )?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        name: Some("arch".into()),
        app: Some("claude-code".into()),
        cwd: Some(cwd.clone()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_launch("t1", true, Some(&spec.to_json()?))?;
    store.set_terminal_status("t1", Some("done"), None, None, 5, 0)?;
    let sid = store.session_id_for_path(Source::Claude, "claude://s1")?;
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 160_000,
            window_tokens: Some(200_000),
        },
    )?;
    store.link_terminal("t1", sid, "hook")?;

    let hc = interactive_hc("claude-code", "done");
    let git = FakeGit::default();
    // Tick 1: phát hiện 80% >= 70%, gửi yêu cầu bàn giao với if_idle=false (vì status="done").
    rotation::tick(&store, &Config::builtin(), &hc, &git, 1_000)?;
    assert_eq!(state(&store), "requested");
    let send_call = hc.calls.borrow()[0].clone();
    assert!(
        send_call.starts_with("send t1 if_idle=false force=false "),
        "{send_call}"
    );
    let exclude = std::fs::read_to_string(repo.join(".git/info/exclude"))?;
    assert!(
        exclude.lines().any(|l| l.trim() == ".agents/handoff/"),
        "{exclude}"
    );

    // Ghi tệp bàn giao đúng đường dẫn với marker ở dòng cuối (kèm CRLF).
    let rel: String = store
        .conn()
        .query_row("SELECT handoff_path FROM rotations", [], |r| r.get(0))?;
    let abs = repo.join(&rel);
    std::fs::write(
        &abs,
        "rotation_id: 1\r\nDa lam xong phan A, con phan B.\r\n<!-- agm:handoff-done -->\r\n",
    )?;
    store.set_terminal_status("t1", Some("done"), None, None, 6, 2_000)?;

    // Tick 2: handoff_ready → spawn t99 với `--` trước prompt 1 dòng ASCII, KHÔNG kill t1!
    rotation::tick(&store, &Config::builtin(), &hc, &git, 2_000)?;
    assert_eq!(state(&store), "awaiting_link");
    let calls = hc.calls.borrow().clone();
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("kill") || c.starts_with("interrupt")),
        "{calls:?}"
    );
    let run_call = calls
        .iter()
        .find(|c| c.starts_with("run "))
        .expect("run call");
    assert!(
        run_call.contains("\"--add-dir\", \"extra\", \"--\", \"Read .agents/handoff/"),
        "{run_call}"
    );
    assert!(
        run_call.contains("\"agm.parent_rotation\": \"1\""),
        "{run_call}"
    );

    // `link_all` không ghép non terminal có `agm.parent_rotation`.
    let sid_new = store.session_id_for_path(Source::Claude, "claude://s2")?;
    store.set_meta(
        sid_new,
        &agmag::model::SessionMeta {
            session_id: "s2".into(),
            cwd: Some(cwd.clone()),
            cli_version: None,
            started_ms: Some(2_000),
        },
    )?;
    assert_eq!(agmag::correlate::link_all(&store)?, 0);

    // Nếu không phải hook thì Claude vẫn chưa tính là linked.
    store.link_terminal("t99", sid_new, "auto")?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, 3_000)?;
    assert_eq!(state(&store), "awaiting_link");

    // Khi hook statusline gắn `link_source = "hook"` → hoàn tất và đánh dấu old handed_off + new managed.
    store.link_terminal("t99", sid_new, "hook")?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, 4_000)?;
    assert_eq!(state(&store), "done");
    let old = store.terminal("t1")?.unwrap();
    let new = store.terminal("t99")?.unwrap();
    assert_eq!(old.handoff_state, "handed_off");
    assert_eq!(old.state, "live");
    assert!(new.managed);
    assert!(new.launch_spec.is_some());

    // Phiên cũ `handed_off` không bao giờ tự xoay vòng lần nữa.
    assert!(!rotation::start(&store, "t1", None, false, 5_000)?);
    Ok(())
}

#[test]
fn pending_rotation_cancels_when_context_drops_below_warn() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let store = Store::open_in_memory()?;
    let spec = agmag::launch::parse_launch_spec(
        "codex",
        &["codex".into()],
        &tmp.path().to_string_lossy(),
    )?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        app: Some("codex".into()),
        cwd: Some(tmp.path().to_string_lossy().into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_launch("t1", true, Some(&spec.to_json()?))?;
    store.set_terminal_status("t1", Some("working"), None, None, 5, 0)?;
    let sid = store.session_id_for_path(Source::Codex, "codex://s1")?;
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 380_000,
            window_tokens: Some(475_000),
        },
    )?;
    store.link_terminal("t1", sid, "auto")?;

    let hc = FakeHc::default();
    let git = FakeGit::default();
    rotation::tick(&store, &Config::builtin(), &hc, &git, 1_000)?;
    assert_eq!(state(&store), "pending");

    // Sau /compact, ngữ cảnh giảm xuống dưới ctx_warn (50%) → hủy pending.
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 2,
            used_tokens: 100_000,
            window_tokens: Some(475_000),
        },
    )?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, 2_000)?;
    assert_eq!(state(&store), "cancelled");
    assert!(hc.calls.borrow().is_empty());
    Ok(())
}

#[test]
fn codex_rotation_continues_if_old_exits_after_handoff_ready_and_links_by_tag() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path().to_string_lossy().to_string();
    let store = Store::open_in_memory()?;
    let spec = agmag::launch::parse_launch_spec(
        "codex",
        &["codex".into(), "-m".into(), "gpt-6.1-sol".into()],
        &cwd,
    )?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        app: Some("codex".into()),
        cwd: Some(cwd.clone()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_launch("t1", true, Some(&spec.to_json()?))?;
    store.set_terminal_status("t1", Some("idle"), None, None, 1, 0)?;
    let sid = store.session_id_for_path(Source::Codex, "codex://old")?;
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 380_000,
            window_tokens: Some(475_000),
        },
    )?;
    store.link_terminal("t1", sid, "auto")?;

    let hc = interactive_hc("codex", "idle");
    let git = FakeGit::default();
    rotation::tick(&store, &Config::builtin(), &hc, &git, 1_000)?;
    assert_eq!(state(&store), "requested");

    let rel: String = store
        .conn()
        .query_row("SELECT handoff_path FROM rotations", [], |r| r.get(0))?;
    std::fs::write(
        tmp.path().join(&rel),
        "rotation_id: 1\nNoi dung ban giao day du cho codex.\n<!-- agm:handoff-done -->\n",
    )?;
    store.set_terminal_status("t1", Some("idle"), None, None, 2, 2_000)?;
    // Phiên cũ thoát ngay sau khi chuyển sang handoff_ready vẫn phải mở/ghép tiếp.
    store.conn().execute(
        "UPDATE rotations SET state = 'handoff_ready', handoff_ms = 2000",
        [],
    )?;
    store.finish_terminal("t1", Some(0), 2_500)?;
    rotation::tick(&store, &Config::builtin(), &hc, &git, 3_000)?;
    assert_eq!(state(&store), "awaiting_link");

    // Codex ghi `<environment_context>` làm turn user đầu tiên, sau đó mới tới turn chứa `[agm-rot:1]`.
    let sid_new = store.session_id_for_path(Source::Codex, "codex://new")?;
    store.set_meta(
        sid_new,
        &agmag::model::SessionMeta {
            session_id: "new".into(),
            cwd: Some(cwd),
            cli_version: None,
            started_ms: Some(3_000),
        },
    )?;
    store.insert_turn(
        sid_new,
        &agmag::model::TurnSummary {
            ts_ms: 3_010,
            role: agmag::model::Role::User,
            summary: "<environment_context>...</environment_context>".into(),
            byte_offset: 0,
        },
    )?;
    store.insert_turn(
        sid_new,
        &agmag::model::TurnSummary {
            ts_ms: 3_020,
            role: agmag::model::Role::User,
            summary: "[agm-rot:1] Read .agents/handoff/1-1000.md...".into(),
            byte_offset: 100,
        },
    )?;

    rotation::tick(&store, &Config::builtin(), &hc, &git, 4_000)?;
    assert_eq!(state(&store), "done");
    assert_eq!(store.terminal("t99")?.unwrap().session, Some(sid_new));
    assert_eq!(store.terminal("t1")?.unwrap().handoff_state, "handed_off");
    Ok(())
}

#[test]
fn link_timeout_moves_to_needs_attention() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path().to_string_lossy().to_string();
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        app: Some("codex".into()),
        cwd: Some(cwd.clone()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.upsert_terminal(&TerminalRow {
        address: "t99".into(),
        app: Some("codex".into()),
        cwd: Some(cwd.clone()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.conn().execute(
        "INSERT INTO rotations (old_terminal, new_terminal, app, cwd, state, requested_ms, updated_ms)
         VALUES ('t1', 't99', 'codex', ?1, 'awaiting_link', 1000, 1000)",
        [&cwd],
    )?;
    rotation::tick(
        &store,
        &Config::builtin(),
        &FakeHc::default(),
        &FakeGit::default(),
        7 * MIN,
    )?;
    assert_eq!(state(&store), "needs_attention");
    assert!(
        store
            .inbox(true)?
            .iter()
            .any(|i| i.kind.as_deref() == Some("needs_attention"))
    );
    Ok(())
}

#[test]
fn manual_rotate_is_not_cancelled_below_ctx_warn() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path().to_string_lossy().to_string();
    let store = Store::open_in_memory()?;
    let spec = agmag::launch::parse_launch_spec("codex", &["codex".into()], &cwd)?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        app: Some("codex".into()),
        cwd: Some(cwd),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_launch("t1", true, Some(&spec.to_json()?))?;
    store.set_terminal_status("t1", Some("idle"), None, None, 1, 0)?;
    let sid = store.session_id_for_path(Source::Codex, "codex://s-manual")?;
    // Chỉ dùng 10% ngữ cảnh (< ctx_warn 50%) — gọi tay `agm rotate` vẫn phải chạy, không được hủy.
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 47_500,
            window_tokens: Some(475_000),
        },
    )?;
    store.link_terminal("t1", sid, "auto")?;

    assert!(rotation::start(&store, "t1", None, true, 1_000)?);
    let hc = interactive_hc("codex", "idle");
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 2_000)?;
    assert_eq!(state(&store), "requested");
    assert_eq!(hc.calls.borrow().len(), 1);
    Ok(())
}

#[test]
fn unmanaged_claude_and_codex_warn_at_ctx_rotate() -> Result<()> {
    for (app, source, path, window) in [
        (
            "claude-code",
            Source::Claude,
            "claude://unmanaged",
            200_000_u64,
        ),
        ("codex", Source::Codex, "codex://unmanaged", 475_000_u64),
    ] {
        let store = Store::open_in_memory()?;
        store.upsert_terminal(&TerminalRow {
            address: "t1".into(),
            name: Some("tay".into()),
            app: Some(app.into()),
            state: "live".into(),
            ..Default::default()
        })?;
        let sid = store.session_id_for_path(source, path)?;
        // 86% ngữ cảnh (≥ ctx_rotate) ở tick đầu.
        store.set_context(
            sid,
            &ContextSnapshot {
                ts_ms: 1,
                used_tokens: window * 86 / 100,
                window_tokens: Some(window),
            },
        )?;
        store.link_terminal("t1", sid, "hook")?;

        let hc = FakeHc::default();
        rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 1_000)?;
        // % tăng lên 87% → nội dung cảnh báo đổi nhưng không được mở thêm mục mới.
        store.set_context(
            sid,
            &ContextSnapshot {
                ts_ms: 2,
                used_tokens: window * 87 / 100,
                window_tokens: Some(window),
            },
        )?;
        rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 2_000)?;

        // Không tạo rotation, không gọi hc, chỉ có đúng 1 cảnh báo `rotation`/`unmanaged` mang % mới nhất.
        let rot_cnt: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM rotations", [], |r| r.get(0))?;
        assert_eq!(rot_cnt, 0, "{app}");
        assert!(hc.calls.borrow().is_empty(), "{app}");
        let warns: Vec<_> = store
            .inbox(false)?
            .into_iter()
            .filter(|i| i.source == "rotation" && i.kind.as_deref() == Some("unmanaged"))
            .collect();
        assert_eq!(warns.len(), 1, "{app}: {warns:?}");
        assert!(warns[0].text.contains("agm manage t1"), "{app}");
        assert!(warns[0].text.contains(&format!("({app})")), "{app}");
        assert!(warns[0].text.contains("87%"), "{app}: {}", warns[0].text);
    }
    Ok(())
}

#[test]
fn ctx_rotate_triggers_from_catalog_only_window() -> Result<()> {
    let catalog = r#"{"data":[{"providerID":"opencode-go","id":"fixture-kimi","limit":{"context":1000000}}]}"#;
    let mut cfg = Config::builtin();
    cfg.opencode_models = Some(agmag::opencode_models::ModelCatalog::from_json(catalog)?);
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        name: Some("w-a-1".into()),
        app: Some("opencode".into()),
        cwd: Some(r"D:\w".into()),
        task: Some("a".into()),
        role: Some("worker".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_status("t1", Some("working"), None, None, 5, 0)?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://catalog-only")?;
    store.set_model(sid, "opencode-go/fixture-kimi")?;
    // 80% của 1 000 000 token (≥ ctx_rotate 70%) nhưng phiên chỉ có cửa sổ từ danh mục.
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 800_000,
            window_tokens: None,
        },
    )?;
    store.link_terminal("t1", sid, "auto")?;

    rotation::tick(&store, &cfg, &FakeHc::default(), &FakeGit::default(), 0)?;

    assert_eq!(state(&store), "pending");
    let pct: Option<f64> = store
        .conn()
        .query_row("SELECT ctx_pct FROM rotations", [], |r| r.get(0))?;
    assert_eq!(pct, Some(80.0));
    Ok(())
}

#[test]
fn requested_continues_when_handoff_file_ready_even_if_old_exited() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path().to_string_lossy().to_string();
    let store = Store::open_in_memory()?;
    let spec = agmag::launch::parse_launch_spec("codex", &["codex".into()], &cwd)?;
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        app: Some("codex".into()),
        cwd: Some(cwd.clone()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_launch("t1", true, Some(&spec.to_json()?))?;
    store.set_terminal_status("t1", Some("idle"), None, None, 1, 0)?;

    assert!(rotation::start(&store, "t1", None, true, 1_000)?);
    let hc = interactive_hc("codex", "idle");
    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 1_500)?;
    assert_eq!(state(&store), "requested");

    let rel: String = store
        .conn()
        .query_row("SELECT handoff_path FROM rotations", [], |r| r.get(0))?;
    std::fs::write(
        tmp.path().join(&rel),
        "rotation_id: 1\nNoi dung ban giao da hoan tat.\n<!-- agm:handoff-done -->\n",
    )?;
    store.finish_terminal("t1", Some(0), 2_000)?;

    rotation::tick(&store, &Config::builtin(), &hc, &FakeGit::default(), 2_500)?;
    assert_eq!(state(&store), "awaiting_link");
    assert_eq!(store.terminal("t99")?.unwrap().state, "live");
    Ok(())
}
