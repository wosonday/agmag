//! Can thiệp có ghi lại: gửi/trả lời tin, sinh agent có hạn mức, kill/interrupt có bảo vệ.
//! Quy tắc của người dùng (2026-11-18): Codex và Claude chỉ bị kill/ngắt khi **người dùng** ra lệnh;
//! agent (vd Codex quản lý) chỉ được kill/ngắt thợ không phải Codex/Claude (thực tế là OpenCode).

use anyhow::{Result, bail};

use crate::config::Config;
use crate::hc::{Hc, RunSpec, terminal_row_from};
use crate::launch::LaunchSpec;
use crate::store::{MessageRow, Store};

pub const KINDS: [&str; 7] = ["GIAO", "BÁO CÁO", "LỖI", "HỎI", "CHẶN", "ĐẠT", "CHƯA ĐẠT"];

/// English message types shown in `agm tui`, paired with the stored code (SQLite, forwarded text, `agm send --type`).
pub const KINDS_EN: [(&str, &str); 7] = [
    ("ASSIGN", "GIAO"),
    ("REPORT", "BÁO CÁO"),
    ("ERROR", "LỖI"),
    ("QUESTION", "HỎI"),
    ("BLOCKED", "CHẶN"),
    ("PASS", "ĐẠT"),
    ("FAIL", "CHƯA ĐẠT"),
];

/// English name of a stored code for display; unknown codes are returned unchanged.
pub fn kind_to_english(stored: &str) -> &str {
    KINDS_EN
        .iter()
        .find(|(_, vi)| *vi == stored)
        .map_or(stored, |(en, _)| en)
}

/// Stored code for an English type (any letter case). Whole word only: `REPORTING` is not `REPORT`.
pub fn kind_from_english(word: &str) -> Option<&'static str> {
    KINDS_EN
        .iter()
        .find(|(en, _)| en.eq_ignore_ascii_case(word))
        .map(|(_, vi)| *vi)
}
const PROTECTED_APPS: [&str; 2] = ["codex", "claude-code"];

#[derive(Debug, Clone, PartialEq)]
pub enum Caller {
    /// Lệnh gõ từ terminal của người; `confirmed` = đã trả lời y hoặc `--yes` nếu lệnh cho phép (cleanup luôn hỏi).
    User { confirmed: bool },
    /// Lệnh do một agent (hoặc script không TTY) gọi; kèm terminal gọi nếu biết.
    Agent(Option<String>),
}

