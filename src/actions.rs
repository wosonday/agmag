//! Can thiệp có ghi lại: gửi/trả lời tin, sinh agent có hạn mức, kill/interrupt có bảo vệ.
//! Quy tắc của người dùng (2026-11-18): Codex và Claude chỉ bị kill/ngắt khi **người dùng** ra lệnh;
//! agent (vd Codex quản lý) chỉ được kill/ngắt thợ không phải Codex/Claude (thực tế là OpenCode).

use anyhow::{bail, Result};

use crate::config::Config;
use crate::hc::{terminal_row_from, Hc, RunSpec};
use crate::store::{MessageRow, Store};

pub const KINDS: [&str; 7] = ["GIAO", "BÁO CÁO", "LỖI", "HỎI", "CHẶN", "ĐẠT", "CHƯA ĐẠT"];
const PROTECTED_APPS: [&str; 2] = ["codex", "claude-code"];

#[derive(Debug, Clone, PartialEq)]
pub enum Caller {
    /// Lệnh gõ từ terminal của người (có TTY, không phải agent); `confirmed` = đã trả lời y hoặc `--yes`.
    User { confirmed: bool },
    /// Lệnh do một agent (hoặc script không TTY) gọi; kèm terminal gọi nếu biết.
    Agent(Option<String>),
}

/// `hc_terminal`: `HC_TERMINAL_ID` của tiến trình gọi; `terminal_is_agent`: terminal đó đã tự báo 7501.
pub fn classify_caller(hc_terminal: Option<&str>, terminal_is_agent: bool, tty: bool, confirmed: bool) -> Caller {
    if tty && !terminal_is_agent {
        Caller::User { confirmed }
    } else {
        Caller::Agent(hc_terminal.map(String::from))
    }
}

fn caller_name(c: &Caller) -> String {
    match c {
        Caller::User { .. } => "user".into(),
        Caller::Agent(t) => t.clone().unwrap_or_else(|| "agent".into()),
    }
}

pub fn format_message(from: &str, kind: &str, task: Option<&str>, text: &str, to_user: bool) -> String {
    let id = task.map(|t| format!(" [ID {t}]")).unwrap_or_default();
    if to_user {
        format!("[{kind}]{id} {text}")
    } else {
        format!("[{kind}]{id} từ {from}: {text}")
    }
}

/// Gửi một tin theo mẫu §3.2. Đích `user` → hộp thư. Tin luôn được ghi lại, kể cả khi gửi thất bại.
pub fn send(store: &Store, hc: &dyn Hc, from: &str, to: &str, kind: &str, task: Option<&str>, text: &str, now: i64) -> Result<MessageRow> {
    if !KINDS.contains(&kind) {
        bail!("loại tin '{kind}' không hợp lệ; dùng một trong: {}", KINDS.join(", "));
    }
    let to_user = to == "user";
    let body = format_message(from, kind, task, text, to_user);
    let mut m = MessageRow {
        ts_ms: now,
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        task: task.map(String::from),
        text: text.into(),
        status: "queued".into(),
        ..Default::default()
    };
    m.id = store.add_message(&m)?;
    if to_user {
        let from_t = (from != "user").then_some(from);
        store.open_inbox(from_t, "message", Some(kind), &body, now)?;
        m.status = "inbox".into();
    } else {
        match hc.send(to, &body, true, false) {
            Ok(v) => {
                m.status = "sent".into();
                m.detail = v["turn"].as_i64().map(|t| format!("turn {t}"));
            }
            Err(e) => {
                m.status = "failed".into();
                m.detail = Some(format!("{e:#}"));
            }
        }
    }
    store.set_message_status(m.id, &m.status, m.detail.as_deref())?;
    Ok(m)
}

