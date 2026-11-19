//! `agm ask`: gọi agy (Gemini) một lần ở chế độ chỉ đọc và ghi lại token.
//! Phiên agy tương tác được quét riêng từ `~/.gemini/antigravity-cli/brain/` (`src/agy.rs`).

use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::model::{summarize, Role, SessionMeta, Source, TurnSummary, Usage, UsageRecord};
use crate::store::Store;

/// Ghi kết quả `agy --output-format json` vào CSDL; trả câu trả lời.
pub fn record(store: &Store, out: &Value, question: &str, model: Option<&str>, now: i64) -> Result<String> {
    let ok = out["status"] == "SUCCESS";
    let response = out["response"].as_str().unwrap_or_default().to_string();
    let conv = out["conversation_id"].as_str().map(String::from).unwrap_or_else(|| format!("ask-{now}"));
    let sid = store.session_id_for_path(Source::Agy, &format!("agy://{conv}"))?;
    store.set_meta(sid, &SessionMeta { session_id: conv.clone(), cwd: None, cli_version: None, started_ms: Some(now) })?;
    // Nếu daemon/TUI vừa quét transcript của chính lượt ask này thành phiên dạng tệp thì xóa bản trùng.
    store.dedupe_agy_ask_session(sid, &conv)?;
    // Model: theo tham số hoặc đầu ra agy; không biết thì để trống (không bịa tên).
    let model = model.map(String::from).or_else(|| out["model"].as_str().map(String::from));
    if let Some(m) = &model {
        store.set_model(sid, m)?;
    }
    let u = &out["usage"];
    if !u.is_object() {
        if !ok {
            bail!("agy trả trạng thái {}: {}", out["status"], response);
        }
        // Không có số liệu token → không ghi bản ghi 0.
        store.insert_turn(sid, &TurnSummary { ts_ms: now, role: Role::User, summary: summarize(question), byte_offset: now as u64 * 2 })?;
        return Ok(response);
    }
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let cache = n("cache_read_tokens");
    let thinking = n("thinking_tokens");
    store.upsert_usage(
        sid,
        &UsageRecord {
            dedupe_key: format!("{conv}:{now}"),
            ts_ms: now,
            model: model.clone(),
            usage: Usage {
                input_uncached: n("input_tokens").saturating_sub(cache),
                cache_read: cache,
                cache_write: 0,
                // Đã kiểm thực tế với `agy --output-format json`: `output_tokens` đã gồm `thinking_tokens`
                // (`input_tokens + output_tokens == total_tokens`). Giữ `reasoning ⊂ output` (config.rs).
                output: n("output_tokens").max(thinking),
                reasoning: thinking,
            },
        },
    )?;
    store.insert_turn(sid, &TurnSummary { ts_ms: now, role: Role::User, summary: summarize(question), byte_offset: now as u64 * 2 })?;
    // Token đã tiêu được ghi kể cả khi agy lỗi/hết giờ; sau đó mới báo lỗi.
    if !ok {
        bail!("agy trả trạng thái {}: {}", out["status"], response);
    }
    store.insert_turn(sid, &TurnSummary { ts_ms: now, role: Role::Assistant, summary: summarize(&response), byte_offset: now as u64 * 2 + 1 })?;
    Ok(response)
}

/// Thư mục làm việc tạm cho `agm ask` (dùng chung để `agy::ingest_agy` lọc bỏ phiên one-shot).
pub fn ask_workdir() -> std::path::PathBuf {
    std::env::temp_dir().join("agmag-ask")
}

/// Chạy agy một lần (chỉ đọc, sandbox) trong thư mục tạm ngoài repo.
pub fn run_agy(question: &str, model: Option<&str>, timeout_s: u64) -> Result<Value> {
    let dir = ask_workdir();
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
    serde_json::from_str(text.trim()).with_context(|| {
        format!(
            "agy ({}) không trả JSON: {} {}",
            out.status,
            text.chars().take(300).collect::<String>(),
            String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>()
        )
    })
}
