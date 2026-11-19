//! Can thiệp có ghi lại: gửi/trả lời tin, sinh agent có hạn mức, kill/interrupt có bảo vệ.
//! Quy tắc của người dùng (2026-11-18): Codex và Claude chỉ bị kill/ngắt khi **người dùng** ra lệnh;
//! agent (vd Codex quản lý) chỉ được kill/ngắt thợ không phải Codex/Claude (thực tế là OpenCode).

use anyhow::{bail, Result};

use crate::config::Config;
use crate::hc::{terminal_row_from, Hc, RunSpec};
use crate::launch::LaunchSpec;
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

/// Các app được coi là agent (tự báo 7501 hoặc nhận ra từ argv).
const AGENT_APPS: [&str; 4] = ["claude-code", "codex", "opencode", "antigravity"];
/// Agent và hành động tự động chỉ được dừng những app này (danh sách trắng).
const AGENT_KILLABLE_APPS: [&str; 1] = ["opencode"];

/// App hiện tại của terminal: ưu tiên `hc get` (mới nhất), rồi CSDL. None nếu không biết.
fn current_app(store: &Store, hc: &dyn Hc, terminal: &str) -> Option<String> {
    let fresh = hc.get(terminal).ok().map(|v| terminal_row_from(&v)).and_then(|r| r.app);
    fresh.or_else(|| store.terminal(terminal).ok().flatten().and_then(|t| t.app))
}

/// Xác định ai đang gọi — không chắc thì coi là agent (fail-closed):
/// có `HC_TERMINAL_ID` mà `hc get` lỗi, hoặc terminal gọi đang chạy một app agent → agent.
pub fn detect_caller(store: &Store, hc: &dyn Hc, hc_terminal: Option<&str>, tty: bool, confirmed: bool) -> Caller {
    let is_agent = match hc_terminal {
        None => false,
        Some(t) => match hc.get(t) {
            Err(_) => true,
            Ok(v) => {
                let row = terminal_row_from(&v);
                let app = row.app.or_else(|| store.terminal(t).ok().flatten().and_then(|r| r.app));
                row.reported || app.as_deref().is_some_and(|a| AGENT_APPS.contains(&a))
            }
        },
    };
    classify_caller(hc_terminal, is_agent, tty, confirmed)
}

