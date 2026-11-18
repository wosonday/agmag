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
