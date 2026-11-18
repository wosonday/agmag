//! Ghép terminal của herdr-core với phiên log (spec §5.3).
//! Thứ tự tin cậy: hook statusline / ghép tay (không bao giờ ghi đè) → cùng app + cùng cwd + bắt đầu gần lúc tạo terminal.
//! Chỉ ghép khi khớp **một–một hai chiều**: phiên là ứng viên duy nhất của terminal VÀ terminal là ứng viên duy nhất
//! của phiên. Phiên đã gắn với bất kỳ terminal nào (kể cả đã kết thúc) không bị ghép lại. Còn mơ hồ → `agm link`.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::params;

use crate::store::Store;

/// Phiên được coi là của terminal nếu bắt đầu trong khoảng [tạo − 1 phút, tạo + 10 phút].
const BEFORE_MS: i64 = 60_000;
const AFTER_MS: i64 = 600_000;

pub fn norm_cwd(p: &str) -> String {
    let s = p.replace('/', "\\");
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let s = s.trim_end_matches('\\');
    if cfg!(windows) { s.to_lowercase() } else { s.to_string() }
}

fn source_for_app(app: &str) -> Option<&'static str> {
    match app {
        "codex" => Some("codex"),
        "claude-code" => Some("claude"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

/// Ghép mọi terminal đang sống chưa có phiên. Trả số liên kết mới.
pub fn link_all(store: &Store) -> Result<usize> {
    // terminal → danh sách phiên ứng viên
    let mut cands: Vec<(String, Vec<i64>)> = Vec::new();
    for t in store.terminals(true)? {
        if t.session.is_some() {
            continue;
        }
        let (Some(app), Some(cwd)) = (t.app.as_deref(), t.cwd.as_deref()) else { continue };
        let Some(source) = source_for_app(app) else { continue };
        let want = norm_cwd(cwd);
        let mut stmt = store.conn().prepare(
            "SELECT s.id, s.cwd FROM sessions s
             WHERE s.source = ?1 AND s.started_ms BETWEEN ?2 AND ?3
               AND NOT EXISTS (SELECT 1 FROM terminals x WHERE x.session = s.id)",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![source, t.created_ms - BEFORE_MS, t.created_ms + AFTER_MS], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
            })?
            .filter_map(|r| r.ok())
            .filter(|(_, c)| c.as_deref().map(norm_cwd).as_deref() == Some(want.as_str()))
            .map(|(id, _)| id)
            .collect();
        cands.push((t.address, ids));
    }
    // Mỗi phiên là ứng viên của bao nhiêu terminal.
    let mut claims: HashMap<i64, usize> = HashMap::new();
    for (_, ids) in &cands {
        for id in ids {
            *claims.entry(*id).or_default() += 1;
        }
    }
    let mut linked = 0;
    for (addr, ids) in &cands {
        if let [only] = ids.as_slice()
            && claims.get(only) == Some(&1)
        {
            store.link_terminal(addr, *only, "auto")?;
            linked += 1;
        }
    }
    Ok(linked)
}