/// Đóng một mục hộp thư mà không gõ gì vào terminal nào.
pub fn dismiss(store: &Store, id: i64, now: i64) -> Result<()> {
    if store.inbox_item(id)?.is_none() {
        bail!("không có mục hộp thư #{id}");
    }
    store.resolve_inbox(id, None, now)
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

#[allow(clippy::too_many_arguments)]
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
    // Chỉ trả lời được câu hỏi/chặn của agent hoặc tin agent gửi cho người dùng; các mục khác chỉ để đóng.
    if !matches!(item.source.as_str(), "blocked" | "message") {
        bail!("mục #{inbox_id} ({}) không phải câu hỏi của agent — dùng `agm done {inbox_id}` để đóng", item.source);
    }
    if item.resolved_ms.is_some() {
        bail!("mục #{inbox_id} đã được xử lý");
    }
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

/// Mọi điều kiện để `spawn` thành công, kiểm trước (không gọi hc). `replacing`: terminal sắp được thay
/// (xoay vòng) — không tính vào max_workers. Trả tên terminal sẽ dùng.
pub fn spawn_precheck(store: &Store, cfg: &Config, req: &SpawnReq, replacing: Option<&str>) -> Result<String> {
    if !valid_name(&req.task) {
        bail!("mã việc '{}' không hợp lệ: chỉ a-z 0-9 - _ (tối đa 32)", req.task);
    }
    if req.template.trim().is_empty() {
        bail!("thiếu template");
    }
    if req.role == "worker" {
        let workers = store
            .terminals(true)?
            .iter()
            .filter(|t| t.role.as_deref() == Some("worker") && Some(t.address.as_str()) != replacing)
            .count();
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
        bail!("tên terminal '{name}' không hợp lệ (1–32 ký tự a-z 0-9 - _); dùng mã việc ngắn hơn hoặc --name");
    }
    Ok(name)
}

/// Kiểm tra điều kiện mở phiên kế nhiệm cho Claude/Codex (không đòi hỏi `task` hay `template` như OpenCode worker).
pub fn spawn_successor_precheck(
    store: &Store,
    task: Option<&str>,
    old_name: Option<&str>,
    old_addr: &str,
    rot_id: i64,
    spec: &LaunchSpec,
) -> Result<String> {
    if !std::path::Path::new(&spec.cwd).is_dir() {
        bail!("thư mục làm việc '{}' không còn tồn tại", spec.cwd);
    }
    if let Some(task_id) = task
        && let Some(reason) = crate::policy::spawn_blocked_by_budget(store, task_id)?
    {
        bail!("{reason}");
    }
    let live_names: std::collections::HashSet<String> = store
        .terminals(true)?
        .into_iter()
        .filter_map(|t| t.name)
        .collect();
    let app_tag = if spec.app == "claude-code" { "claude" } else { "codex" };
    let base_raw = old_name
        .filter(|n| !n.is_empty())
        .map(|n| n.split("-r").next().unwrap_or(n))
        .unwrap_or(old_addr);
    let sanitized: String = base_raw
        .bytes()
        .map(|b| if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' { b as char } else { '-' })
        .collect();
    let prefix = if sanitized.is_empty() { format!("rot-{app_tag}") } else { sanitized };
    for n in 1..=99u32 {
        let suffix = format!("-r{n}");
        let max_prefix = 32usize.saturating_sub(suffix.len());
        let p: String = prefix.chars().take(max_prefix).collect();
        let candidate = format!("{p}{suffix}");
        if valid_name(&candidate) && !live_names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    let fallback = format!("rot-{app_tag}-{rot_id}");
    if valid_name(&fallback) && !live_names.contains(&fallback) {
        return Ok(fallback);
    }
    bail!("không tạo được tên terminal kế nhiệm duy nhất")
}

/// Sinh agent qua `hc run --bg --outlive-parent` sau khi kiểm hạn mức. Trả địa chỉ terminal mới.
pub fn spawn(store: &Store, hc: &dyn Hc, cfg: &Config, req: &SpawnReq, now: i64) -> Result<String> {
    let name = spawn_precheck(store, cfg, req, None)?;
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
/// Agent/tự động: **danh sách trắng** — chỉ app OpenCode đã xác định; không rõ app → từ chối.
fn authorize(store: &Store, hc: &dyn Hc, caller: &Caller, target: &str, verb: &str, now: i64) -> Result<()> {
    let app = current_app(store, hc, target);
    let shown = app.clone().unwrap_or_else(|| "không rõ app".into());
    let who = caller_name(caller);
    let refusal = match caller {
        Caller::User { confirmed: false } => Some("cần xác nhận (trả lời y)".to_string()),
        Caller::User { confirmed: true } => None,
        Caller::Agent(_) if app.as_deref().is_some_and(|a| AGENT_KILLABLE_APPS.contains(&a)) => None,
        Caller::Agent(_) if app.as_deref().is_some_and(|a| PROTECTED_APPS.contains(&a)) => {
            Some(format!("chỉ người dùng được {verb} phiên {shown}"))
        }
        Caller::Agent(_) => Some(format!("chỉ người dùng được {verb} {target} ({shown}); agent chỉ dừng được thợ OpenCode")),
    };
    if let Some(reason) = refusal {
        store.log_action(&who, verb, target, &format!("từ chối: {reason}"), false, now)?;
        if matches!(caller, Caller::Agent(_)) {
            store.open_inbox(Some(target), "request", Some(verb), &format!("{who} đề nghị {verb} {target} ({shown})"), now)?;
        }
        bail!("{reason}");
    }
    Ok(())
}

/// Bật `managed=1` + lưu `LaunchSpec` sau khi kiểm tra nghiêm ngặt người gọi (chỉ User đã xác nhận `y`).
pub fn set_managed_launch(store: &Store, caller: &Caller, target: &str, spec: &LaunchSpec, now: i64) -> Result<()> {
    let who = caller_name(caller);
    let detail = format!("exe={}, args={:?}, cwd={}", spec.executable, spec.argv, spec.cwd);
    match caller {
        Caller::User { confirmed: true } => {
            let json = spec.to_json()?;
            store.set_terminal_launch(target, true, Some(&json))?;
            store.log_action(&who, "manage", target, &detail, true, now)?;
            Ok(())
        }
        Caller::User { confirmed: false } => {
            store.log_action(&who, "manage", target, "chưa xác nhận", false, now)?;
            bail!("chưa xác nhận bật quản lý cho {target}");
        }
        Caller::Agent(_) => {
            let reason = format!("chỉ người dùng ở terminal mới được bật quản lý `agm manage` cho {target}");
            store.log_action(&who, "manage", target, &format!("từ chối: {reason}"), false, now)?;
            store.open_inbox(
                Some(target),
                "request",
                Some("manage"),
                &format!("{who} đề nghị bật quản lý {target} ({detail})"),
                now,
            )?;
            bail!("{reason}");
        }
    }
}

pub fn kill(store: &Store, hc: &dyn Hc, caller: &Caller, target: &str, now: i64) -> Result<()> {
    authorize(store, hc, caller, target, "kill", now)?;
    let res = hc.kill(target);
    store.log_action(&caller_name(caller), "kill", target, "", res.is_ok(), now)?;
    res
}

pub fn interrupt(store: &Store, hc: &dyn Hc, caller: &Caller, target: &str, now: i64) -> Result<()> {
    authorize(store, hc, caller, target, "interrupt", now)?;
    let res = hc.interrupt(target);
    store.log_action(&caller_name(caller), "interrupt", target, "", res.is_ok(), now)?;
    res
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CleanupCandidate {
    pub rotation_id: i64,
    pub old_terminal: String,
    pub old_name: Option<String>,
    pub old_app: Option<String>,
    pub old_live: bool,
    pub new_terminal: Option<String>,
    pub new_live: bool,
    pub handoff_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CleanupReport {
    pub done: usize,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

/// Danh sách phiên nguồn đã `handed_off` đủ điều kiện dọn thủ công:
/// 1) `old live` + phiên kế nhiệm đã `linked` & `live`, hoặc 2) `old` đã thoát (`exited`).
pub fn eligible_cleanups(store: &Store) -> Result<Vec<CleanupCandidate>> {
    let mut stmt = store.conn().prepare(
        "SELECT r.id, o.address, o.name, o.app, o.state, n.address, n.state, n.session, r.handoff_path
         FROM rotations r
         JOIN terminals o ON o.address = r.old_terminal
         LEFT JOIN terminals n ON n.address = r.new_terminal
         WHERE r.state = 'done'
           AND o.handoff_state = 'handed_off'
           AND o.state != 'removed'
         ORDER BY r.id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let old_state: String = r.get(4)?;
            let new_state: Option<String> = r.get(6)?;
            let new_sess: Option<i64> = r.get(7)?;
            Ok((
                CleanupCandidate {
                    rotation_id: r.get(0)?,
                    old_terminal: r.get(1)?,
                    old_name: r.get(2)?,
                    old_app: r.get(3)?,
                    old_live: old_state == "live",
                    new_terminal: r.get(5)?,
                    new_live: new_state.as_deref() == Some("live") && new_sess.is_some(),
                    handoff_path: r.get(8)?,
                },
                new_state,
                new_sess,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|(c, _, _)| (!c.old_live) || (c.old_live && c.new_live))
        .map(|(c, _, _)| c)
        .collect())
}

/// Thực thi dọn dẹp cho danh sách đã đóng băng, chỉ chấp nhận `Caller::User { confirmed: true }`.
/// Kiểm tra lại từng mục sát lúc thao tác; luôn giữ lịch sử `sessions`/`usage`/`rotations` và tệp handoff.
pub fn execute_cleanup_batch(
    store: &Store,
    hc: &dyn Hc,
    caller: &Caller,
    items: &[CleanupCandidate],
    now: i64,
) -> Result<CleanupReport> {
    if !matches!(caller, Caller::User { confirmed: true }) {
        bail!("dọn dẹp phiên chỉ dành cho người dùng đã xác nhận");
    }
    let mut report = CleanupReport::default();
    for item in items {
        let Some(old_row) = store.terminal(&item.old_terminal)? else {
            report.skipped.push(format!("{}: không còn thấy terminal", item.old_terminal));
            continue;
        };
        if old_row.handoff_state != "handed_off" || old_row.state == "removed" {
            report.skipped.push(format!("{}: không còn ở trạng thái chờ dọn", item.old_terminal));
            continue;
        }
        if old_row.state == "live" {
            let new_ok = match item.new_terminal.as_deref().and_then(|a| store.terminal(a).ok().flatten()) {
                Some(nt) => nt.state == "live" && nt.session.is_some(),
                None => false,
            };
            if !new_ok {
                report.skipped.push(format!("{}: phiên kế nhiệm không còn live", item.old_terminal));
                continue;
            }
            if let Ok(v) = hc.get(&item.old_terminal) {
                let fresh = terminal_row_from(&v);
                if fresh.state != "live" {
                    let _ = store.finish_terminal(&item.old_terminal, fresh.exit_code, now);
                    let _ = store.remove_terminal(&item.old_terminal);
                    report.done += 1;
                    continue;
                }
                if let (Some(want_app), Some(got_app)) = (item.old_app.as_deref(), fresh.app.as_deref())
                    && want_app != got_app
                {
                    report.skipped.push(format!(
                        "{}: đang chạy '{got_app}' thay vì '{want_app}'",
                        item.old_terminal
                    ));
                    continue;
                }
            }
            match kill(store, hc, caller, &item.old_terminal, now) {
                Ok(()) => {
                    let _ = store.finish_terminal(&item.old_terminal, None, now);
                    let _ = store.remove_terminal(&item.old_terminal);
                    let _ = store.resolve_inbox_for_terminal(&item.old_terminal, "rotation", now);
                    let _ = store.resolve_inbox_for_terminal(&item.old_terminal, "context", now);
                    report.done += 1;
                }
                Err(e) => report.errors.push(format!("{}: {e:#}", item.old_terminal)),
            }
        } else {
            // Nhánh old exited: dọn bản ghi terminal và đóng inbox liên quan, không gọi hc.
            match store.remove_terminal(&item.old_terminal) {
                Ok(()) => {
                    let _ = store.resolve_inbox_for_terminal(&item.old_terminal, "rotation", now);
                    let _ = store.resolve_inbox_for_terminal(&item.old_terminal, "context", now);
                    let _ = store.log_action("user", "cleanup", &item.old_terminal, "old exited", true, now);
                    report.done += 1;
                }
                Err(e) => report.errors.push(format!("{}: {e:#}", item.old_terminal)),
            }
        }
    }
    Ok(report)
}
