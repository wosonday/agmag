use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// Dựng thư mục giả lập ~/.agmag, ~/.codex/sessions, ~/.claude/projects từ fixture mini.
fn sandbox(codex_model: Option<&str>) -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let codex_dir = tmp.path().join("codex/2026/11/18");
    let claude_dir = tmp.path().join("claude/proj");
    fs::create_dir_all(&codex_dir).unwrap();
    fs::create_dir_all(&claude_dir).unwrap();
    fs::create_dir_all(tmp.path().join("home")).unwrap();
    let mut codex = fs::read_to_string("tests/fixtures/codex/mini.jsonl").unwrap();
    if let Some(model) = codex_model {
        codex = codex.replace(r#""model":"gpt-6.1-sol""#, &format!(r#""model":"{model}""#));
    }
    fs::write(codex_dir.join("rollout-a.jsonl"), codex).unwrap();
    fs::copy(
        "tests/fixtures/claude/mini.jsonl",
        claude_dir.join("s-1.jsonl"),
    )
    .unwrap();
    tmp
}

fn agm(tmp: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agm"))
        .args(args)
        .env("AGM_HOME", tmp.join("home"))
        .env("AGM_CODEX_DIR", tmp.join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.join("claude"))
        .env("AGM_OPENCODE_DB", tmp.join("opencode.db"))
        .env("AGM_AGY_DIR", tmp.join("agy"))
        .output()
        .unwrap()
}

fn json(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn cli_ls_json() {
    let tmp = sandbox(None);
    let v = json(&agm(tmp.path(), &["ls", "--json", "--days", "36500"]));
    assert_eq!(v.as_array().unwrap().len(), 2);
}

#[test]
fn cost_json_unknown_model_null_usd() {
    let tmp = sandbox(Some("mystery-1"));
    let v = json(&agm(
        tmp.path(),
        &["cost", "--by", "model", "--json", "--days", "36500"],
    ));
    let row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "mystery-1")
        .expect("mystery-1 row");
    assert!(row["cost_usd"].is_null());
    assert!(row["usage"]["output"].as_u64().unwrap() > 0);
}

#[test]
fn cli_quota_json() {
    let tmp = sandbox(None);
    let v = json(&agm(tmp.path(), &["quota", "--json"]));
    let row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["used_percent"] == 22.0)
        .expect("22% used row");
    // JSON keeps `used_percent` (as logged) and adds `left_percent` (what the TUI/table show).
    let left = row["left_percent"].as_f64().expect("left_percent");
    assert!(
        left == 78.0 || (left == 100.0 && row["reset_passed"] == true),
        "left_percent = {left}"
    );
    // Fixture quan sát 2026-11-18, đồng hồ thật ≥ 2026-11-20 → dữ liệu cũ phải được đánh dấu.
    assert_eq!(row["stale"], true, "quota fixture 2 ngày tuổi phải stale");
    let age = row["observed_age_s"].as_i64().expect("observed_age_s");
    assert!(age > 3_600, "observed_age_s = {age}");
}

#[test]
fn cli_bad_config_exits_1() {
    let tmp = sandbox(None);
    fs::write(tmp.path().join("home/providers.toml"), "billing = 3\n[[[").unwrap();
    let out = agm(tmp.path(), &["ls", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("providers.toml"));
}

#[test]
fn cli_log_json_returns_turns() {
    let tmp = sandbox(None);
    let v = json(&agm(tmp.path(), &["log", "s-1", "--json"]));
    assert_eq!(
        v.as_array().unwrap().last().unwrap()["summary"],
        "chào bạn lần nữa"
    );
}

fn agm_env(tmp: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_agm"));
    c.args(args)
        .env("AGM_HOME", tmp.join("home"))
        .env("AGM_CODEX_DIR", tmp.join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.join("claude"))
        .env("AGM_OPENCODE_DB", tmp.join("opencode.db"))
        .env("AGM_AGY_DIR", tmp.join("agy"))
        .env("AGM_HC_BIN", tmp.join("khong-co-hc"))
        .env_remove("HC_TERMINAL_ID");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

#[test]
fn cli_send_to_user_and_inbox_json() {
    let tmp = sandbox(None);
    let out = agm_env(
        tmp.path(),
        &[
            "send", "user", "--type", "HỎI", "--task", "a", "đổi", "đặc", "tả?",
        ],
        &[("HC_TERMINAL_ID", "t5")],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]));
    assert_eq!(v[0]["text"], "[HỎI] [ID a] đổi đặc tả?");
    assert_eq!(v[0]["terminal"], "t5");
}

