//! `agm hook statusline`: Claude Code gọi lệnh statusline với JSON phiên trên stdin.
//! agmag dùng nó để ghi hạn mức gói Claude (5h/7d), kích thước cửa sổ ngữ cảnh, và ghép terminal ↔ phiên.
//! Tài liệu trường: https://code.claude.com/docs/en/statusline (đã kiểm ở P0).

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::ingest::path_key;
use crate::model::{ContextSnapshot, QuotaSnapshot, QuotaWindow, Source};
use crate::store::{Store, TerminalRow};

/// Ghi số liệu từ JSON statusline và trả về một dòng trạng thái ngắn để Claude Code hiển thị.
pub fn statusline(store: &Store, input: &Value, hc_terminal: Option<&str>, now_ms: i64) -> Result<String> {
    let mut windows = Vec::new();
    for (key, minutes) in [("five_hour", 300u32), ("seven_day", 10080u32)] {
        let w = &input["rate_limits"][key];
        if let (Some(pct), Some(reset)) = (w["used_percentage"].as_f64(), w["resets_at"].as_i64()) {
            windows.push(QuotaWindow { window_minutes: minutes, used_percent: pct, resets_at_s: reset });
        }
    }
    if !windows.is_empty() {
        store.insert_quota(&QuotaSnapshot { provider: "claude".into(), ts_ms: now_ms, windows: windows.clone() })?;
    }

    let ctx = &input["context_window"];
    let pct = ctx["used_percentage"].as_f64();
    if let Some(transcript) = input["transcript_path"].as_str().filter(|s| !s.is_empty()) {
        let sid = store.session_id_for_path(Source::Claude, &path_key(Path::new(transcript)))?;
        if let Some(size) = ctx["context_window_size"].as_u64() {
            store.set_context_window(sid, size)?;
            if let Some(p) = pct {
                let used = (p / 100.0 * size as f64).round() as u64;
                store.set_context(sid, &ContextSnapshot { ts_ms: now_ms, used_tokens: used, window_tokens: Some(size) })?;
            }
        }
        if let Some(addr) = hc_terminal.filter(|a| !a.is_empty()) {
            if store.terminal(addr)?.is_none() {
                store.upsert_terminal(&TerminalRow { address: addr.into(), state: "live".into(), ..Default::default() })?;
            }
            store.set_terminal_app(addr, "claude-code")?;
            store.link_terminal(addr, sid, "hook")?;
        }
    }

    let mut parts = vec![input["model"]["display_name"].as_str().unwrap_or("Claude").to_string()];
    if let Some(p) = pct {
        parts.push(format!("ctx {:.0}%", p));
    }
    for w in &windows {
        let label = if w.window_minutes == 300 { "5h" } else { "7d" };
        // Hạn mức gói: hiển thị phần còn lại (ctx vẫn là phần đã dùng).
        parts.push(format!("{label} {:.0}% left", (100.0 - w.used_percent).clamp(0.0, 100.0)));
    }
    Ok(parts.join(" · "))
}
