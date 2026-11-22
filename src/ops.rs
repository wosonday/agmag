//! Các lệnh v1 của binary `agm`: daemon, send, inbox, reply, spawn, kill, interrupt, rotate, budget, policy, link, ask, hook, ps.

use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use agmag::actions::{
    self, Caller, CleanupCandidate, SpawnReq, classify_caller, eligible_cleanups,
    execute_cleanup_batch,
};
use agmag::config::Config;
use agmag::git::GitCli;
use agmag::hc::{Hc, HcCli, effective_cwd, terminal_row_from};
use agmag::launch::{
    check_claude_hook_installed, extract_launch_argv, parse_cmd_tokens, parse_launch_spec,
};
use agmag::store::{BudgetRow, Store};
use agmag::{correlate, daemon, policy, report, rotation};

pub fn hc_terminal() -> Option<String> {
    std::env::var("HC_TERMINAL_ID")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Ai đang gọi: người dùng (TTY, terminal gọi không phải agent) hay agent. Hỏi y/N khi cần xác nhận.
/// Fail-closed (`actions::detect_caller`): không chắc thì là agent.
pub fn caller(store: &Store, yes: bool, needs_confirm: Option<&str>) -> Result<Caller> {
    let term = hc_terminal();
    let tty = std::io::stdin().is_terminal();
    let first = actions::detect_caller(store, &HcCli::default(), term.as_deref(), tty, yes);
    let Caller::User { confirmed } = first else {
        return Ok(first);
    };
    if confirmed {
        return Ok(first);
    }
    let Some(question) = needs_confirm else {
        return Ok(first);
    };
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(classify_caller(
        term.as_deref(),
        false,
        tty,
        matches!(line.trim(), "y" | "Y" | "yes" | "có"),
    ))
}

pub fn print_json(v: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

// ---------- branch ----------

pub fn branch_get(file: &Path, entry: &str, json_out: bool) -> Result<()> {
    let Some(fields) = agmag::branches::get(file, entry)? else {
        bail!("không có mục {entry} trong {}", file.display());
    };
    if json_out {
        return print_json(&json!({"entry": entry, "fields": fields}));
    }
    for (k, v) in fields {
        println!("{k}: {v}");
    }
    Ok(())
}

pub fn branch_set(
    store: &Store,
    file: &Path,
    entry: &str,
    pairs: &[String],
    worktree: Option<&str>,
    json_out: bool,
    now_ms: i64,
) -> Result<()> {
    let mut fields = Vec::new();
    for p in pairs {
        let Some((k, v)) = p.split_once('=') else {
            bail!("đối số '{p}' phải có dạng khóa=giá-trị");
        };
        fields.push((k.to_string(), v.to_string()));
    }
    let req = agmag::branches::BranchUpdate {
        file: file.to_path_buf(),
        entry: entry.to_string(),
        fields,
        worktree: worktree.map(str::to_string),
    };
    let out = agmag::branches::update(
        store,
        &GitCli,
        hc_terminal().as_deref(),
        &req,
        agmag::branches::LOCK_TIMEOUT,
        now_ms,
    )?;
    if json_out {
        return print_json(&out);
    }
    println!(
        "Updated {} ({})",
        out.entry,
        if out.created { "created" } else { "changed" }
    );
    Ok(())
}

// ---------- daemon ----------

pub fn daemon_replay(store: &Store, file: &Path, now: i64) -> Result<usize> {
    let text = std::fs::read_to_string(file).with_context(|| format!("đọc {}", file.display()))?;
    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            daemon::apply_event(store, &v, now)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Vòng lặp nền: luồng `hc events --json` (tự nối lại) + mỗi `interval` giây quét log, ghép, policy, xoay vòng.
pub fn daemon_loop(
    store: &Store,
    cfg: &Config,
    interval: u64,
    mut periodic: impl FnMut(&Store) -> Result<()>,
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    let hc_bin = std::env::var("AGM_HC_BIN").unwrap_or_else(|_| "hc".into());
    std::thread::spawn(move || {
        loop {
            match Command::new(&hc_bin)
                .args(["events", "--json"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    if let Some(out) = child.stdout.take() {
                        for line in BufReader::new(out).lines().map_while(Result::ok) {
                            if tx.send(line).is_err() {
                                return;
                            }
                        }
                    }
                    let _ = child.wait();
                }
                Err(e) => eprintln!("agm daemon: không chạy được `{hc_bin} events`: {e}"),
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
    let hc = HcCli::default();
    let git = GitCli;
    eprintln!("agm daemon: watching (every {interval}s). Ctrl+C to stop.");
    let mut last: Option<std::time::Instant> = None;
    loop {
        let wait = last.map_or(Duration::ZERO, |l| {
            Duration::from_secs(interval).saturating_sub(l.elapsed())
        });
        match rx.recv_timeout(wait) {
            Ok(line) => {
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && let Err(e) = daemon::apply_event(store, &v, crate::now_ms())
                {
                    eprintln!("agm daemon: sự kiện lỗi: {e:#}");
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("luồng sự kiện hc đã dừng"),
        }
        if last.is_none_or(|l| l.elapsed() >= Duration::from_secs(interval)) {
            // Rút cạn sự kiện đã đến để policy chạy trên trạng thái mới nhất.
            while let Ok(line) = rx.try_recv() {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    let _ = daemon::apply_event(store, &v, crate::now_ms());
                }
            }
            last = Some(std::time::Instant::now());
            let now = crate::now_ms();
            // Mỗi bước độc lập: một bước lỗi không làm bỏ qua các bước sau.
            let report = |name: &str, r: Result<()>| {
                if let Err(e) = r {
                    eprintln!("agm daemon: {name} lỗi: {e:#}");
                }
            };
            report("quét log", periodic(store));
            report("ghép terminal", correlate::link_all(store).map(|_| ()));
            report("policy", policy::tick(store, cfg, &hc, now));
            report("xoay vòng", rotation::tick(store, cfg, &hc, &git, now));
        }
    }
}

// ---------- tin nhắn & hộp thư ----------

pub fn send(
    store: &Store,
    to: &str,
    kind: &str,
    task: Option<&str>,
    text: &str,
    json_out: bool,
    now: i64,
) -> Result<()> {
    let from = hc_terminal().unwrap_or_else(|| "user".into());
    let m = actions::send(store, &HcCli::default(), &from, to, kind, task, text, now)?;
    if json_out {
        print_json(&m)?;
    } else {
        println!(
            "Message #{} → {}: {}{}",
            m.id,
            m.to,
            m.status,
            m.detail
                .as_deref()
                .map(|d| format!(" ({d})"))
                .unwrap_or_default()
        );
    }
    if m.status == "failed" {
        bail!("gửi không thành công: {}", m.detail.unwrap_or_default());
    }
    Ok(())
}

pub fn inbox(store: &Store, all: bool, json_out: bool) -> Result<()> {
    let rows = store.inbox(!all)?;
    if json_out {
        return print_json(&rows);
    }
    if rows.is_empty() {
        println!("Inbox is empty.");
    }
    for r in rows {
        let done = if r.resolved_ms.is_some() {
            " (resolved)"
        } else {
            ""
        };
        println!(
            "#{} [{}{}] {} — {}{done}",
            r.id,
            r.source,
            r.kind.map(|k| format!("/{k}")).unwrap_or_default(),
            r.terminal.unwrap_or_else(|| "-".into()),
            r.text
        );
    }
    Ok(())
}

pub fn reply(store: &Store, id: i64, text: &str, now: i64) -> Result<()> {
    actions::reply(store, &HcCli::default(), id, text, now)?;
    println!("Replied to #{id}.");
    Ok(())
}

// ---------- sinh / dừng agent ----------

pub fn spawn(store: &Store, cfg: &Config, req: &SpawnReq, now: i64) -> Result<()> {
    let addr = actions::spawn(store, &HcCli::default(), cfg, req, now)?;
    println!("{addr}");
    Ok(())
}

pub fn stop(store: &Store, target: &str, verb: &str, yes: bool, now: i64) -> Result<()> {
    let app = store
        .terminal(target)?
        .and_then(|t| t.app)
        .unwrap_or_else(|| "?".into());
    // Codex/Claude (hoặc chưa rõ app): luôn phải gõ y — `--yes` không đủ, vì agent có thể tự thêm cờ.
    let yes = yes && app == "opencode";
    let c = caller(store, yes, Some(&format!("{verb} {target} ({app})?")))?;
    let hc = HcCli::default();
    match verb {
        "kill" => actions::kill(store, &hc, &c, target, now)?,
        _ => actions::interrupt(store, &hc, &c, target, now)?,
    }
    println!("Done: {verb} {target}.");
    Ok(())
}

pub fn rotate(store: &Store, target: &str, yes: bool, now: i64) -> Result<()> {
    let Some(t) = store.terminal(target)? else {
        bail!("không biết terminal {target}")
    };
    match t.app.as_deref() {
        Some("opencode") => {
            if rotation::start(store, &t.address, None, true, now)? {
                println!(
                    "Queued {target} for rotation; the daemon will request a handoff when the agent is idle."
                );
            } else {
                println!("{target} is already rotating.");
            }
            Ok(())
        }
        Some(app @ ("claude-code" | "codex")) => {
            if t.handoff_state == "handed_off" {
                bail!("{target} đã bàn giao sang phiên mới rồi; dùng `agm cleanup` để dọn.");
            }
            if !t.managed {
                bail!(
                    "{target} ({app}) là phiên mở tay chưa bật quản lý — chạy `agm manage {target}` trước."
                );
            }
            if rotation::start(store, &t.address, None, true, now)? {
                println!(
                    "Queued {target} ({app}) for handoff and a successor session (the old session is not killed)."
                );
            } else {
                println!("{target} is already in a handoff or has handed off.");
            }
            Ok(())
        }
        other => {
            let c = caller(store, yes, None)?;
            bail!(
                "xoay vòng chưa hỗ trợ {}; hãy tự bàn giao rồi `agm kill {target}`{}",
                other.unwrap_or("app chưa rõ"),
                if matches!(c, Caller::Agent(_)) {
                    " (chỉ người dùng được kill)"
                } else {
                    ""
                }
            );
        }
    }
}

fn claude_settings_home() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("AGM_CLAUDE_HOME") {
        return Some(std::path::PathBuf::from(p));
    }
    dirs::home_dir()
}

pub fn manage(
    store: &Store,
    target: &str,
    off: bool,
    cmd: Option<&str>,
    raw_argv: &[String],
    now: i64,
) -> Result<()> {
    let Some(mut t) = store.terminal(target)? else {
        bail!("không biết terminal {target} (chạy `agm daemon` hoặc `agm ps` trước)");
    };
    if off {
        store.clear_terminal_managed(&t.address)?;
        // Tắt quản lý là hạ quyền nên ai cũng được làm, nhưng nhật ký phải ghi đúng người gọi.
        let who = hc_terminal().unwrap_or_else(|| "user".into());
        store.log_action(&who, "manage", &t.address, "off", true, now)?;
        println!("Automatic management disabled for {}.", t.address);
        return Ok(());
    }
    let hc = HcCli::default();
    let hc_info = hc.get(&t.address).ok();
    if let Some(v) = &hc_info {
        let fresh = terminal_row_from(v);
        if let Some(app) = fresh.app {
            t.app = Some(app);
        }
        // Thư mục người dùng đang đứng (foreground) khi gõ `agm manage`, không phải thư mục
        // lúc tạo shell — câu hỏi xác nhận in `cwd=` và launch_spec lưu đúng thư mục này.
        if let Some(cwd) = effective_cwd(v) {
            t.cwd = Some(cwd);
        }
    }
    let Some(app) = t
        .app
        .as_deref()
        .filter(|a| matches!(*a, "claude-code" | "codex"))
    else {
        bail!(
            "agm manage chỉ hỗ trợ claude-code và codex ({} là {})",
            t.address,
            t.app.as_deref().unwrap_or("không rõ app")
        );
    };
    let Some(cwd) = t.cwd.as_deref().filter(|c| !c.trim().is_empty()) else {
        bail!("không biết cwd của {}", t.address);
    };
    if app == "claude-code" {
        let home = claude_settings_home()
            .context("không tìm thấy thư mục home cho .claude/settings.json")?;
        check_claude_hook_installed(&home)?;
    }
    let argv = if !raw_argv.is_empty() {
        raw_argv.to_vec()
    } else if let Some(c) = cmd {
        parse_cmd_tokens(c)?
    } else if let Some(v) = &hc_info
        && let Some(found) = extract_launch_argv(v, app)
    {
        found
    } else {
        let bin = if app == "claude-code" {
            "claude"
        } else {
            "codex"
        };
        bail!(
            "không đọc được lệnh chạy {app} trực tiếp từ {} — hãy truyền lệnh tường minh qua `agm manage {target} -- {bin} ...`",
            t.address
        );
    };
    let spec = parse_launch_spec(app, &argv, cwd)?;
    let question = format!(
        "Bật quản lý tự động cho {} ({app}): exe={}, args={:?}, cwd={}?",
        t.address, spec.executable, spec.argv, spec.cwd
    );
    // Không nhận --yes: agent chạy ngầm có thể tự truyền --yes để nâng quyền argv cho phiên kế nhiệm.
    let c = caller(store, false, Some(&question))?;
    actions::set_managed_launch(store, &c, &t.address, &spec, now)?;
    println!(
        "Management enabled for {} ({app}): exe={}, args={:?}, cwd={}",
        t.address, spec.executable, spec.argv, spec.cwd
    );
    Ok(())
}

pub fn cleanup(
    store: &Store,
    targets: &[String],
    all: bool,
    _yes: bool,
    json_out: bool,
    now: i64,
) -> Result<()> {
    let candidates = eligible_cleanups(store)?;
    if json_out && targets.is_empty() && !all {
        return print_json(&candidates);
    }
    if candidates.is_empty() {
        if json_out {
            return print_json(&serde_json::json!({"done": 0, "skipped": [], "errors": []}));
        }
        println!("No handed-off sessions are ready for cleanup.");
        return Ok(());
    }

    let selected: Vec<CleanupCandidate> = if all {
        candidates.clone()
    } else if !targets.is_empty() {
        let mut chosen = Vec::new();
        for t in targets {
            let Some(item) = candidates
                .iter()
                .find(|c| c.old_terminal == *t || c.old_name.as_deref() == Some(t.as_str()))
            else {
                bail!("terminal '{t}' không nằm trong danh sách đủ điều kiện dọn dẹp");
            };
            if !chosen
                .iter()
                .any(|x: &CleanupCandidate| x.rotation_id == item.rotation_id)
            {
                chosen.push(item.clone());
            }
        }
        chosen
    } else {
        print_cleanup_candidates(&candidates);
        if !std::io::stdin().is_terminal() {
            bail!("cần chỉ định terminal, `--all` hoặc chạy ở terminal tương tác để chọn");
        }
        eprint!("Select items to clean up (e.g. 1,2 or all; Enter cancels): ");
        std::io::stderr().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let indices = parse_selection(line.trim(), candidates.len())?;
        if indices.is_empty() {
            println!("Cancelled.");
            return Ok(());
        }
        indices
            .into_iter()
            .map(|idx| candidates[idx].clone())
            .collect()
    };

    if targets.is_empty() && !all {
        // Đã hiển thị bảng ở trên; giờ chỉ hiện danh sách đã chốt.
        println!("Will clean up {} item(s):", selected.len());
    } else {
        print_cleanup_candidates(&selected);
    }
    let prompt = format!(
        "Confirm cleanup of {} handed-off session(s)?",
        selected.len()
    );
    // Batch cleanup always requires one typed confirmation, including --yes.
    // A cached app classification must never authorize a protected live target.
    let c = caller(store, false, Some(&prompt))?;
    if !matches!(c, Caller::User { confirmed: true }) {
        println!("Cancelled.");
        return Ok(());
    }
    let report = execute_cleanup_batch(store, &HcCli::default(), &c, &selected, now)?;
    if json_out {
        return print_json(&report);
    }
    println!(
        "Cleanup finished: {} cleaned, {} skipped, {} errors.",
        report.done,
        report.skipped.len(),
        report.errors.len()
    );
    for s in &report.skipped {
        println!("  [skipped] {s}");
    }
    for e in &report.errors {
        println!("  [error] {e}");
    }
    Ok(())
}

pub fn parse_selection(input: &str, max: usize) -> Result<Vec<usize>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.eq_ignore_ascii_case("all") {
        return Ok((0..max).collect());
    }
    let mut out = Vec::new();
    for part in trimmed.split([',', ' ']).filter(|s| !s.is_empty()) {
        let idx: usize = part
            .parse()
            .with_context(|| format!("số mục không hợp lệ: '{part}'"))?;
        if idx == 0 || idx > max {
            bail!("mục #{idx} nằm ngoài phạm vi 1..={max}");
        }
        if !out.contains(&(idx - 1)) {
            out.push(idx - 1);
        }
    }
    Ok(out)
}

fn print_cleanup_candidates(items: &[CleanupCandidate]) {
    for (i, c) in items.iter().enumerate() {
        let old_st = if c.old_live { "live" } else { "exited" };
        let new_desc = match (&c.new_terminal, c.new_live) {
            (Some(a), true) => format!("{a} (live)"),
            (Some(a), false) => format!("{a} (not live)"),
            (None, _) => "-".into(),
        };
        println!(
            "#{} {} ({}, {}) → {}  [rot #{} · {}]",
            i + 1,
            c.old_terminal,
            c.old_app.as_deref().unwrap_or("?"),
            old_st,
            new_desc,
            c.rotation_id,
            c.handoff_path.as_deref().unwrap_or("-"),
        );
    }
}

// ---------- ngân sách, policy, ghép tay ----------

pub fn budget_set(store: &Store, scope: &str, soft: f64, hard: f64, action: &str) -> Result<()> {
    let unit = match scope.split(':').collect::<Vec<_>>().as_slice() {
        ["task", id] if !id.is_empty() => "tokens",
        ["provider", p, w] if !p.is_empty() && w.parse::<u32>().is_ok() => "percent",
        _ => bail!(
            "phạm vi '{scope}' không hợp lệ: dùng task:<ID> hoặc provider:<tên>:<phút cửa sổ> (vd provider:codex:10080)"
        ),
    };
    if !soft.is_finite() || !hard.is_finite() || soft < 0.0 || hard < 0.0 {
        bail!("ngưỡng phải là số hữu hạn ≥ 0");
    }
    if soft > hard {
        bail!("ngưỡng mềm ({soft}) phải ≤ ngưỡng cứng ({hard})");
    }
    if !matches!(action, "notify" | "interrupt" | "kill") {
        bail!("hành động '{action}' không hợp lệ: notify | interrupt | kill");
    }
    store.set_budget(&BudgetRow {
        scope: scope.into(),
        unit: unit.into(),
        soft,
        hard,
        action: action.into(),
    })?;
    println!("Budget {scope} set: soft {soft}, hard {hard} {unit}, action {action}.");
    Ok(())
}

pub fn budget_ls(store: &Store, json_out: bool, now_ms: i64) -> Result<()> {
    let mut rows = Vec::new();
    for b in store.budgets()? {
        let v = policy::budget_value(store, &Config::builtin(), &b, now_ms)?;
        rows.push(json!({"scope": b.scope, "unit": b.unit, "soft": b.soft, "hard": b.hard, "action": b.action, "value": v}));
    }
    if json_out {
        return print_json(&rows);
    }
    for r in rows {
        println!(
            "{} — {} / soft {} / hard {} {} ({})",
            r["scope"],
            r["value"],
            r["soft"],
            r["hard"],
            r["unit"].as_str().unwrap_or(""),
            r["action"].as_str().unwrap_or("")
        );
    }
    Ok(())
}

pub fn link(store: &Store, terminal: &str, session_prefix: &str) -> Result<()> {
    let (sid, path) = report::resolve_session(store, session_prefix)?;
    if store.terminal(terminal)?.is_none() {
        bail!("không biết terminal {terminal} (chạy `agm daemon` hoặc `agm ps` trước)");
    }
    store.link_terminal(terminal, sid, "manual")?;
    println!("Linked {terminal} ↔ {path}.");
    Ok(())
}

// ---------- hook ----------

/// Không bao giờ lỗi: statusline phải luôn in ra một dòng.
pub fn hook_statusline(home: &Path, now: i64) -> String {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let run = || -> Result<String> {
        let v: Value = serde_json::from_str(&input)?;
        let store = Store::open(&home.join("agm.db"))?;
        agmag::hook::statusline(&store, &v, hc_terminal().as_deref(), now)
    };
    run().unwrap_or_else(|_| "agm".into())
}

pub fn hook_install(agm_exe: &Path) -> Result<()> {
    let path = dirs::home_dir()
        .context("không tìm thấy home")?
        .join(".claude")
        .join("settings.json");
    match agmag::hook::install_statusline(&path, agm_exe)? {
        agmag::hook::InstallOutcome::Added => println!("Added statusLine to {}.", path.display()),
        agmag::hook::InstallOutcome::AlreadyInstalled => {
            println!("agm statusLine is already installed in {}.", path.display())
        }
    }
    Ok(())
}
