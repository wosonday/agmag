//! `agm ask`: gọi agy (Gemini) một lần ở chế độ chỉ đọc và ghi lại token.
//! Phiên agy tương tác không để lại token trong máy (P0), nên đây là cách duy nhất đo được agy.

use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::model::{summarize, Role, SessionMeta, Source, TurnSummary, Usage, UsageRecord};
use crate::store::Store;

/// Ghi kết quả `agy --output-format json` vào CSDL; trả câu trả lời.
pub fn record(store: &Store, out: &Value, question: &str, model: Option<&str>, now: i64) -> Result<String> {
    if out["status"] != "SUCCESS" {
        bail!("agy trả trạng thái {}: {}", out["status"], out["response"].as_str().unwrap_or_default());
    }
    let response = out["response"].as_str().unwrap_or_default().to_string();
    let conv = out["conversation_id"].as_str().map(String::from).unwrap_or_else(|| format!("ask-{now}"));
    let sid = store.session_id_for_path(Source::Agy, &format!("agy://{conv}"))?;
    store.set_meta(sid, &SessionMeta { session_id: conv.clone(), cwd: None, cli_version: None, started_ms: Some(now) })?;
    let model = model.unwrap_or("agy-default");
    store.set_model(sid, model)?;
    let u = &out["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let cache = n("cache_read_tokens");
    let thinking = n("thinking_tokens");
    store.upsert_usage(
        sid,
        &UsageRecord {
            dedupe_key: format!("{conv}:{now}"),
            ts_ms: now,
            model: Some(model.into()),
            usage: Usage {
                input_uncached: n("input_tokens").saturating_sub(cache),
                cache_read: cache,
                cache_write: 0,
                // agy: input + output = total, thinking tách riêng → tính vào output (tính phí như output).
                output: n("output_tokens") + thinking,
                reasoning: thinking,
            },
        },
    )?;
    store.insert_turn(sid, &TurnSummary { ts_ms: now, role: Role::User, summary: summarize(question), byte_offset: now as u64 * 2 })?;
    store.insert_turn(sid, &TurnSummary { ts_ms: now, role: Role::Assistant, summary: summarize(&response), byte_offset: now as u64 * 2 + 1 })?;
    Ok(response)
}

/// Chạy agy một lần (chỉ đọc, sandbox) trong thư mục tạm ngoài repo.
pub fn run_agy(question: &str, model: Option<&str>, timeout_s: u64) -> Result<Value> {
    let dir = std::env::temp_dir().join("agmag-ask");
    std::fs::create_dir_all(&dir)?;
    let mut cmd = Command::new(std::env::var("AGM_AGY_BIN").unwrap_or_else(|_| "agy".into()));
    cmd.current_dir(&dir)
        .args(["--mode", "plan", "--sandbox", "--output-format", "json"])
        .arg("--print-timeout")
        .arg(format!("{timeout_s}s"));
    if let Some(m) = model {
        cmd.args(["--model", m]);
    }
    let out = cmd.arg("-p").arg(question).output().context("chạy agy")?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).with_context(|| format!("đầu ra agy không phải JSON: {}", text.chars().take(300).collect::<String>()))
}