/// `hc_terminal`: `HC_TERMINAL_ID` của tiến trình gọi; `terminal_is_agent`: terminal đó đã tự báo 7501.
pub fn classify_caller(
    hc_terminal: Option<&str>,
    terminal_is_agent: bool,
    tty: bool,
    confirmed: bool,
) -> Caller {
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
/// Tên tiến trình shell tương tác (sau khi bỏ đuôi `.exe`, chữ thường).
const SHELL_NAMES: [&str; 8] = [
    "pwsh",
    "powershell",
    "cmd",
    "bash",
    "sh",
    "zsh",
    "fish",
    "nu",
];

fn is_interactive_shell(name: &str) -> bool {
    let base = name.trim().to_ascii_lowercase();
    let base = base.strip_suffix(".exe").unwrap_or(&base);
    SHELL_NAMES.contains(&base)
}

/// `hc get` cho thấy tiến trình nền là shell tương tác và không có agent nào đang sống
/// (không có `status.app`). Các cờ `reported`/app trong CSDL có thể còn sót lại từ agent đã thoát.
fn idle_shell_foreground(v: &serde_json::Value) -> bool {
    let Some(name) = v["foreground"]["name"].as_str() else {
        return false;
    };
    is_interactive_shell(name)
        && v["status"]["app"]
            .as_str()
            .is_none_or(|a| a.trim().is_empty())
}

/// App hiện tại của terminal: ưu tiên `hc get` (mới nhất), rồi CSDL. None nếu không biết.
fn current_app(store: &Store, hc: &dyn Hc, terminal: &str) -> Option<String> {
    let fresh = hc
        .get(terminal)
        .ok()
        .map(|v| terminal_row_from(&v))
        .and_then(|r| r.app);
    fresh.or_else(|| store.terminal(terminal).ok().flatten().and_then(|t| t.app))
}

/// Xác định ai đang gọi — không chắc thì coi là agent (fail-closed):
/// có `HC_TERMINAL_ID` mà `hc get` lỗi, hoặc terminal gọi đang chạy một app agent → agent.
pub fn detect_caller(
    store: &Store,
    hc: &dyn Hc,
    hc_terminal: Option<&str>,
    tty: bool,
    confirmed: bool,
) -> Caller {
    let is_agent = match hc_terminal {
        None => false,
        Some(t) => match hc.get(t) {
            Err(_) => true,
            Ok(v) => {
                if idle_shell_foreground(&v) {
                    // Agent đã thoát, shell của người dùng hiện lại: bỏ qua cờ reported/CSDL cũ.
                    false
                } else {
                    let row = terminal_row_from(&v);
                    let app = row
                        .app
                        .or_else(|| store.terminal(t).ok().flatten().and_then(|r| r.app));
                    row.reported || app.as_deref().is_some_and(|a| AGENT_APPS.contains(&a))
                }
            }
        },
    };
    classify_caller(hc_terminal, is_agent, tty, confirmed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneSelectorParse {
    pub ids: Vec<i64>,
    pub legacy_single: bool,
}

fn is_plain_single_id(s: &str) -> bool {
    !s.is_empty() && !s.contains(',') && !s.contains('-') && s.chars().all(|c| c.is_ascii_digit())
}

fn parse_id_token(token: &str) -> Result<i64> {
    if token.is_empty() || !token.chars().all(|c| c.is_ascii_digit()) {
        bail!("invalid id selector: {token:?}");
    }
    token
        .parse::<i64>()
        .map_err(|_| anyhow::anyhow!("invalid id selector: {token:?}"))
}

fn parse_selector_arg(arg: &str, out: &mut Vec<i64>) -> Result<()> {
    if arg.is_empty() {
        bail!("invalid id selector: empty");
    }
    for part in arg.split(',') {
        if part.is_empty() {
            bail!("invalid id selector: empty token");
        }
        if let Some((a, b)) = part.split_once('-') {
            if b.contains('-') {
                bail!("invalid id selector: {part:?}");
            }
            let start = parse_id_token(a)?;
            let end = parse_id_token(b)?;
            if end < start {
                bail!("invalid id range: {part}");
            }
            // `end - start` cannot overflow (both are non-negative); `+ 1` could at i64::MAX.
            if end - start >= 10_000 {
                bail!("id range too large: {part}");
            }
            for id in start..=end {
                out.push(id);
            }
        } else {
            out.push(parse_id_token(part)?);
        }
    }
    Ok(())
}

/// Parse selectors for `agm done`: each arg is `N`, `N-M`, or `N,M-K`.
pub fn parse_done_selectors(args: &[String]) -> Result<DoneSelectorParse> {
    if args.is_empty() {
        bail!("no id selectors");
    }
    let legacy_single = args.len() == 1 && is_plain_single_id(&args[0]);
    let mut ids = Vec::new();
    for arg in args {
        parse_selector_arg(arg, &mut ids)?;
    }
    if ids.is_empty() {
        bail!("no id selectors");
    }
    Ok(DoneSelectorParse { ids, legacy_single })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DoneFilters {
    /// Canonical stored kind code (e.g. "BÁO CÁO"), if specified.
    pub kind: Option<String>,
    pub terminal: Option<String>,
    /// One of "message", "context", "rotation", "blocked".
    pub source: Option<String>,
}

impl DoneFilters {
    pub fn is_empty(&self) -> bool {
        self.kind.is_none() && self.terminal.is_none() && self.source.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DoneSkip {
    pub id: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DoneReport {
    pub done: Vec<i64>,
    pub skipped: Vec<DoneSkip>,
}

/// Normalize a `--kind` filter value (English alias or Vietnamese code) to a stored code.
pub fn normalize_done_kind(input: &str) -> Option<String> {
    if let Some(code) = kind_from_english(input) {
        return Some(code.to_string());
    }
    KINDS
        .iter()
        .find(|k| k.eq_ignore_ascii_case(input) || **k == input)
        .map(|k| (*k).to_string())
}

/// Validate and normalize a `--source` filter value.
pub fn normalize_done_source(input: &str) -> Option<String> {
    match input.to_ascii_lowercase().as_str() {
        "message" | "context" | "rotation" | "blocked" => Some(input.to_ascii_lowercase()),
        _ => None,
    }
}

fn done_item_matches_filters(item: &crate::store::InboxRow, filters: &DoneFilters) -> bool {
    if let Some(ref kind) = filters.kind {
        let ok = item
            .kind
            .as_deref()
            .is_some_and(|k| k == kind.as_str() || k.eq_ignore_ascii_case(kind.as_str()));
        if !ok {
            return false;
        }
    }
    if filters
        .terminal
        .as_ref()
        .is_some_and(|term| item.terminal.as_deref() != Some(term.as_str()))
    {
        return false;
    }
    if filters
        .source
        .as_ref()
        .is_some_and(|source| &item.source != source)
    {
        return false;
    }
    true
}

/// Bulk dismiss inbox items by explicit IDs and/or filters.
///
/// - `legacy_single`: plain legacy form (`agm done <N>` with no filters).
/// - `explicit_source_blocked`: user explicitly selected `source=blocked`.
pub fn dismiss_done(
    store: &Store,
    ids: Vec<i64>,
    filters: DoneFilters,
    explicit_source_blocked: bool,
    legacy_single: bool,
    now: i64,
) -> Result<DoneReport> {
    if ids.is_empty() && filters.is_empty() {
        bail!("usage: agm done <ID...> [--kind K] [--terminal T] [--source S]");
    }

    let mut report = DoneReport {
        done: Vec::new(),
        skipped: Vec::new(),
    };

    if !ids.is_empty() {
        for id in ids {
            let item = store.inbox_item(id)?;
            let Some(item) = item else {
                if legacy_single {
                    bail!("không có mục hộp thư #{id}");
                }
                report.skipped.push(DoneSkip {
                    id,
                    reason: "missing".into(),
                });
                continue;
            };
            if item.resolved_ms.is_some() {
                // Legacy single-ID behavior only errors on missing; resolving again is idempotent.
                if !legacy_single {
                    report.skipped.push(DoneSkip {
                        id,
                        reason: "already done".into(),
                    });
                    continue;
                }
            }
            if !done_item_matches_filters(&item, &filters) {
                if legacy_single {
                    // legacy path with filter is not expected, but be safe
                    bail!("không có mục hộp thư #{id}");
                }
                report.skipped.push(DoneSkip {
                    id,
                    reason: "missing".into(),
                });
                continue;
            }
            if item.source == "blocked" && !explicit_source_blocked && !legacy_single {
                report.skipped.push(DoneSkip {
                    id,
                    reason: "blocked (use --source blocked)".into(),
                });
                continue;
            }
            report.done.push(id);
        }
    } else {
        for item in store.inbox(true)? {
            if !done_item_matches_filters(&item, &filters) {
                continue;
            }
            if item.source == "blocked" && !explicit_source_blocked {
                report.skipped.push(DoneSkip {
                    id: item.id,
                    reason: "blocked (use --source blocked)".into(),
                });
                continue;
            }
            report.done.push(item.id);
        }
    }

    store.resolve_inbox_many(&report.done, now)?;
    Ok(report)
}

/// Dóng một mục hộp thư mà không gõ gì vào terminal nào (hành vi đơn lẻ cũ).
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

pub fn format_message(
    from: &str,
    kind: &str,
    task: Option<&str>,
    text: &str,
    to_user: bool,
) -> String {
    let id = task.map(|t| format!(" [ID {t}]")).unwrap_or_default();
    if to_user {
        format!("[{kind}]{id} {text}")
    } else {
        format!("[{kind}]{id} từ {from}: {text}")
    }
}

#[allow(clippy::too_many_arguments)]
/// Gửi một tin theo mẫu §3.2. Đích `user` → hộp thư. Tin luôn được ghi lại, kể cả khi gửi thất bại.
pub fn send(
    store: &Store,
    hc: &dyn Hc,
    from: &str,
    to: &str,
    kind: &str,
    task: Option<&str>,
    text: &str,
    now: i64,
) -> Result<MessageRow> {
    if !KINDS.contains(&kind) {
        bail!(
            "loại tin '{kind}' không hợp lệ; dùng một trong: {}",
            KINDS.join(", ")
        );
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
        // `hc send --if` chỉ nhận đúng `idle`/`working`; agent vừa xong lượt (`done`) không còn làm việc nên
        // gõ vào là an toàn, nhưng `--if idle` lại từ chối. Đọc trạng thái mới: `done` → gửi không kèm chốt;
        // mọi trạng thái khác (working, blocked, idle) hoặc đọc lỗi → giữ chốt `--if idle` như cũ.
        let done = hc.get(to).ok().is_some_and(|v| {
            v["state"].as_str() == Some("live") && v["status"]["state"].as_str() == Some("done")
        });
        match hc.send(to, &body, !done, false) {
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
    let Some(item) = store.inbox_item(inbox_id)? else {
        bail!("không có mục hộp thư #{inbox_id}")
    };
    // Chỉ trả lời được câu hỏi/chặn của agent hoặc tin agent gửi cho người dùng; các mục khác chỉ để đóng.
    if !matches!(item.source.as_str(), "blocked" | "message") {
        bail!(
            "mục #{inbox_id} ({}) không phải câu hỏi của agent — dùng `agm done {inbox_id}` để đóng",
            item.source
        );
    }
    if item.resolved_ms.is_some() {
        bail!("mục #{inbox_id} đã được xử lý");
    }
    let Some(term) = item.terminal.as_deref() else {
        bail!("mục #{inbox_id} không gắn với terminal nào để trả lời")
    };
    if term == "agent" {
        bail!(
            "mục #{inbox_id} do một agent không rõ terminal gửi qua MCP — không có terminal để trả lời; dùng `agm done {inbox_id}` để đóng"
        );
    }
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
    pub text: Vec<String>,
}

fn normalize_role(role: &str) -> Result<String> {
    let r = role.trim().to_ascii_lowercase();
    if r.is_empty() {
        bail!("vai trò (role) không được rỗng");
    }
    Ok(r)
}

fn valid_name(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Mọi điều kiện để `spawn` thành công, kiểm trước (không gọi hc). `replacing`: terminal sắp được thay
/// (xoay vòng) — không tính vào max_workers. Trả tên terminal sẽ dùng.
pub fn spawn_precheck(
    store: &Store,
    cfg: &Config,
    req: &SpawnReq,
    replacing: Option<&str>,
) -> Result<String> {
    if !valid_name(&req.task) {
        bail!(
            "mã việc '{}' không hợp lệ: chỉ a-z 0-9 - _ (tối đa 32)",
            req.task
        );
    }
    if req.template.trim().is_empty() {
        bail!("thiếu template");
    }
    let role = normalize_role(&req.role)?;
    if role == "worker" {
        let workers = store
            .terminals(true)?
            .iter()
            .filter(|t| {
                t.role
                    .as_deref()
                    .map(|r| r.trim().to_ascii_lowercase())
                    .as_deref()
                    == Some("worker")
                    && Some(t.address.as_str()) != replacing
            })
            .count();
        if workers as u32 >= cfg.policy.max_workers {
            bail!(
                "đã có {workers} thợ đang chạy (max_workers = {}); chờ một thợ xong hoặc tăng [policy].max_workers",
                cfg.policy.max_workers
            );
        }
    }
    if let Some(reason) = crate::policy::spawn_blocked_by_budget(store, &req.task)? {
        bail!("{reason}");
    }
    let name = match &req.name {
        Some(n) => n.clone(),
        None => {
            let prefix = format!("w-{}-", req.task);
            let used = store
                .terminals(false)?
                .iter()
                .filter(|t| t.name.as_deref().is_some_and(|n| n.starts_with(&prefix)))
                .count();
            format!("{prefix}{}", used + 1)
        }
    };
    if !valid_name(&name) {
        bail!(
            "tên terminal '{name}' không hợp lệ (1–32 ký tự a-z 0-9 - _); dùng mã việc ngắn hơn hoặc --name"
        );
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
    let app_tag = if spec.app == "claude-code" {
        "claude"
    } else {
        "codex"
    };
    let base_raw = old_name
        .filter(|n| !n.is_empty())
        .map(|n| n.split("-r").next().unwrap_or(n))
        .unwrap_or(old_addr);
    let sanitized: String = base_raw
        .bytes()
        .map(|b| {
            if b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' {
                b as char
            } else {
                '-'
            }
        })
        .collect();
    let prefix = if sanitized.is_empty() {
        format!("rot-{app_tag}")
    } else {
        sanitized
    };
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
    let role = normalize_role(&req.role)?;
    let normalized = SpawnReq {
        role: role.clone(),
        ..req.clone()
    };
    let req = &normalized;
    let name = spawn_precheck(store, cfg, req, None)?;
    let listed = hc
        .templates()
        .map(|ts| ts.iter().any(|t| t["name"].as_str() == Some(&req.template)))
        .unwrap_or(false);
    let on_disk = crate::hc::load_template(&req.template).is_ok();
    let is_template = listed || on_disk;
    let mut argv = vec![req.template.clone()];
    if is_template {
        if !req.text.is_empty() {
            argv.push(req.text.join(" "));
        }
    } else {
        argv.extend(req.text.iter().cloned());
    }
    let spec = RunSpec {
        argv,
        name: Some(name.clone()),
        cwd: req.cwd.clone(),
        labels: [
            ("agm.task".to_string(), req.task.clone()),
            ("agm.role".to_string(), req.role.clone()),
        ]
        .into(),
    };
    let mut brief = None;
    let res = match hc.run(&spec) {
        Err(e) if cfg!(windows) && is_template && crate::hc::is_missing_sh(&e) => {
            let template = crate::hc::load_template(&req.template)?;
            let direct = RunSpec {
                argv: template.argv,
                ..spec.clone()
            };
            brief = Some((
                format!("{}\n\n{}", template.body, req.text.join(" ")),
                crate::hc::app_from_argv(&direct.argv),
            ));
            hc.run(&direct)
        }
        other => other,
    };
    store.log_action(
        "agm",
        "spawn",
        &name,
        &format!("{} task={} role={}", req.template, req.task, req.role),
        res.is_ok(),
        now,
    )?;
    let v = res?;
    let mut row = terminal_row_from(&v);
    if row.address.is_empty() {
        bail!("hc run không trả địa chỉ terminal: {v}");
    }
    row.template = Some(req.template.clone());
    row.task.get_or_insert(req.task.clone());
    row.role.get_or_insert(req.role.clone());
    if row.created_ms == 0 {
        row.created_ms = now;
    }
    store.upsert_terminal(&row)?;
    if let Some((brief, app)) = brief {
        send_spawn_brief(store, hc, &row.address, &brief, app.as_deref(), now)?;
    }
    Ok(row.address)
}

/// Verify a successor's brief transport before rotation is allowed to kill anything.
/// Unknown programs fail closed; OpenCode accepts an explicit --prompt argument.
pub fn prepare_rotation_spawn(hc: &dyn Hc, req: &mut SpawnReq) -> Result<()> {
    let listed = hc
        .templates()
        .map(|ts| ts.iter().any(|t| t["name"].as_str() == Some(&req.template)))
        .unwrap_or(false);
    if listed {
        if cfg!(windows) {
            // `hc run <template>` cần đường dự phòng `sh` trên Windows, nên phải đọc được command:.
            crate::hc::load_template(&req.template)?;
        }
        return Ok(());
    }
    // Đường dự phòng: template có trên đĩa và parse được command:.
    if crate::hc::load_template(&req.template).is_ok() {
        return Ok(());
    }
    if crate::hc::app_from_argv(std::slice::from_ref(&req.template)).as_deref() == Some("opencode")
    {
        req.text = vec!["--prompt".into(), req.text.join(" ")];
        return Ok(());
    }
    bail!(
        "không xác minh được template hoặc cách nhận brief của '{}'",
        req.template
    )
}

/// Mã thoát CLI khi terminal đã mở nhưng brief chưa tới: script giao việc không được
/// dọn worktree (thợ đang chạy trong đó) và cũng không được spawn lại (sẽ sinh trùng thợ).
pub const EXIT_BRIEF_UNDELIVERED: u8 = 3;

/// `agm spawn` đã mở terminal nhưng không gửi được brief template; brief nằm trong hộp thư.
#[derive(Debug)]
pub struct BriefUndelivered {
    pub address: String,
    pub detail: String,
}

impl std::fmt::Display for BriefUndelivered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "đã mở terminal {} nhưng chưa gửi được brief ({}); terminal vẫn chạy — gửi brief trong mục hộp thư brief_failed, không mở thêm phiên",
            self.address, self.detail
        )
    }
}

impl std::error::Error for BriefUndelivered {}

/// Mã thoát cho lỗi CLI: `EXIT_BRIEF_UNDELIVERED` nếu là `BriefUndelivered` (kể cả đã bọc context), còn lại 1.
pub fn exit_code(err: &anyhow::Error) -> u8 {
    if err.downcast_ref::<BriefUndelivered>().is_some() {
        EXIT_BRIEF_UNDELIVERED
    } else {
        1
    }
}

fn send_spawn_brief(
    store: &Store,
    hc: &dyn Hc,
    address: &str,
    brief: &str,
    app: Option<&str>,
    now: i64,
) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut detail = "agent chưa sẵn sàng".to_string();
    loop {
        match hc.get(address) {
            Ok(v)
                if v["state"].as_str() == Some("live")
                    && app.is_some_and(|a| v["status"]["app"].as_str() == Some(a))
                    && matches!(v["status"]["state"].as_str(), Some("idle" | "done")) =>
            {
                match hc.send(
                    address,
                    brief,
                    v["status"]["state"].as_str() != Some("done"),
                    false,
                ) {
                    Ok(_) => {
                        store.log_action(
                            "agm",
                            "spawn_brief",
                            address,
                            "đã gửi brief template",
                            true,
                            now,
                        )?;
                        return Ok(());
                    }
                    Err(e) => detail = format!("{e:#}"),
                }
            }
            Ok(v) if v["state"].as_str() == Some("finished") => {
                detail = "terminal đã kết thúc trước khi nhận brief".into();
                break;
            }
            Err(e) => detail = format!("{e:#}"),
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    store.open_inbox(
        Some(address),
        "spawn",
        Some("brief_failed"),
        &format!("Chưa gửi được brief template: {detail}\n\n{brief}"),
        now,
    )?;
    store.log_action("agm", "spawn_brief", address, &detail, false, now)?;
    Err(BriefUndelivered {
        address: address.to_string(),
        detail,
    }
    .into())
}

/// Kiểm quyền kill/ngắt theo quy tắc của người dùng; từ chối thì ghi nhật ký và chuyển đề nghị vào hộp thư.
/// Agent/tự động: **danh sách trắng** — chỉ app OpenCode đã xác định; không rõ app → từ chối.
fn authorize(
    store: &Store,
    hc: &dyn Hc,
    caller: &Caller,
    target: &str,
    verb: &str,
    now: i64,
) -> Result<()> {
    let app = current_app(store, hc, target);
    let shown = app.clone().unwrap_or_else(|| "không rõ app".into());
    let who = caller_name(caller);
    let refusal = match caller {
        Caller::User { confirmed: false } => Some("cần xác nhận (trả lời y)".to_string()),
        Caller::User { confirmed: true } => None,
        Caller::Agent(_)
            if app
                .as_deref()
                .is_some_and(|a| AGENT_KILLABLE_APPS.contains(&a)) =>
        {
            None
        }
        Caller::Agent(_) if app.as_deref().is_some_and(|a| PROTECTED_APPS.contains(&a)) => {
            Some(format!("chỉ người dùng được {verb} phiên {shown}"))
        }
        Caller::Agent(_) => Some(format!(
            "chỉ người dùng được {verb} {target} ({shown}); agent chỉ dừng được thợ OpenCode"
        )),
    };
    if let Some(reason) = refusal {
        store.log_action(
            &who,
            verb,
            target,
            &format!("từ chối: {reason}"),
            false,
            now,
        )?;
        if matches!(caller, Caller::Agent(_)) {
            store.open_inbox(
                Some(target),
                "request",
                Some(verb),
                &format!("{who} đề nghị {verb} {target} ({shown})"),
                now,
            )?;
        }
        bail!("{reason}");
    }
    Ok(())
}

/// Bật `managed=1` + lưu `LaunchSpec` sau khi kiểm tra nghiêm ngặt người gọi (chỉ User đã xác nhận `y`).
pub fn set_managed_launch(
    store: &Store,
    caller: &Caller,
    target: &str,
    spec: &LaunchSpec,
    now: i64,
) -> Result<()> {
    let who = caller_name(caller);
    let detail = format!(
        "exe={}, args={:?}, cwd={}",
        spec.executable, spec.argv, spec.cwd
    );
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
            let reason =
                format!("chỉ người dùng ở terminal mới được bật quản lý `agm manage` cho {target}");
            store.log_action(
                &who,
                "manage",
                target,
                &format!("từ chối: {reason}"),
                false,
                now,
            )?;
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

pub fn interrupt(
    store: &Store,
    hc: &dyn Hc,
    caller: &Caller,
    target: &str,
    now: i64,
) -> Result<()> {
    authorize(store, hc, caller, target, "interrupt", now)?;
    let res = hc.interrupt(target);
    store.log_action(
        &caller_name(caller),
        "interrupt",
        target,
        "",
        res.is_ok(),
        now,
    )?;
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
    pub old_created_ms: i64,
    pub new_created_ms: Option<i64>,
    pub old_session: Option<i64>,
    pub new_session: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CleanupReport {
    pub done: usize,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

/// Danh sách phiên nguồn đã `handed_off` đủ điều kiện dọn thủ công:
/// 1) `old live` + phiên kế nhiệm đã `linked` & `live`, hoặc 2) `old` đã thoát (`finished`).
pub fn eligible_cleanups(store: &Store) -> Result<Vec<CleanupCandidate>> {
    let mut stmt = store.conn().prepare(
        "SELECT r.id, o.address, o.name, o.app, o.state, r.new_terminal, n.state, n.session, r.handoff_path,
                o.created_ms, n.created_ms, o.session
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
                    old_created_ms: r.get(9)?,
                    new_created_ms: r.get(10)?,
                    old_session: r.get(11)?,
                    new_session: new_sess,
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

/// Require explicit live/finished state and the same known app, creation identity and cwd.
fn fresh_cleanup_terminal(
    hc: &dyn Hc,
    expected: &crate::store::TerminalRow,
) -> Result<crate::store::TerminalRow> {
    let value = hc.get(&expected.address)?;
    let fresh = terminal_row_from(&value);
    if !matches!(value["state"].as_str(), Some("live" | "finished"))
        || fresh.address != expected.address
        || expected.app.as_deref().is_none_or(str::is_empty)
        || fresh.app != expected.app
        || expected.created_ms <= 0
        || fresh.created_ms != expected.created_ms
        || expected.cwd.as_deref().is_some_and(|cwd| {
            fresh.cwd.as_deref().is_none_or(|current| {
                crate::correlate::norm_cwd(current) != crate::correlate::norm_cwd(cwd)
            })
        })
    {
        bail!(
            "không xác minh được danh tính hiện tại của {}",
            expected.address
        );
    }
    Ok(fresh)
}

fn record_cleanup(store: &Store, address: &str, detail: &str, now: i64) -> Result<()> {
    store.remove_terminal(address)?;
    store.resolve_inbox_for_terminal(address, "rotation", now)?;
    store.resolve_inbox_for_terminal(address, "context", now)?;
    store.log_action("user", "cleanup", address, detail, true, now)
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
        let unchanged: bool = store.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM rotations
             WHERE id = ?1 AND old_terminal = ?2 AND new_terminal IS ?3
               AND app IS ?4 AND state = 'done')",
            rusqlite::params![
                item.rotation_id,
                item.old_terminal,
                item.new_terminal,
                item.old_app
            ],
            |row| row.get(0),
        )?;
        if !unchanged || item.new_terminal.as_deref() == Some(&item.old_terminal) {
            report.skipped.push(format!(
                "{}: quan hệ bàn giao đã thay đổi",
                item.old_terminal
            ));
            continue;
        }
        let Some(old_row) = store.terminal(&item.old_terminal)? else {
            report
                .skipped
                .push(format!("{}: không còn thấy terminal", item.old_terminal));
            continue;
        };
        if old_row.handoff_state != "handed_off" || old_row.state == "removed" {
            report.skipped.push(format!(
                "{}: không còn ở trạng thái chờ dọn",
                item.old_terminal
            ));
            continue;
        }
        if old_row.created_ms != item.old_created_ms || old_row.session != item.old_session {
            report.skipped.push(format!(
                "{}: danh tính nguồn đã đổi từ lúc chọn",
                item.old_terminal
            ));
            continue;
        }
        if old_row.state == "live" {
            let successor = match item
                .new_terminal
                .as_deref()
                .map(|a| store.terminal(a))
                .transpose()?
                .flatten()
            {
                Some(nt)
                    if nt.state == "live"
                        && nt.session.is_some()
                        && nt.app == item.old_app
                        && Some(nt.created_ms) == item.new_created_ms
                        && nt.session == item.new_session =>
                {
                    nt
                }
                _ => {
                    report.skipped.push(format!(
                        "{}: phiên kế nhiệm không còn live hoặc chưa ghép",
                        item.old_terminal
                    ));
                    continue;
                }
            };
            if old_row.app != item.old_app {
                report
                    .skipped
                    .push(format!("{}: app nguồn đã thay đổi", item.old_terminal));
                continue;
            }
            match fresh_cleanup_terminal(hc, &successor) {
                Ok(fresh) if fresh.state == "live" => {}
                _ => {
                    report.skipped.push(format!(
                        "{}: không xác minh được phiên kế nhiệm còn live",
                        item.old_terminal
                    ));
                    continue;
                }
            }
            let fresh = match fresh_cleanup_terminal(hc, &old_row) {
                Ok(fresh) => fresh,
                Err(e) => {
                    report.skipped.push(format!("{}: {e:#}", item.old_terminal));
                    continue;
                }
            };
            if fresh.state == "finished" {
                let result = store
                    .finish_terminal(&item.old_terminal, fresh.exit_code, now)
                    .and_then(|()| {
                        record_cleanup(store, &item.old_terminal, "old freshly finished", now)
                    });
                match result {
                    Ok(()) => report.done += 1,
                    Err(e) => report.errors.push(format!("{}: {e:#}", item.old_terminal)),
                }
                continue;
            }
            // Caller was explicitly confirmed above; kill immediately after identity checks,
            // without a second lookup that could silently fall back to stale database state.
            let result = hc.kill(&item.old_terminal);
            if let Err(e) = store.log_action(
                "user",
                "kill",
                &item.old_terminal,
                "cleanup",
                result.is_ok(),
                now,
            ) {
                report.errors.push(format!(
                    "{}: lỗi ghi nhật ký kill: {e:#}",
                    item.old_terminal
                ));
            }
            match result {
                Ok(()) => {
                    let tracked = store
                        .finish_terminal(&item.old_terminal, None, now)
                        .and_then(|()| {
                            record_cleanup(
                                store,
                                &item.old_terminal,
                                "old killed after handoff",
                                now,
                            )
                        });
                    match tracked {
                        Ok(()) => report.done += 1,
                        Err(e) => report.errors.push(format!(
                            "{}: đã kill nhưng lỗi ghi nhận: {e:#}",
                            item.old_terminal
                        )),
                    }
                }
                Err(e) => report.errors.push(format!("{}: {e:#}", item.old_terminal)),
            }
        } else if old_row.state == "finished" {
            // Nhánh old exited: dọn bản ghi terminal và đóng inbox liên quan, không gọi hc.
            match record_cleanup(store, &item.old_terminal, "old exited", now) {
                Ok(()) => report.done += 1,
                Err(e) => report.errors.push(format!("{}: {e:#}", item.old_terminal)),
            }
        } else {
            report.skipped.push(format!(
                "{}: trạng thái nguồn không xác định",
                item.old_terminal
            ));
        }
    }
    Ok(report)
}
