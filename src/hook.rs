//! `agm hook statusline`: Claude Code gọi lệnh statusline với JSON phiên trên stdin.
//! agmag dùng nó để ghi hạn mức gói Claude (5h/7d), kích thước cửa sổ ngữ cảnh, và ghép terminal ↔ phiên.
//! Tài liệu trường: https://code.claude.com/docs/en/statusline (đã kiểm ở P0).

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::ingest::path_key;
use crate::model::{ContextSnapshot, QuotaSnapshot, QuotaWindow, Source};
use crate::store::{Store, TerminalRow};

/// Kết quả của `agm hook install`.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    Added,
    /// statusLine hiện có đã gọi `hook statusline` (của agm, kể cả cài ở đường dẫn khác) → không làm gì.
    AlreadyInstalled,
}

/// Thêm statusLine của agm vào `settings_path` (Claude Code `settings.json`).
/// Chạy lại là an toàn; statusLine **của người khác** thì không bao giờ ghi đè (lỗi, tệp giữ nguyên).
pub fn install_statusline(settings_path: &Path, agm_exe: &Path) -> Result<InstallOutcome> {
    let path = settings_path;
    let mut v: Value = match std::fs::read_to_string(path) {
        Ok(t) => serde_json::from_str(&t)
            .with_context(|| format!("{} không phải JSON hợp lệ", path.display()))?,
        // Chỉ "chưa có tệp" mới bắt đầu từ rỗng; lỗi khác (bị khóa, không có quyền) → dừng, không ghi đè cấu hình.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e).with_context(|| format!("đọc {}", path.display())),
    };
    if !v.is_object() {
        bail!("{} không phải một đối tượng JSON", path.display());
    }
    if !v["statusLine"].is_null() {
        // Cùng tiêu chí với `launch::check_claude_hook_installed`.
        let cmd = v["statusLine"]["command"]
            .as_str()
            .or_else(|| v["statusLine"].as_str())
            .unwrap_or_default();
        if cmd.contains("hook statusline") {
            return Ok(InstallOutcome::AlreadyInstalled);
        }
        bail!(
            "{} đã có statusLine; agmag không ghi đè. Gọi `agm hook statusline` từ script hiện có của bạn.",
            path.display()
        );
    }
    v["statusLine"] =
        json!({"type": "command", "command": format!("\"{}\" hook statusline", agm_exe.display())});
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(InstallOutcome::Added)
}

/// Ghi số liệu từ JSON statusline và trả về một dòng trạng thái ngắn để Claude Code hiển thị.
pub fn statusline(
    store: &Store,
    input: &Value,
    hc_terminal: Option<&str>,
    now_ms: i64,
) -> Result<String> {
    let mut windows = Vec::new();
    for (key, minutes) in [("five_hour", 300u32), ("seven_day", 10080u32)] {
        let w = &input["rate_limits"][key];
        if let (Some(pct), Some(reset)) = (w["used_percentage"].as_f64(), w["resets_at"].as_i64()) {
            windows.push(QuotaWindow {
                window_minutes: minutes,
                used_percent: pct,
                resets_at_s: reset,
            });
        }
    }
    if !windows.is_empty() {
        store.insert_quota(&QuotaSnapshot {
            provider: "claude".into(),
            ts_ms: now_ms,
            windows: windows.clone(),
        })?;
    }

    let ctx = &input["context_window"];
    let pct = ctx["used_percentage"].as_f64();
    if let Some(transcript) = input["transcript_path"].as_str().filter(|s| !s.is_empty()) {
        let sid = store.session_id_for_path(Source::Claude, &path_key(Path::new(transcript)))?;
        if let Some(size) = ctx["context_window_size"].as_u64() {
            store.set_context_window(sid, size)?;
            if let Some(p) = pct {
                let used = (p / 100.0 * size as f64).round() as u64;
                store.set_context(
                    sid,
                    &ContextSnapshot {
                        ts_ms: now_ms,
                        used_tokens: used,
                        window_tokens: Some(size),
                    },
                )?;
            }
        }
        if let Some(addr) = hc_terminal.filter(|a| !a.is_empty()) {
            if store.terminal(addr)?.is_none() {
                store.upsert_terminal(&TerminalRow {
                    address: addr.into(),
                    state: "live".into(),
                    ..Default::default()
                })?;
            }
            store.set_terminal_app(addr, "claude-code")?;
            store.link_terminal(addr, sid, "hook")?;
        }
    }

    let mut parts = vec![
        input["model"]["display_name"]
            .as_str()
            .unwrap_or("Claude")
            .to_string(),
    ];
    if let Some(p) = pct {
        parts.push(format!("ctx {:.0}%", p));
    }
    for w in &windows {
        let label = if w.window_minutes == 300 { "5h" } else { "7d" };
        // Hạn mức gói: hiển thị phần còn lại (ctx vẫn là phần đã dùng).
        parts.push(format!(
            "{label} {:.0}% left",
            (100.0 - w.used_percent).clamp(0.0, 100.0)
        ));
    }
    Ok(parts.join(" · "))
}