#[test]
fn cli_budget_set_ls_rm() {
    let tmp = sandbox(None);
    assert!(
        agm_env(
            tmp.path(),
            &["budget", "set", "task:a", "--soft", "100", "--hard", "200"],
            &[]
        )
        .status
        .success()
    );
    assert!(
        agm_env(
            tmp.path(),
            &[
                "budget",
                "set",
                "provider:codex:10080",
                "--soft",
                "80",
                "--hard",
                "95"
            ],
            &[]
        )
        .status
        .success()
    );
    let v = json(&agm_env(tmp.path(), &["budget", "ls", "--json"], &[]));
    let rows = v.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|r| r["scope"] == "task:a" && r["unit"] == "tokens")
    );
    assert!(
        rows.iter()
            .any(|r| r["scope"] == "provider:codex:10080" && r["unit"] == "percent")
    );
    assert!(
        !agm_env(
            tmp.path(),
            &["budget", "set", "bừa", "--soft", "1", "--hard", "2"],
            &[]
        )
        .status
        .success()
    );
    assert!(
        agm_env(tmp.path(), &["budget", "rm", "task:a"], &[])
            .status
            .success()
    );
}

#[test]
fn cli_policy_toggle() {
    let tmp = sandbox(None);
    assert!(
        agm_env(tmp.path(), &["policy", "off"], &[])
            .status
            .success()
    );
    let v = json(&agm_env(tmp.path(), &["policy", "status", "--json"], &[]));
    assert_eq!(v["enabled"], false);
    assert!(agm_env(tmp.path(), &["policy", "on"], &[]).status.success());
    assert_eq!(
        json(&agm_env(tmp.path(), &["policy", "status", "--json"], &[]))["enabled"],
        true
    );
}

#[test]
fn cli_replay_then_agent_cannot_kill_codex() {
    let tmp = sandbox(None);
    let events = tmp.path().join("events.jsonl");
    fs::write(
        &events,
        r#"{"channel":null,"seq":1,"terminals":[{"address":"t1q","argv":["codex"],"cwd":"D:\w","created_ms":1,"labels":{},"state":"live","reported":true,"status":{"app":"codex","state":"idle"}}]}
{"jsonrpc":"2.0","method":"event","params":{"type":"status","terminal":"t1q","seq":2,"data":{"reported":true,"status":{"app":"codex","kind":"permission","state":"blocked"}}}}
"#,
    )
    .unwrap();
    assert!(
        agm_env(
            tmp.path(),
            &["daemon", "--replay", events.to_str().unwrap()],
            &[]
        )
        .status
        .success()
    );
    let terms = json(&agm_env(tmp.path(), &["ps", "--json"], &[]));
    assert_eq!(terms[0]["app"], "codex");
    // Lệnh từ một agent (có HC_TERMINAL_ID, không TTY) → bị từ chối, không gọi hc.
    let out = agm_env(tmp.path(), &["kill", "t1q"], &[("HC_TERMINAL_ID", "t9")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("chỉ người dùng"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let inbox = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]));
    assert!(
        inbox
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["source"] == "request")
    );
    assert!(
        inbox
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["source"] == "blocked" && i["kind"] == "permission")
    );
}

