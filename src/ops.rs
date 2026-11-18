//! Các lệnh v1 của binary `agm`: daemon, send, inbox, reply, spawn, kill, interrupt, rotate, budget, policy, link, ask, hook, ps.

use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use agmag::actions::{self, classify_caller, Caller, SpawnReq};
use agmag::config::Config;
use agmag::git::GitCli;
use agmag::hc::HcCli;
use agmag::store::{BudgetRow, Store};
use agmag::{correlate, daemon, policy, report, rotation};

pub fn hc_terminal() -> Option<String> {
    std::env::var("HC_TERMINAL_ID").ok().filter(|s| !s.is_empty())
}

/// Ai đang gọi: người dùng (TTY, terminal gọi không phải agent) hay agent. Hỏi y/N khi cần xác nhận.
/// Fail-closed (`actions::detect_caller`): không chắc thì là agent.
pub fn caller(store: &Store, yes: bool, needs_confirm: Option<&str>) -> Result<Caller> {
    let term = hc_terminal();
    let tty = std::io::stdin().is_terminal();
    let first = actions::detect_caller(store, &HcCli::default(), term.as_deref(), tty, yes);
    let Caller::User { confirmed } = first else { return Ok(first) };
    if confirmed {
        return Ok(first);
    }
    let Some(question) = needs_confirm else { return Ok(first) };
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(classify_caller(term.as_deref(), false, tty, matches!(line.trim(), "y" | "Y" | "yes" | "có")))
}

