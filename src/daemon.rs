//! Tiến trình nền: theo dõi `hc events --json`, quét log định kỳ, chạy policy.

use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::Value;

use crate::hc::{app_from_argv, terminal_row_from};
use crate::store::Store;

/// Áp một dòng của `hc events --json` vào CSDL. Dòng lạ bị bỏ qua (định dạng sự kiện có thể thêm loại mới).
pub fn apply_event(store: &Store, line: &Value, now: i64) -> Result<()> {
    // Ảnh chụp đầu luồng (và sau `missed`): {channel, seq, terminals:[...]}.
    if let Some(terms) = line["terminals"].as_array() {
        for t in terms {
            apply_terminal(store, t, now)?;
        }
        return Ok(());
    }
    if line["method"] == "missed" {
        if let Some(terms) = line["params"]["terminals"].as_array() {
            for t in terms {
                apply_terminal(store, t, now)?;
            }
        }
        return Ok(());
    }
    if line["method"] != "event" {
        return Ok(());
    }
    let p = &line["params"];
    let addr = p["terminal"].as_str().unwrap_or_default();
    let data = &p["data"];
    match p["type"].as_str().unwrap_or_default() {
        "created" | "restarted" => apply_terminal(store, data, now)?,
        "status" => apply_status(store, addr, &data["status"], data["turn"].as_i64(), now)?,
        "turn" => store.set_terminal_turn(addr, data["turn"].as_i64().unwrap_or(0), data["state"].as_str(), now)?,
        "finished" | "lost" => {
            store.finish_terminal(addr, data["exit_code"].as_i64(), data["finished_ms"].as_i64().unwrap_or(now))?;
            store.resolve_inbox_for_terminal(addr, "blocked", now)?;
        }
        "removed" => store.remove_terminal(addr)?,
        "labels" => {
            let labels: BTreeMap<String, String> = data["labels"]
                .as_object()
                .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
                .unwrap_or_default();
            store.set_terminal_labels(addr, &labels)?;
        }
        "foreground" => {
            if let Some(t) = store.terminal(addr)? {
                if !t.reported {
                    let argv: Vec<String> = data["argv"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                    if let Some(app) = app_from_argv(&argv) {
                        store.set_terminal_app(addr, &app)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn apply_terminal(store: &Store, t: &Value, now: i64) -> Result<()> {
    let row = terminal_row_from(t);
    if row.address.is_empty() {
        return Ok(());
    }
    store.upsert_terminal(&row)?;
    if !t["status"].is_null() {
        apply_status(store, &row.address, &t["status"], t["turn"].as_i64(), now)?;
    }
    Ok(())
}

/// `status` là đối tượng 7501 ({app, state, kind, msg}) hoặc chuỗi (dạng `hc ls`).
fn apply_status(store: &Store, addr: &str, status: &Value, turn: Option<i64>, now: i64) -> Result<()> {
    let state = status["state"].as_str().or_else(|| status.as_str());
    let kind = status["kind"].as_str();
    let msg = status["msg"].as_str();
    let current_turn = store.terminal(addr)?.map_or(0, |t| t.turn);
    store.set_terminal_status(addr, state, kind, msg, turn.unwrap_or(current_turn), now)?;
    let app = status["app"].as_str();
    if let Some(app) = app {
        store.set_terminal_app(addr, app)?;
    }
    if state == Some("blocked") {
        let text = match msg {
            Some(m) if !m.is_empty() => m.to_string(),
            _ => format!("{} đang chờ: {}", app.unwrap_or("agent"), kind.unwrap_or("không rõ")),
        };
        store.open_inbox(Some(addr), "blocked", kind, &text, now)?;
    } else if state.is_some() {
        store.resolve_inbox_for_terminal(addr, "blocked", now)?;
    }
    Ok(())
}