#[test]
fn cli_hook_statusline_never_fails() {
    use std::io::Write;
    let tmp = sandbox(None);
    let mut child = Command::new(env!("CARGO_BIN_EXE_agm"))
        .args(["hook", "statusline"])
        .env("AGM_HOME", tmp.path().join("home"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"not json").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(!out.stdout.is_empty());
}

#[test]
fn cli_tui_with_piped_stdin_cannot_kill_codex() {
    use std::io::Write;
    let tmp = sandbox(None);
    let events = tmp.path().join("ev.jsonl");
    fs::write(&events, "{\"channel\":null,\"seq\":1,\"terminals\":[{\"address\":\"t1q\",\"argv\":[\"codex\"],\"created_ms\":1,\"labels\":{},\"state\":\"live\",\"reported\":true,\"status\":{\"app\":\"codex\",\"state\":\"idle\"}}]}\n").unwrap();
    assert!(
        agm_env(
            tmp.path(),
            &["daemon", "--replay", events.to_str().unwrap()],
            &[]
        )
        .status
        .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_agm"))
        .args(["tui", "--refresh", "60"])
        .env("AGM_HOME", tmp.path().join("home"))
        .env("AGM_CODEX_DIR", tmp.path().join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.path().join("claude"))
        .env("AGM_OPENCODE_DB", tmp.path().join("opencode.db"))
        .env("AGM_AGY_DIR", tmp.path().join("agy"))
        .env("AGM_HC_BIN", tmp.path().join("khong-co-hc"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"p off\nk t1q\ny\nq\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    // Không lệnh nào được thực hiện: policy vẫn bật.
    assert_eq!(
        json(&agm_env(tmp.path(), &["policy", "status", "--json"], &[]))["enabled"],
        true
    );
}

#[test]
fn cli_emergency_commands_work_with_broken_config() {
    let tmp = sandbox(None);
    fs::write(tmp.path().join("home/providers.toml"), "[[[").unwrap();
    assert!(
        agm_env(tmp.path(), &["policy", "off"], &[])
            .status
            .success()
    );
    assert!(
        agm_env(tmp.path(), &["inbox", "--json"], &[])
            .status
            .success()
    );
}

#[test]
fn cli_done_dismisses_inbox_item() {
    let tmp = sandbox(None);
    assert!(
        agm_env(
            tmp.path(),
            &["send", "user", "--type", "BÁO CÁO", "xong"],
            &[("HC_TERMINAL_ID", "t5")]
        )
        .status
        .success()
    );
    let id = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]))[0]["id"]
        .as_i64()
        .unwrap();
    assert!(
        agm_env(tmp.path(), &["done", &id.to_string()], &[])
            .status
            .success()
    );
    assert!(
        json(&agm_env(tmp.path(), &["inbox", "--json"], &[]))
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cli_rotate_refuses_unknown_app_and_budget_rejects_nan() {
    let tmp = sandbox(None);
    let events = tmp.path().join("ev.jsonl");
    fs::write(&events, "{\"channel\":null,\"seq\":1,\"terminals\":[{\"address\":\"t7\",\"argv\":[],\"created_ms\":1,\"labels\":{\"agm.task\":\"a\",\"agm.role\":\"worker\"},\"state\":\"live\"}]}\n").unwrap();
    assert!(
        agm_env(
            tmp.path(),
            &["daemon", "--replay", events.to_str().unwrap()],
            &[]
        )
        .status
        .success()
    );
    assert_eq!(
        agm_env(tmp.path(), &["rotate", "t7"], &[]).status.code(),
        Some(1)
    );
    assert!(
        !agm_env(
            tmp.path(),
            &["budget", "set", "task:a", "--soft", "NaN", "--hard", "5"],
            &[]
        )
        .status
        .success()
    );
    assert!(
        !agm_env(
            tmp.path(),
            &["budget", "set", "task:a", "--soft", "-1", "--hard", "5"],
            &[]
        )
        .status
        .success()
    );
}

#[test]
fn cli_agy_scan_ls_and_cost() {
    let tmp = sandbox(None);
    let agy_logs = tmp.path().join("agy/brain/conv-cli/.system_generated/logs");
    fs::create_dir_all(&agy_logs).unwrap();
    fs::write(
        agy_logs.join("transcript.jsonl"),
        concat!(
            "{\"step_index\":0,\"type\":\"USER_INPUT\",\"created_at\":\"2026-11-18T13:09:38Z\",\"content\":\"<USER_REQUEST>\\nhi\\n</USER_REQUEST>\"}\n",
            "{\"step_index\":1,\"type\":\"PLANNER_RESPONSE\",\"created_at\":\"2026-11-18T13:09:40Z\",\"model\":\"gemini-4-hydrogen-high\",\"input_tokens\":500,\"cache_read_tokens\":200,\"output_tokens\":40,\"content\":\"ok\"}\n"
        ),
    )
    .unwrap();

    let scan = json(&agm(tmp.path(), &["scan", "--days", "36500", "--json"]));
    assert!(scan["events"].as_u64().unwrap() >= 4);

    let sessions = json(&agm(tmp.path(), &["ls", "--days", "36500", "--json"]));
    assert!(
        sessions
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["source"] == "agy" && r["session_id"] == "conv-cli")
    );

    let cost = json(&agm(
        tmp.path(),
        &["cost", "--by", "source", "--days", "36500", "--json"],
    ));
    let agy_row = cost
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "agy")
        .expect("agy cost row");
    assert_eq!(agy_row["usage"]["input_uncached"], 500);
    assert_eq!(agy_row["usage"]["cache_read"], 200);
    assert_eq!(agy_row["usage"]["output"], 40);
}

#[test]
fn cli_cost_summary_window_month_and_scan_since() {
    let tmp = sandbox(None);
    let scan = json(&agm(
        tmp.path(),
        &["scan", "--since", "2020-01-01", "--json"],
    ));
    assert!(scan["files"].as_u64().unwrap() >= 2);

    // Chèn thêm bản ghi usage cách đây 10 ngày để kiểm chứng `cost --by month` (mặc định không cần --window)
    // không bị cắt về 24h (`days = 1`).
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let old_ts = now_epoch - 10 * 86_400_000;
    {
        let store = agmag::store::Store::open(&tmp.path().join("home/agm.db")).unwrap();
        let sid = store
            .session_id_for_path(agmag::model::Source::Claude, "claude://old-in-4m")
            .unwrap();
        store
            .upsert_usage(
                sid,
                &agmag::model::UsageRecord {
                    dedupe_key: "u-10d".into(),
                    ts_ms: old_ts,
                    model: Some("claude-opus-4-7".into()),
                    usage: agmag::model::Usage {
                        output: 777,
                        ..Default::default()
                    },
                },
            )
            .unwrap();
    }

    let sum = json(&agm(tmp.path(), &["cost", "--summary", "--json"]));
    assert_eq!(sum["recent_months"].as_array().unwrap().len(), 4);
    assert_eq!(sum["incomplete_coverage"], false);

    let by_m_default = json(&agm(tmp.path(), &["cost", "--by", "month", "--json"]));
    let total_out: u64 = by_m_default
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["usage"]["output"].as_u64().unwrap_or(0))
        .sum();
    assert!(
        total_out >= 777,
        "cost --by month phai bao gom usage cu hon 1 ngay"
    );

    let by_m = json(&agm(
        tmp.path(),
        &["cost", "--window", "4months", "--by", "month", "--json"],
    ));
    assert!(by_m.as_array().is_some());
}