/// Trả lời một mục hộp thư bằng cách gõ vào terminal liên quan. Mục `blocked` dùng `force` (trả lời hộp thoại).
pub fn reply(store: &Store, hc: &dyn Hc, inbox_id: i64, text: &str, now: i64) -> Result<()> {
    let Some(item) = store.inbox_item(inbox_id)? else { bail!("không có mục hộp thư #{inbox_id}") };
    let Some(term) = item.terminal.as_deref() else { bail!("mục #{inbox_id} không gắn với terminal nào để trả lời") };
    let res = hc.send(term, text, false, item.source == "blocked");
    store.log_action("user", "reply", term, text, res.is_ok(), now)?;
    res?;
    store.resolve_inbox(inbox_id, Some(text), now)?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct SpawnReq {
    pub template: String,
    pub task: String,
    /// worker | audit | research | … (nhãn agm.role).
    pub role: String,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub text: Option<String>,
}

fn valid_name(s: &str) -> bool {
    (1..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Sinh agent qua `hc run --bg --outlive-parent` sau khi kiểm hạn mức. Trả địa chỉ terminal mới.
pub fn spawn(store: &Store, hc: &dyn Hc, cfg: &Config, req: &SpawnReq, now: i64) -> Result<String> {
    if !valid_name(&req.task) {
        bail!("mã việc '{}' không hợp lệ: chỉ a-z 0-9 - _ (tối đa 32)", req.task);
    }
    let live = store.terminals(true)?;
    if req.role == "worker" {
        let workers = live.iter().filter(|t| t.role.as_deref() == Some("worker")).count();
        if workers as u32 >= cfg.policy.max_workers {
            bail!("đã có {workers} thợ đang chạy (max_workers = {}); chờ một thợ xong hoặc tăng [policy].max_workers", cfg.policy.max_workers);
        }
    }
    if let Some(reason) = crate::policy::spawn_blocked_by_budget(store, &req.task)? {
        bail!("{reason}");
    }
    let name = match &req.name {
        Some(n) => n.clone(),
        None => {
            let prefix = format!("w-{}-", req.task);
            let used = store.terminals(false)?.iter().filter(|t| t.name.as_deref().is_some_and(|n| n.starts_with(&prefix))).count();
            format!("{prefix}{}", used + 1)
        }
    };
    if !valid_name(&name) {
        bail!("tên terminal '{name}' không hợp lệ (1–32 ký tự a-z 0-9 - _)");
    }
    let mut argv = vec![req.template.clone()];
    argv.extend(req.text.clone());
    let spec = RunSpec {
        argv,
        name: Some(name.clone()),
        cwd: req.cwd.clone(),
        labels: [("agm.task".to_string(), req.task.clone()), ("agm.role".to_string(), req.role.clone())].into(),
    };
    let res = hc.run(&spec);
    store.log_action("agm", "spawn", &name, &format!("{} task={} role={}", req.template, req.task, req.role), res.is_ok(), now)?;
    let v = res?;
    let mut row = terminal_row_from(&v);
    if row.address.is_empty() {
        bail!("hc run không trả địa chỉ terminal: {v}");
    }
    row.template.get_or_insert(req.template.clone());
    row.task.get_or_insert(req.task.clone());
    row.role.get_or_insert(req.role.clone());
    if row.created_ms == 0 {
        row.created_ms = now;
    }
    store.upsert_terminal(&row)?;
    Ok(row.address)
}

/// Kiểm quyền kill/ngắt theo quy tắc của người dùng; từ chối thì ghi nhật ký và chuyển đề nghị vào hộp thư.
fn authorize(store: &Store, caller: &Caller, target: &str, verb: &str, now: i64) -> Result<()> {
    let app = store.terminal(target)?.and_then(|t| t.app).unwrap_or_default();
    let protected = PROTECTED_APPS.contains(&app.as_str());
    let who = caller_name(caller);
    let refusal = match caller {
        Caller::User { confirmed: false } => Some("cần xác nhận (trả lời y hoặc dùng --yes)".to_string()),
        Caller::User { confirmed: true } => None,
        Caller::Agent(_) if protected => Some(format!("chỉ người dùng được {verb} phiên {app}")),
        Caller::Agent(_) => None,
    };
    if let Some(reason) = refusal {
        store.log_action(&who, verb, target, &format!("từ chối: {reason}"), false, now)?;
        if matches!(caller, Caller::Agent(_)) {
            store.open_inbox(Some(target), "request", Some(verb), &format!("{who} đề nghị {verb} {target} ({app})"), now)?;
        }
        bail!("{reason}");
    }
    Ok(())
}

pub fn kill(store: &Store, hc: &dyn Hc, caller: &Caller, target: &str, now: i64) -> Result<()> {
    authorize(store, caller, target, "kill", now)?;
    let res = hc.kill(target);
    store.log_action(&caller_name(caller), "kill", target, "", res.is_ok(), now)?;
    res
}

pub fn interrupt(store: &Store, hc: &dyn Hc, caller: &Caller, target: &str, now: i64) -> Result<()> {
    authorize(store, caller, target, "interrupt", now)?;
    let res = hc.interrupt(target);
    store.log_action(&caller_name(caller), "interrupt", target, "", res.is_ok(), now)?;
    res
}