pub fn print_json(v: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
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
pub fn daemon_loop(store: &Store, cfg: &Config, interval: u64, mut periodic: impl FnMut(&Store) -> Result<()>) -> Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    let hc_bin = std::env::var("AGM_HC_BIN").unwrap_or_else(|_| "hc".into());
    std::thread::spawn(move || loop {
        match Command::new(&hc_bin).args(["events", "--json"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
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
    });
    let hc = HcCli::default();
    let git = GitCli;
    eprintln!("agm daemon: đang theo dõi (chu kỳ {interval}s). Ctrl+C để dừng.");
    let mut last: Option<std::time::Instant> = None;
    loop {
        let wait = last.map_or(Duration::ZERO, |l| Duration::from_secs(interval).saturating_sub(l.elapsed()));
        match rx.recv_timeout(wait) {
            Ok(line) => {
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && let Err(e) = daemon::apply_event(store, &v, crate::now_ms()) {
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

pub fn send(store: &Store, to: &str, kind: &str, task: Option<&str>, text: &str, json_out: bool, now: i64) -> Result<()> {
    let from = hc_terminal().unwrap_or_else(|| "user".into());
    let m = actions::send(store, &HcCli::default(), &from, to, kind, task, text, now)?;
    if json_out {
        print_json(&m)?;
    } else {
        println!("Tin #{} → {}: {}{}", m.id, m.to, m.status, m.detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default());
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
        println!("Hộp thư trống.");
    }
    for r in rows {
        let done = if r.resolved_ms.is_some() { " (đã xử lý)" } else { "" };
        println!("#{} [{}{}] {} — {}{done}", r.id, r.source, r.kind.map(|k| format!("/{k}")).unwrap_or_default(), r.terminal.unwrap_or_else(|| "-".into()), r.text);
    }
    Ok(())
}

pub fn reply(store: &Store, id: i64, text: &str, now: i64) -> Result<()> {
    actions::reply(store, &HcCli::default(), id, text, now)?;
    println!("Đã trả lời mục #{id}.");
    Ok(())
}

// ---------- sinh / dừng agent ----------

pub fn spawn(store: &Store, cfg: &Config, req: &SpawnReq, now: i64) -> Result<()> {
    let addr = actions::spawn(store, &HcCli::default(), cfg, req, now)?;
    println!("{addr}");
    Ok(())
}

pub fn stop(store: &Store, target: &str, verb: &str, yes: bool, now: i64) -> Result<()> {
    let app = store.terminal(target)?.and_then(|t| t.app).unwrap_or_else(|| "?".into());
    // Codex/Claude (hoặc chưa rõ app): luôn phải gõ y — `--yes` không đủ, vì agent có thể tự thêm cờ.
    let yes = yes && app == "opencode";
    let c = caller(store, yes, Some(&format!("{verb} {target} ({app})?")))?;
    let hc = HcCli::default();
    match verb {
        "kill" => actions::kill(store, &hc, &c, target, now)?,
        _ => actions::interrupt(store, &hc, &c, target, now)?,
    }
    println!("Đã {verb} {target}.");
    Ok(())
}

pub fn rotate(store: &Store, target: &str, yes: bool, now: i64) -> Result<()> {
    let Some(t) = store.terminal(target)? else { bail!("không biết terminal {target}") };
    // Danh sách trắng: chỉ thợ OpenCode được xoay vòng (bước cuối là kill phiên cũ).
    if t.app.as_deref() != Some("opencode") {
        // Xoay vòng kết thúc bằng kill phiên cũ: Codex/Claude chỉ khi người dùng ra lệnh, và v1 chỉ tự xoay OpenCode.
        let c = caller(store, yes, None)?;
        bail!(
            "xoay vòng chỉ áp dụng cho thợ OpenCode; {target} là {} — hãy tự bàn giao rồi `agm kill {target}`{}",
            t.app.unwrap_or_else(|| "app chưa rõ".into()),
            if matches!(c, Caller::Agent(_)) { " (chỉ người dùng được kill)" } else { "" }
        );
    }
    if rotation::start(store, &t.address, None, true, now)? {
        println!("Đã xếp {target} vào hàng xoay vòng; daemon sẽ yêu cầu bàn giao khi agent rảnh.");
    } else {
        println!("{target} đang trong một vòng xoay.");
    }
    Ok(())
}

// ---------- ngân sách, policy, ghép tay ----------

pub fn budget_set(store: &Store, scope: &str, soft: f64, hard: f64, action: &str) -> Result<()> {
    let unit = match scope.split(':').collect::<Vec<_>>().as_slice() {
        ["task", id] if !id.is_empty() => "tokens",
        ["provider", p, w] if !p.is_empty() && w.parse::<u32>().is_ok() => "percent",
        _ => bail!("phạm vi '{scope}' không hợp lệ: dùng task:<ID> hoặc provider:<tên>:<phút cửa sổ> (vd provider:codex:10080)"),
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
    store.set_budget(&BudgetRow { scope: scope.into(), unit: unit.into(), soft, hard, action: action.into() })?;
    println!("Đã đặt ngân sách {scope}: mềm {soft}, cứng {hard} {unit}, hành động {action}.");
    Ok(())
}

pub fn budget_ls(store: &Store, json_out: bool) -> Result<()> {
    let mut rows = Vec::new();
    for b in store.budgets()? {
        let v = policy::budget_value(store, &b)?;
        rows.push(json!({"scope": b.scope, "unit": b.unit, "soft": b.soft, "hard": b.hard, "action": b.action, "value": v}));
    }
    if json_out {
        return print_json(&rows);
    }
    for r in rows {
        println!("{} — {} / mềm {} / cứng {} {} ({})", r["scope"], r["value"], r["soft"], r["hard"], r["unit"].as_str().unwrap_or(""), r["action"].as_str().unwrap_or(""));
    }
    Ok(())
}

pub fn link(store: &Store, terminal: &str, session_prefix: &str) -> Result<()> {
    let (sid, path) = report::resolve_session(store, session_prefix)?;
    if store.terminal(terminal)?.is_none() {
        bail!("không biết terminal {terminal} (chạy `agm daemon` hoặc `agm ps` trước)");
    }
    store.link_terminal(terminal, sid, "manual")?;
    println!("Đã ghép {terminal} ↔ {path}.");
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
    let path = dirs::home_dir().context("không tìm thấy home")?.join(".claude").join("settings.json");
    let mut v: Value = match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).with_context(|| format!("{} không phải JSON hợp lệ", path.display()))?,
        // Chỉ "chưa có tệp" mới bắt đầu từ rỗng; lỗi khác (bị khóa, không có quyền) → dừng, không ghi đè cấu hình.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e).with_context(|| format!("đọc {}", path.display())),
    };
    if !v.is_object() {
        bail!("{} không phải một đối tượng JSON", path.display());
    }
    if !v["statusLine"].is_null() {
        bail!("{} đã có statusLine; agmag không ghi đè. Gọi `agm hook statusline` từ script hiện có của bạn.", path.display());
    }
    v["statusLine"] = json!({"type": "command", "command": format!("\"{}\" hook statusline", agm_exe.display())});
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&v)?)?;
    println!("Đã thêm statusLine vào {}.", path.display());
    Ok(())
}