#[test]
fn cli_manage_rotate_and_cleanup() {
    let tmp = sandbox(None);
    let work = tmp.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let claude_home = tmp.path().join("claude_home");
    fs::create_dir_all(claude_home.join(".claude")).unwrap();
    fs::write(
        claude_home.join(".claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":"\"agm\" hook statusline"}}"#,
    )
    .unwrap();

    let events = tmp.path().join("ev.jsonl");
    let ev = serde_json::json!({
        "channel": null,
        "seq": 1,
        "terminals": [{
            "address": "t11",
            "argv": ["claude", "--model", "opus"],
            "cwd": work.to_string_lossy(),
            "created_ms": 1,
            "labels": {},
            "state": "live",
            "reported": true,
            "status": {"app": "claude-code", "state": "done"}
        }]
    });
    fs::write(&events, format!("{ev}\n")).unwrap();
    assert!(
        agm_env(
            tmp.path(),
            &["daemon", "--replay", events.to_str().unwrap()],
            &[]
        )
        .status
        .success()
    );

    // Chạy qua pipe (không TTY / như agent) phải bị từ chối khi bật `agm manage`.
    let claude_home_str = claude_home.to_string_lossy().to_string();
    let m_out = agm_env(
        tmp.path(),
        &["manage", "t11", "--", "claude", "--model", "opus"],
        &[("AGM_CLAUDE_HOME", &claude_home_str)],
    );
    assert!(
        !m_out.status.success(),
        "agm manage không TTY phải bị từ chối fail-closed"
    );
    let m_err = String::from_utf8_lossy(&m_out.stderr);
    assert!(
        m_err.contains("chỉ người dùng"),
        "phải bị từ chối vì không phải người dùng, stderr: {m_err}"
    );

    // Khi người dùng xác nhận trong phiên tương tác (giả lập qua Store + Caller::User { confirmed: true }),
    // `rotate` và `cleanup --json` hoạt động bình thường.
    {
        let store = agmag::store::Store::open(&tmp.path().join("home/agm.db")).unwrap();
        let spec = agmag::launch::parse_launch_spec(
            "claude-code",
            &["claude".into(), "--model".into(), "opus".into()],
            &work.to_string_lossy(),
        )
        .unwrap();
        agmag::actions::set_managed_launch(
            &store,
            &agmag::actions::Caller::User { confirmed: true },
            "t11",
            &spec,
            10,
        )
        .unwrap();
    }

    let r_out = agm_env(tmp.path(), &["rotate", "t11"], &[]);
    assert!(
        r_out.status.success(),
        "{}",
        String::from_utf8_lossy(&r_out.stderr)
    );

    let c_out = agm_env(tmp.path(), &["cleanup", "--json"], &[]);
    assert_eq!(json(&c_out).as_array().unwrap().len(), 0);
}

