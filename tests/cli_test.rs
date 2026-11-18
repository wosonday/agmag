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
    fs::copy("tests/fixtures/claude/mini.jsonl", claude_dir.join("s-1.jsonl")).unwrap();
    tmp
}

fn agm(tmp: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agm"))
        .args(args)
        .env("AGM_HOME", tmp.join("home"))
        .env("AGM_CODEX_DIR", tmp.join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.join("claude"))
        .env("AGM_OPENCODE_DB", tmp.join("opencode.db"))
        .output()
        .unwrap()
}

fn json(out: &Output) -> Value {
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
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
    let v = json(&agm(tmp.path(), &["cost", "--by", "model", "--json", "--days", "36500"]));
    let row = v.as_array().unwrap().iter().find(|r| r["key"] == "mystery-1").expect("mystery-1 row");
    assert!(row["cost_usd"].is_null());
    assert!(row["usage"]["output"].as_u64().unwrap() > 0);
}

#[test]
fn cli_quota_json() {
    let tmp = sandbox(None);
    let v = json(&agm(tmp.path(), &["quota", "--json"]));
    assert!(v.as_array().unwrap().iter().any(|r| r["used_percent"] == 22.0));
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
    assert_eq!(v.as_array().unwrap().last().unwrap()["summary"], "chào bạn lần nữa");
}

fn agm_env(tmp: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_agm"));
    c.args(args)
        .env("AGM_HOME", tmp.join("home"))
        .env("AGM_CODEX_DIR", tmp.join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.join("claude"))
        .env("AGM_OPENCODE_DB", tmp.join("opencode.db"))
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
    let out = agm_env(tmp.path(), &["send", "user", "--type", "HỎI", "--task", "a", "đổi", "đặc", "tả?"], &[("HC_TERMINAL_ID", "t5")]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]));
    assert_eq!(v[0]["text"], "[HỎI] [ID a] đổi đặc tả?");
    assert_eq!(v[0]["terminal"], "t5");
}

#[test]
fn cli_budget_set_ls_rm() {
    let tmp = sandbox(None);
    assert!(agm_env(tmp.path(), &["budget", "set", "task:a", "--soft", "100", "--hard", "200"], &[]).status.success());
    assert!(agm_env(tmp.path(), &["budget", "set", "provider:codex:10080", "--soft", "80", "--hard", "95"], &[]).status.success());
    let v = json(&agm_env(tmp.path(), &["budget", "ls", "--json"], &[]));
    let rows = v.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r["scope"] == "task:a" && r["unit"] == "tokens"));
    assert!(rows.iter().any(|r| r["scope"] == "provider:codex:10080" && r["unit"] == "percent"));
    assert!(!agm_env(tmp.path(), &["budget", "set", "bừa", "--soft", "1", "--hard", "2"], &[]).status.success());
    assert!(agm_env(tmp.path(), &["budget", "rm", "task:a"], &[]).status.success());
}

#[test]
fn cli_policy_toggle() {
    let tmp = sandbox(None);
    assert!(agm_env(tmp.path(), &["policy", "off"], &[]).status.success());
    let v = json(&agm_env(tmp.path(), &["policy", "status", "--json"], &[]));
    assert_eq!(v["enabled"], false);
    assert!(agm_env(tmp.path(), &["policy", "on"], &[]).status.success());
    assert_eq!(json(&agm_env(tmp.path(), &["policy", "status", "--json"], &[]))["enabled"], true);
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
    assert!(agm_env(tmp.path(), &["daemon", "--replay", events.to_str().unwrap()], &[]).status.success());
    let terms = json(&agm_env(tmp.path(), &["ps", "--json"], &[]));
    assert_eq!(terms[0]["app"], "codex");
    // Lệnh từ một agent (có HC_TERMINAL_ID, không TTY) → bị từ chối, không gọi hc.
    let out = agm_env(tmp.path(), &["kill", "t1q"], &[("HC_TERMINAL_ID", "t9")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("chỉ người dùng"), "{}", String::from_utf8_lossy(&out.stderr));
    let inbox = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]));
    assert!(inbox.as_array().unwrap().iter().any(|i| i["source"] == "request"));
    assert!(inbox.as_array().unwrap().iter().any(|i| i["source"] == "blocked" && i["kind"] == "permission"));
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
    assert!(agm_env(tmp.path(), &["daemon", "--replay", events.to_str().unwrap()], &[]).status.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_agm"))
        .args(["tui", "--refresh", "60"])
        .env("AGM_HOME", tmp.path().join("home"))
        .env("AGM_CODEX_DIR", tmp.path().join("codex"))
        .env("AGM_CLAUDE_DIR", tmp.path().join("claude"))
        .env("AGM_OPENCODE_DB", tmp.path().join("opencode.db"))
        .env("AGM_HC_BIN", tmp.path().join("khong-co-hc"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"k t1q\ny\nq\n").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    let inbox = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]));
    assert!(inbox.as_array().unwrap().iter().any(|i| i["source"] == "request"), "{inbox}");
}

#[test]
fn cli_emergency_commands_work_with_broken_config() {
    let tmp = sandbox(None);
    fs::write(tmp.path().join("home/providers.toml"), "[[[").unwrap();
    assert!(agm_env(tmp.path(), &["policy", "off"], &[]).status.success());
    assert!(agm_env(tmp.path(), &["inbox", "--json"], &[]).status.success());
}

#[test]
fn cli_done_dismisses_inbox_item() {
    let tmp = sandbox(None);
    assert!(agm_env(tmp.path(), &["send", "user", "--type", "BÁO CÁO", "xong"], &[("HC_TERMINAL_ID", "t5")]).status.success());
    let id = json(&agm_env(tmp.path(), &["inbox", "--json"], &[]))[0]["id"].as_i64().unwrap();
    assert!(agm_env(tmp.path(), &["done", &id.to_string()], &[]).status.success());
    assert!(json(&agm_env(tmp.path(), &["inbox", "--json"], &[])).as_array().unwrap().is_empty());
}
