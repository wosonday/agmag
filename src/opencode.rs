//! Đọc phiên OpenCode v2 từ `opencode.db` (SQLite) — chỉ đọc, chỉ hai bảng `session_v2`, `session_message`.
//! Không bao giờ chạm các bảng chứa bí mật (`credential`, `account`, …).

use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;

use crate::ingest::IngestStats;
use crate::model::{ContextSnapshot, Role, SessionMeta, Source, TurnSummary, Usage, UsageRecord, summarize};
use crate::store::{Cursor, Store};

/// Vị trí mặc định của CSDL OpenCode v2 (ghi đè bằng `AGM_OPENCODE_DB`).
pub fn default_db() -> PathBuf {
    std::env::var_os("AGM_OPENCODE_DB").map(PathBuf::from).unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_default()
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db")
    })
}

struct Msg {
    id: String,
    session: String,
    kind: String,
    seq: i64,
    updated: i64,
    data: String,
}

/// Quét các message OpenCode mới hơn con trỏ. Tệp không tồn tại → không làm gì.
pub fn ingest_opencode(store: &Store, db: &Path) -> Result<IngestStats> {
    let mut stats = IngestStats::default();
    if !db.exists() {
        return Ok(stats);
    }
    let src = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    src.busy_timeout(std::time::Duration::from_secs(5))?;
    let key = format!("opencode-db:{}", db.to_string_lossy());
    let since = store.cursor(&key)?.map_or(0, |c| c.offset as i64);

    let msgs: Vec<Msg> = src
        .prepare(
            "SELECT id, session_id, type, seq, time_updated, data FROM session_message
             WHERE time_updated > ?1 ORDER BY time_updated, seq",
        )?
        .query_map(params![since], |r| {
            Ok(Msg { id: r.get(0)?, session: r.get(1)?, kind: r.get(2)?, seq: r.get(3)?, updated: r.get(4)?, data: r.get(5)? })
        })?
        .collect::<rusqlite::Result<_>>()?;

    stats.files = 1;
    let tx = store.conn().unchecked_transaction()?;
    let mut max_updated = since;
    for m in &msgs {
        max_updated = max_updated.max(m.updated);
        stats.lines += 1;
        let sid = store.session_id_for_path(Source::OpenCode, &format!("opencode://{}", m.session))?;
        if let Some(meta) = session_meta(&src, &m.session)? {
            store.set_meta(sid, &meta)?;
        }
        let data: Value = match serde_json::from_str(&m.data) {
            Ok(v) => v,
            Err(_) => {
                stats.bad += 1;
                store.add_line_stats(sid, 0, 1)?;
                continue;
            }
        };
        let ts = data["time"]["created"].as_i64().unwrap_or(m.updated);
        match m.kind.as_str() {
            "assistant" => {
                if let Some(model) = data["model"]["id"].as_str() {
                    store.set_model(sid, model)?;
                }
                let tok = &data["tokens"];
                if tok.is_object() {
                    let n = |v: &Value| v.as_u64().unwrap_or(0);
                    let usage = Usage {
                        input_uncached: n(&tok["input"]),
                        cache_read: n(&tok["cache"]["read"]),
                        cache_write: n(&tok["cache"]["write"]),
                        output: n(&tok["output"]),
                        reasoning: n(&tok["reasoning"]),
                    };
                    let used = usage.input_uncached + usage.cache_read + usage.cache_write;
                    store.upsert_usage(
                        sid,
                        &UsageRecord {
                            dedupe_key: m.id.clone(),
                            ts_ms: ts,
                            model: data["model"]["id"].as_str().map(String::from),
                            usage,
                        },
                    )?;
                    if used > 0 {
                        store.set_context(sid, &ContextSnapshot { ts_ms: ts, used_tokens: used, window_tokens: None })?;
                    }
                    stats.events += 1;
                }
                if let Some(text) = assistant_summary(&data) {
                    store.insert_turn(sid, &TurnSummary { ts_ms: ts, role: Role::Assistant, summary: text, byte_offset: m.seq as u64 })?;
                }
            }
            "user" => {
                if let Some(text) = data["text"].as_str().map(summarize).filter(|t| !t.is_empty()) {
                    store.insert_turn(sid, &TurnSummary { ts_ms: ts, role: Role::User, summary: text, byte_offset: m.seq as u64 })?;
                }
            }
            _ => {}
        }
    }
    store.save_cursor(&Cursor { path: key, offset: max_updated.max(0) as u64, size: 0, mtime_ms: 0, head_hash: 0 })?;
    tx.commit()?;
    Ok(stats)
}

fn session_meta(src: &Connection, id: &str) -> Result<Option<SessionMeta>> {
    let row = src
        .query_row(
            "SELECT directory, version, time_created FROM session_v2 WHERE id = ?1",
            params![id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
        )
        .ok();
    Ok(row.map(|(dir, version, created)| SessionMeta {
        session_id: id.to_string(),
        cwd: Some(dir),
        cli_version: Some(version),
        started_ms: Some(created),
    }))
}

/// Khối văn bản đầu tiên không rỗng; nếu không có thì tên công cụ đầu tiên. Lượt chỉ có suy nghĩ → None.
fn assistant_summary(data: &Value) -> Option<String> {
    let parts = data["content"].as_array()?;
    parts
        .iter()
        .filter(|p| p["type"] == "text")
        .filter_map(|p| p["text"].as_str())
        .map(summarize)
        .find(|t| !t.is_empty())
        .or_else(|| {
            parts
                .iter()
                .find_map(|p| p["tool"].as_str().or_else(|| p["name"].as_str()))
                .map(|name| format!("[tool] {name}"))
        })
}