#[test]
fn cli_cleanup_help_documents_mandatory_confirmation_even_with_yes() {
    let tmp = sandbox(None);
    let out = agm_env(tmp.path(), &["cleanup", "--help"], &[]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("cleanup still requires one typed confirmation")
    );
}

#[test]
fn cli_cleanup_yes_without_human_confirmation_does_not_touch_terminals() {
    let tmp = sandbox(None);
    {
        let store = agmag::store::Store::open(&tmp.path().join("home/agm.db")).unwrap();
        for address in ["t1", "t2"] {
            store
                .upsert_terminal(&agmag::store::TerminalRow {
                    address: address.into(),
                    app: Some("codex".into()),
                    state: "live".into(),
                    created_ms: 1,
                    ..Default::default()
                })
                .unwrap();
        }
        let sid = store
            .session_id_for_path(agmag::model::Source::Codex, "codex://successor")
            .unwrap();
        store.link_terminal("t2", sid, "rotation").unwrap();
        store
            .set_terminal_handoff_state("t1", "handed_off")
            .unwrap();
        store.conn().execute(
            "INSERT INTO rotations (old_terminal,new_terminal,app,state,updated_ms) VALUES ('t1','t2','codex','done',1)", []
        ).unwrap();
    }
    let out = agm_env(tmp.path(), &["cleanup", "--all", "--yes"], &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Cancelled."));
    let store = agmag::store::Store::open(&tmp.path().join("home/agm.db")).unwrap();
    assert_eq!(store.terminal("t1").unwrap().unwrap().state, "live");
    assert_eq!(
        store
            .conn()
            .query_row("SELECT COUNT(*) FROM actions WHERE kind='kill'", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}

#[test]
fn cli_help_is_english_for_every_command() {
    let tmp = sandbox(None);
    let commands: &[&[&str]] = &[
        &[],
        &["scan"],
        &["ls"],
        &["cost"],
        &["quota"],
        &["log"],
        &["daemon"],
        &["ps"],
        &["send"],
        &["inbox"],
        &["done"],
        &["reply"],
        &["spawn"],
        &["kill"],
        &["interrupt"],
        &["rotate"],
        &["manage"],
        &["cleanup"],
        &["budget"],
        &["budget", "set"],
        &["budget", "ls"],
        &["budget", "rm"],
        &["policy"],
        &["link"],
        &["ask"],
        &["hook"],
        &["hook", "statusline"],
        &["hook", "install"],
        &["sys"],
        &["tui"],
    ];
    for command in commands {
        let mut args = command.to_vec();
        args.push("--help");
        let out = agm_env(tmp.path(), &args, &[]);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let help = String::from_utf8(out.stdout).unwrap();
        if help.contains("--json") {
            assert!(help.contains("Print JSON"), "{args:?}: {help}");
        }
        // Message codes are existing protocol values, not prose to translate.
        let mut prose = help.clone();
        for code in ["CHƯA ĐẠT", "BÁO CÁO", "LỖI", "HỎI", "CHẶN", "ĐẠT", "GIAO"] {
            prose = prose.replace(code, "");
        }
        assert!(prose.is_ascii(), "{args:?}: untranslated help: {help}");
    }
}

#[test]
fn cli_stop_help_preserves_user_confirmation_warning() {
    let tmp = sandbox(None);
    for command in ["kill", "interrupt"] {
        let out = agm_env(tmp.path(), &[command, "--help"], &[]);
        assert!(out.status.success());
        let help = String::from_utf8(out.stdout).unwrap();
        assert!(
            help.contains("Codex/Claude and unknown apps still require a user to type y"),
            "{help}"
        );
    }
}

#[test]
fn cli_send_help_explains_existing_message_codes() {
    let tmp = sandbox(None);
    let out = agm_env(tmp.path(), &["send", "--help"], &[]);
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();
    for description in [
        "GIAO (assign)",
        "\"BÁO CÁO\" (report)",
        "LỖI (error)",
        "HỎI (ask)",
        "CHẶN (blocked)",
        "ĐẠT (pass)",
        "\"CHƯA ĐẠT\" (fail)",
    ] {
        assert!(help.contains(description), "{help}");
    }
}

#[test]
fn cli_done_bulk_and_legacy_cases() {
    let tmp = sandbox(None);
    // no args -> usage error
    let out = agm_env(tmp.path(), &["done"], &[]);
    assert!(!out.status.success());

    // single nonexistent -> exit != 0
    let out = agm_env(tmp.path(), &["done", "999999"], &[]);
    assert!(!out.status.success());

    // create inbox items directly for deterministic kinds/ids
    let store = agmag::store::Store::open(&tmp.path().join("home/agm.db")).unwrap();
    let a = store
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r1", 1)
        .unwrap()
        .unwrap();
    let b = store
        .open_inbox(Some("t1"), "message", Some("HỎI"), "q1", 2)
        .unwrap()
        .unwrap();
    let c = store
        .open_inbox(Some("t2"), "message", Some("BÁO CÁO"), "r2", 3)
        .unwrap()
        .unwrap();
    drop(store);

    // legacy single JSON format
    let out = agm_env(tmp.path(), &["done", &a.to_string(), "--json"], &[]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v, serde_json::json!({"done": a}));

    // bulk dismiss remaining by range
    let range = format!("{b}-{c}");
    let out = agm_env(tmp.path(), &["done", &range, "--json"], &[]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["done"], serde_json::json!([b, c]));
    assert_eq!(v["skipped"], serde_json::json!([]));
}

#[test]
fn cli_done_range_and_filters_never_close_blocked_items() {
    let tmp = sandbox(None);
    let store = agmag::store::Store::open(&tmp.path().join("home/agm.db")).unwrap();
    let r1 = store
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r1", 1)
        .unwrap()
        .unwrap();
    let blk = store
        .open_inbox(Some("t9"), "blocked", Some("permission"), "approve?", 2)
        .unwrap()
        .unwrap();
    let r2 = store
        .open_inbox(Some("t1"), "message", Some("BÁO CÁO"), "r2", 3)
        .unwrap()
        .unwrap();
    drop(store);
    let open_ids = |tmp: &std::path::Path| -> Vec<i64> {
        let s = agmag::store::Store::open(&tmp.join("home/agm.db")).unwrap();
        s.inbox(true).unwrap().iter().map(|i| i.id).collect()
    };

    // A range spanning the blocked item closes the others and reports it as skipped.
    let out = agm_env(tmp.path(), &["done", &format!("{r1}-{r2}"), "--json"], &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["done"], serde_json::json!([r1, r2]));
    assert_eq!(v["skipped"][0]["id"], serde_json::json!(blk));
    assert_eq!(open_ids(tmp.path()), vec![blk]);

    // A filter that matches it (terminal) still skips it.
    let out = agm_env(tmp.path(), &["done", "--terminal", "t9", "--json"], &[]);
    assert!(out.status.success());
    assert_eq!(json(&out)["done"], serde_json::json!([]));
    assert_eq!(open_ids(tmp.path()), vec![blk]);

    // Only an explicit --source blocked closes it.
    let out = agm_env(tmp.path(), &["done", "--source", "blocked", "--json"], &[]);
    assert!(out.status.success());
    assert_eq!(json(&out)["done"], serde_json::json!([blk]));
    assert!(open_ids(tmp.path()).is_empty());
}

#[test]
fn branch_set_then_get_roundtrip() {
    let tmp = sandbox(None);
    let file = tmp.path().join("BRANCHES.md");
    let f = file.to_str().unwrap();
    let out = agm(
        tmp.path(),
        &[
            "branch",
            "set",
            "a/x",
            "status=wip",
            "summary=Thêm cảnh báo",
            "--file",
            f,
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = agm(tmp.path(), &["--json", "branch", "get", "a/x", "--file", f]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["entry"], "a/x");
    let fields = v["fields"].as_array().unwrap();
    assert!(fields.contains(&serde_json::json!(["summary", "Thêm cảnh báo"])));
}

#[test]
fn branch_set_rejects_missing_equals() {
    let tmp = sandbox(None);
    let file = tmp.path().join("BRANCHES.md");
    let out = agm(
        tmp.path(),
        &[
            "branch",
            "set",
            "a/x",
            "status",
            "--file",
            file.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("khóa=giá-trị"));
}

#[test]
fn branch_get_missing_entry_fails() {
    let tmp = sandbox(None);
    let file = tmp.path().join("BRANCHES.md");
    let out = agm(
        tmp.path(),
        &["branch", "get", "a/nope", "--file", file.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("không có mục a/nope"));
}
