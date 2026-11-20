//! Quét phiên tương tác của Antigravity CLI (`agy`) từ `~/.gemini/antigravity-cli`:
//! - `brain/<conversation_id>/.system_generated/logs/transcript.jsonl`: token từng lượt và tóm tắt.
//! - `conversation_summaries.db` (chỉ đọc): lấy `workspace_uris` làm `cwd` để ghép terminal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use rusqlite::{Connection, OpenFlags};

use crate::ask::ask_workdir;
use crate::correlate::norm_cwd;
use crate::ingest::{IngestStats, ingest_file, path_key};
use crate::model::Source;
use crate::store::Store;

/// Thư mục gốc mặc định của Antigravity CLI (ghi đè bằng `AGM_AGY_DIR`).
pub fn default_dir() -> PathBuf {
    std::env::var_os("AGM_AGY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".gemini")
                .join("antigravity-cli")
        })
}

/// Tìm các tệp `brain/<conv_id>/.system_generated/logs/transcript.jsonl` sửa đổi từ `since_ms`.
pub fn discover(root: &Path, since_ms: i64) -> Vec<(String, PathBuf)> {
    let brain = root.join("brain");
    let Ok(entries) = std::fs::read_dir(&brain) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(conv_id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let log = dir
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl");
        let Ok(meta) = log.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_millis()).ok());
        if modified.is_some_and(|ms| ms >= since_ms) {
            out.push((conv_id, log));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// Quét các phiên `agy` gần đây.
/// Bỏ qua phiên `agm ask` (đã ghi dạng `agy://...` hoặc chạy trong `ask_workdir()`) để tránh đếm trùng.
pub fn ingest_agy(store: &Store, root: &Path, since_ms: i64) -> Result<IngestStats> {
    let logs = discover(root, since_ms);
    let summaries = root.join("conversation_summaries.db");
    if logs.is_empty() && !summaries.exists() {
        return Ok(IngestStats::default());
    }
    let cwds = load_cwds(&summaries);
    // agy có thể ghi summary SAU dòng cuối của transcript: bổ sung cwd cho phiên đã quét mà transcript không
    // còn đổi (không lọt vào `discover` nữa), để vẫn ghép được với terminal.
    for (conv_id, cwd) in &cwds {
        store.conn().execute(
            // Chỉ phiên dạng tệp: phiên `agm ask` (`agy://…`) chạy trong thư mục tạm, không được thành ứng viên
            // ghép terminal theo cwd.
            "UPDATE sessions SET cwd = ?2
             WHERE source = 'agy' AND session_id = ?1 AND cwd IS NULL AND path NOT LIKE 'agy://%'",
            [conv_id, cwd],
        )?;
    }
    let ask_cwd = norm_cwd(&ask_workdir().to_string_lossy());
    let mut total = IngestStats::default();
    for (conv_id, path) in logs {
        let cwd = cwds.get(&conv_id).map(String::as_str);
        if cwd.is_some_and(|c| norm_cwd(c) == ask_cwd) || has_ask_session(store, &conv_id)? {
            continue;
        }
        let stats = match ingest_file(store, Source::Agy, &path) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("agm: bỏ qua {}: {err:#}", path.display());
                total.skipped += 1;
                continue;
            }
        };
        let sid = store.session_id_for_path(Source::Agy, &path_key(&path))?;
        store.set_session_identity(sid, &conv_id, cwd)?;
        if has_ask_session(store, &conv_id)? {
            let ask_path = format!("agy://{conv_id}");
            if let Ok(ask_sid) = store.conn().query_row(
                "SELECT id FROM sessions WHERE source = 'agy' AND path = ?1",
                [&ask_path],
                |r| r.get::<_, i64>(0),
            ) {
                store.dedupe_agy_ask_session(ask_sid, &conv_id)?;
            }
            continue;
        }
        total.files += stats.files;
        total.lines += stats.lines;
        total.events += stats.events;
        total.unknown += stats.unknown;
        total.bad += stats.bad;
    }
    Ok(total)
}

fn has_ask_session(store: &Store, conv_id: &str) -> Result<bool> {
    Ok(store.conn().query_row(
        // Chỉ phiên ask CÓ usage mới thay được transcript; ask thiếu `usage` thì transcript là nguồn duy nhất.
        "SELECT EXISTS(SELECT 1 FROM sessions s WHERE s.source = 'agy' AND s.session_id = ?1 AND s.path LIKE 'agy://%'
                       AND EXISTS (SELECT 1 FROM usage u WHERE u.session = s.id))",
        [conv_id],
        |r| r.get(0),
    )?)
}

fn load_cwds(db: &Path) -> HashMap<String, String> {
    match try_load_cwds(db) {
        Ok(map) => map,
        Err(err) => {
            eprintln!("agm: cảnh báo đọc {}: {err:#}", db.display());
            HashMap::new()
        }
    }
}

fn try_load_cwds(db: &Path) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    if !db.exists() {
        return Ok(out);
    }
    let src = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    src.busy_timeout(std::time::Duration::from_secs(2))?;
    let mut stmt =
        src.prepare("SELECT conversation_id, workspace_uris FROM conversation_summaries")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, uris) = row?;
        if let Some(cwd) = first_workspace_path(&uris) {
            out.insert(id, cwd);
        }
    }
    Ok(out)
}

fn first_workspace_path(workspace_uris: &str) -> Option<String> {
    let arr: Vec<String> = serde_json::from_str(workspace_uris).ok()?;
    arr.into_iter().find_map(|uri| file_uri_to_path(&uri))
}

pub fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest
        .strip_prefix("localhost")
        .filter(|s| s.starts_with('/'))
        .unwrap_or(rest);
    let decoded = percent_decode(rest);
    if decoded.is_empty() {
        return None;
    }
    if !decoded.starts_with('/') {
        // UNC: file://server/share/...
        return Some(if cfg!(windows) {
            format!(r"\\{}", decoded.replace('/', "\\"))
        } else {
            format!("//{decoded}")
        });
    }
    let trimmed = match decoded.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => &decoded[1..],
        _ => decoded.as_str(),
    };
    if trimmed.is_empty() {
        return None;
    }
    Some(if cfg!(windows) {
        trimmed.replace('/', "\\")
    } else {
        trimmed.to_string()
    })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(hi), Some(lo)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(hi << 4 | lo);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
