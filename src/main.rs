use std::fs::File;
use std::io::{BufRead, BufReader, IsTerminal, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use serde::Serialize;

use agmag::config::{Billing, Config};
use agmag::ingest::{ingest_all, Roots};
use agmag::model::Usage;
use agmag::report::{self, GroupBy};
use agmag::store::Store;

mod ops;
mod tui;

#[derive(Parser)]
#[command(name = "agm", version, about = "Theo dõi token, hạn mức gói và hội thoại của các agent trong herdr")]
struct Cli {
    /// In JSON (mặc định khi đầu ra không phải terminal)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Quét log Codex/Claude mới vào CSDL
    Scan {
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
    /// Các phiên hoạt động gần đây
    Ls {
        #[arg(long, default_value_t = 1)]
        days: i64,
    },
    /// Tổng token (và USD cho model trả theo API) theo nhóm
    Cost {
        #[arg(long, value_enum, default_value_t = GroupBy::Model)]
        by: GroupBy,
        #[arg(long, default_value_t = 1)]
        days: i64,
    },
    /// Hạn mức gói theo cửa sổ thời gian
    Quota,
    /// Các lượt hội thoại của một phiên (tiền tố session id)
    Log {
        session: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// In nguyên dòng log gốc của từng lượt
        #[arg(long)]
        full: bool,
    },
    /// Theo dõi nền: sự kiện hc, quét log, ghép terminal, ngân sách, xoay vòng thợ
    Daemon {
        #[arg(long, default_value_t = 10)]
        interval: u64,
        /// Nạp sự kiện hc từ tệp (mỗi dòng một JSON) rồi thoát — để gỡ lỗi
        #[arg(long)]
        replay: Option<PathBuf>,
    },
    /// Các terminal của herdr-core mà agmag biết
    Ps {
        #[arg(long)]
        all: bool,
    },
    /// Gửi tin theo mẫu (GIAO, BÁO CÁO, LỖI, HỎI, CHẶN, ĐẠT, CHƯA ĐẠT); đích `user` → hộp thư
    Send {
        to: String,
        #[arg(long = "type")]
        kind: String,
        #[arg(long)]
        task: Option<String>,
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Hộp thư người dùng: agent bị chặn, câu hỏi, đề nghị, cảnh báo
    Inbox {
        #[arg(long)]
        all: bool,
    },
    /// Đóng một mục hộp thư (không gõ gì vào terminal nào)
    Done { id: i64 },
    /// Trả lời một mục hộp thư (gõ vào terminal liên quan)
    Reply {
        id: i64,
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Sinh agent qua hc, có kiểm hạn mức thợ và ngân sách
    Spawn {
        template: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value = "worker")]
        role: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(last = true)]
        text: Vec<String>,
    },
    /// Kết thúc một terminal (Codex/Claude: chỉ người dùng, có xác nhận)
    Kill {
        target: String,
        #[arg(long)]
        yes: bool,
    },
    /// Ngắt lượt đang chạy (cùng quy tắc với kill)
    Interrupt {
        target: String,
        #[arg(long)]
        yes: bool,
    },
    /// Xoay vòng một thợ OpenCode: bàn giao → kill → phiên mới cùng model
    Rotate {
        target: String,
        #[arg(long)]
        yes: bool,
    },
    /// Ngân sách: task:<ID> (token) hoặc provider:<tên>:<phút> (% hạn mức)
    Budget {
        #[command(subcommand)]
        cmd: BudgetCmd,
    },
    /// Bật/tắt mọi hành động tự động (nút dừng khẩn)
    Policy {
        #[arg(value_parser = ["on", "off", "status"])]
        state: String,
    },
    /// Ghép tay terminal ↔ phiên
    Link { terminal: String, session: String },
    /// Hỏi agy (Gemini) một lần, chỉ đọc; ghi lại token
    Ask {
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
        #[arg(required = true, num_args = 1..)]
        question: Vec<String>,
    },
    /// Hook cho Claude Code
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    /// Giao diện theo dõi trong một pane của herdr
    Tui {
        #[arg(long, default_value_t = 2)]
        refresh: u64,
    },
}

#[derive(Subcommand)]
enum BudgetCmd {
    Set {
        scope: String,
        #[arg(long)]
        soft: f64,
        #[arg(long)]
        hard: f64,
        #[arg(long, default_value = "notify")]
        action: String,
    },
    Ls,
    Rm { scope: String },
}

#[derive(Subcommand)]
enum HookCmd {
    /// Đọc JSON statusline từ stdin, ghi hạn mức/ngữ cảnh, in một dòng trạng thái
    Statusline,
    /// Thêm statusLine vào ~/.claude/settings.json (không ghi đè cái đã có)
    Install,
}

const SCAN_DAYS: i64 = 7;
const DAY_MS: i64 = 86_400_000;

pub(crate) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn agm_home() -> PathBuf {
    std::env::var_os("AGM_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".agmag"))
}

fn roots() -> Roots {
    let mut r = Roots::from_home();
    if let Some(p) = std::env::var_os("AGM_CODEX_DIR") {
        r.codex = PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("AGM_CLAUDE_DIR") {
        r.claude = PathBuf::from(p);
    }
    r
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Cmd::Hook { cmd: HookCmd::Statusline } = cli.cmd {
        println!("{}", ops::hook_statusline(&agm_home(), now_ms()));
        return ExitCode::SUCCESS;
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agm: lỗi: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let home = agm_home();
    let cfg_path = home.join("providers.toml");
    // Chỉ nạp cấu hình khi lệnh cần: `policy off`, `kill`, `inbox`, `done` phải chạy được cả khi providers.toml hỏng.
    let cfg = || Config::load(&cfg_path).with_context(|| format!("đọc cấu hình {}", cfg_path.display()));
    let store = Store::open(&home.join("agm.db"))?;
    let json = cli.json || !std::io::stdout().is_terminal();
    let now = now_ms();

    let reads = matches!(cli.cmd, Cmd::Scan { .. } | Cmd::Ls { .. } | Cmd::Cost { .. } | Cmd::Quota | Cmd::Log { .. });
    let scan_days = match cli.cmd {
        Cmd::Scan { days } => days.clamp(0, 36_500),
        Cmd::Ls { days } | Cmd::Cost { days, .. } => days.clamp(0, 36_500).max(SCAN_DAYS),
        _ => SCAN_DAYS,
    };
    let stats = if reads { scan(&store, now - scan_days * DAY_MS)? } else { Default::default() };

    match cli.cmd {
        Cmd::Hook { cmd: HookCmd::Statusline } => unreachable!("xử lý trong main"),
        Cmd::Hook { cmd: HookCmd::Install } => ops::hook_install(&std::env::current_exe()?),
        Cmd::Daemon { replay: Some(file), .. } => {
            let n = ops::daemon_replay(&store, &file, now)?;
            agmag::correlate::link_all(&store)?;
            emit(json, &serde_json::json!({ "applied": n }), || println!("Đã nạp {n} sự kiện."))
        }
        Cmd::Daemon { interval, replay: None } => {
            ops::daemon_loop(&store, &cfg()?, interval, |s| scan(s, now_ms() - SCAN_DAYS * DAY_MS).map(|_| ()))
        }
        Cmd::Ps { all } => {
            let rows = store.terminals(!all)?;
            emit(json, &rows, || print_terminals(&rows))
        }
        Cmd::Send { to, kind, task, text } => ops::send(&store, &to, &kind, task.as_deref(), &text.join(" "), json, now),
        Cmd::Inbox { all } => ops::inbox(&store, all, json),
        Cmd::Done { id } => {
            agmag::actions::dismiss(&store, id, now)?;
            emit(json, &serde_json::json!({ "done": id }), || println!("Đã đóng mục #{id}."))
        }
        Cmd::Reply { id, text } => ops::reply(&store, id, &text.join(" "), now),
        Cmd::Spawn { template, task, role, name, cwd, text } => {
            let text = (!text.is_empty()).then(|| text.join(" "));
            ops::spawn(&store, &cfg()?, &agmag::actions::SpawnReq { template, task, role, name, cwd, text }, now)
        }
        Cmd::Kill { target, yes } => ops::stop(&store, &target, "kill", yes, now),
        Cmd::Interrupt { target, yes } => ops::stop(&store, &target, "interrupt", yes, now),
        Cmd::Rotate { target, yes } => ops::rotate(&store, &target, yes, now),
        Cmd::Budget { cmd: BudgetCmd::Set { scope, soft, hard, action } } => ops::budget_set(&store, &scope, soft, hard, &action),
        Cmd::Budget { cmd: BudgetCmd::Ls } => ops::budget_ls(&store, json),
        Cmd::Budget { cmd: BudgetCmd::Rm { scope } } => {
            if !store.rm_budget(&scope)? {
                anyhow::bail!("không có ngân sách {scope}");
            }
            Ok(())
        }
        Cmd::Policy { state } => match state.as_str() {
            "status" => {
                let on = agmag::policy::enabled(&store)?;
                emit(json, &serde_json::json!({ "enabled": on }), || println!("policy: {}", if on { "bật" } else { "TẮT" }))
            }
            s => {
                store.set_setting("policy", s)?;
                store.log_action(&ops::hc_terminal().unwrap_or_else(|| "user".into()), "policy", s, "", true, now)?;
                emit(json, &serde_json::json!({ "enabled": s == "on" }), || {
                    println!("policy: {}", if s == "on" { "bật" } else { "TẮT — không còn hành động tự động" })
                })
            }
        },
        Cmd::Link { terminal, session } => ops::link(&store, &terminal, &session),
        Cmd::Ask { model, timeout, question } => {
            let q = question.join(" ");
            let out = agmag::ask::run_agy(&q, model.as_deref(), timeout)?;
            let text = agmag::ask::record(&store, &out, &q, model.as_deref(), now)?;
            emit(json, &out, || println!("{text}"))
        }
        Cmd::Tui { refresh } => tui::run(&store, &cfg()?, refresh, |s| scan(s, now_ms() - SCAN_DAYS * DAY_MS).map(|_| ())),
        Cmd::Scan { .. } => emit(json, &stats_json(&stats), || {
            println!(
                "Đã quét {} tệp, {} dòng mới ({} sự kiện, {} dòng loại lạ, {} dòng hỏng).",
                stats.files, stats.lines, stats.events, stats.unknown, stats.bad
            )
        }),
        Cmd::Ls { days } => {
            let rows = report::sessions(&store, &cfg()?, now - days.clamp(0, 36_500) * DAY_MS)?;
            emit(json, &rows, || print_sessions(&rows))
        }
        Cmd::Cost { by, days } => {
            let rows = report::cost(&store, &cfg()?, by, now - days.clamp(0, 36_500) * DAY_MS)?;
            emit(json, &rows, || print_cost(&rows, by))
        }
        Cmd::Quota => {
            let rows = report::quota(&store, now / 1000)?;
            emit(json, &rows, || print_quota(&rows, now / 1000))
        }
        Cmd::Log { session, limit, full } => {
            let turns = report::turns(&store, &session, limit)?;
            if full {
                let (_, path) = report::resolve_session(&store, &session)?;
                if path.starts_with("opencode://") || path.starts_with("agy://") {
                    anyhow::bail!("--full chỉ dùng cho phiên Codex/Claude (log jsonl); phiên này lưu trong CSDL của {}", path.split(':').next().unwrap_or(""));
                }
                let raw = turns
                    .iter()
                    .map(|t| raw_line(&path, t.byte_offset))
                    .collect::<Result<Vec<_>>>()?;
                emit(true, &raw, || {})
            } else {
                emit(json, &turns, || {
                    let mut t = table(&["giờ (UTC)", "vai", "tóm tắt"]);
                    for turn in &turns {
                        t.add_row(vec![hhmm(turn.ts_ms), format!("{:?}", turn.role).to_lowercase(), turn.summary.clone()]);
                    }
                    println!("{t}");
                })
            }
        }
    }
}

/// Quét log Codex/Claude (theo tệp) và OpenCode (theo CSDL). Một nguồn lỗi không làm hỏng nguồn khác.
fn scan(store: &Store, since_ms: i64) -> Result<agmag::ingest::IngestStats> {
    let mut stats = ingest_all(store, &roots(), since_ms)?;
    match agmag::opencode::ingest_opencode(store, &agmag::opencode::default_db()) {
        Ok(oc) => {
            stats.files += oc.files;
            stats.lines += oc.lines;
            stats.events += oc.events;
            stats.bad += oc.bad;
        }
        Err(err) => {
            eprintln!("agm: bỏ qua opencode.db: {err:#}");
            stats.skipped += 1;
        }
    }
    Ok(stats)
}

fn print_terminals(rows: &[agmag::store::TerminalRow]) {
    let mut t = table(&["terminal", "tên", "app", "trạng thái", "task", "vai", "phiên", "thư mục"]);
    for r in rows {
        t.add_row(vec![
            r.address.clone(),
            r.name.clone().unwrap_or_default(),
            r.app.clone().unwrap_or_else(|| "?".into()),
            format!("{}{}", r.status.clone().unwrap_or_else(|| r.state.clone()), r.kind.as_deref().map(|k| format!(" ({k})")).unwrap_or_default()),
            r.task.clone().unwrap_or_default(),
            r.role.clone().unwrap_or_default(),
            r.session.map(|s| format!("#{s} {}", r.link_source.as_deref().unwrap_or(""))).unwrap_or_default(),
            r.cwd.clone().unwrap_or_default(),
        ]);
    }
    println!("{t}");
}

fn emit<T: Serialize + ?Sized>(json: bool, value: &T, human: impl FnOnce()) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        human();
    }
    Ok(())
}

fn stats_json(s: &agmag::ingest::IngestStats) -> serde_json::Value {
    serde_json::json!({"files": s.files, "lines": s.lines, "events": s.events, "unknown": s.unknown, "bad": s.bad, "skipped": s.skipped})
}

/// Đọc nguyên một dòng log tại vị trí byte (chỉ đọc).
fn raw_line(path: &str, offset: u64) -> Result<serde_json::Value> {
    let mut f = File::open(path).with_context(|| format!("mở {path}"))?;
    f.seek(SeekFrom::Start(offset))?;
    let mut line = String::new();
    BufReader::new(f).read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim_end()).unwrap_or(serde_json::Value::String(line)))
}

fn table(header: &[&str]) -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED).set_header(header.to_vec());
    t
}

fn human_tokens(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

fn hhmm(ts_ms: i64) -> String {
    let s = ts_ms.div_euclid(1000).rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
}

fn print_sessions(rows: &[report::SessionRow]) {
    let mut t = table(&["nguồn", "phiên", "model", "token", "ngữ cảnh", "lần cuối (UTC)", "thư mục", "cảnh báo"]);
    for r in rows {
        t.add_row(vec![
            r.source.as_str().to_string(),
            r.session_id.as_deref().unwrap_or("?").chars().take(8).collect(),
            r.model.clone().unwrap_or_else(|| "?".into()),
            human_tokens(r.usage.total()),
            r.context_pct.map(|p| format!("{p}%")).unwrap_or_else(|| "?".into()),
            r.last_ms.map(hhmm).unwrap_or_else(|| "?".into()),
            r.cwd.clone().unwrap_or_else(|| "?".into()),
            r.warning.clone().unwrap_or_default(),
        ]);
    }
    println!("{t}");
}

fn usd_cell(billing: Option<Billing>, cost: Option<f64>) -> String {
    match (billing, cost) {
        (_, Some(c)) => format!("${c:.2}"),
        (Some(Billing::Subscription), None) => "— (gói tháng)".into(),
        _ => "?".into(),
    }
}

fn print_cost(rows: &[report::CostRow], by: GroupBy) {
    let key = match by {
        GroupBy::Model => "model",
        GroupBy::Day => "ngày (UTC)",
        GroupBy::Source => "nguồn",
        GroupBy::Cwd => "thư mục",
    };
    let mut t = table(&[key, "input", "cache đọc", "cache ghi", "output", "tổng", "USD"]);
    let mut total = Usage::default();
    for r in rows {
        total.add(&r.usage);
        t.add_row(vec![
            r.key.clone(),
            human_tokens(r.usage.input_uncached),
            human_tokens(r.usage.cache_read),
            human_tokens(r.usage.cache_write),
            human_tokens(r.usage.output),
            human_tokens(r.usage.total()),
            usd_cell(r.billing, r.cost_usd),
        ]);
    }
    t.add_row(vec!["TỔNG".into(), String::new(), String::new(), String::new(), String::new(), human_tokens(total.total()), String::new()]);
    println!("{t}");
}

fn window_label(minutes: u32) -> String {
    if minutes.is_multiple_of(1440) {
        format!("{}d", minutes / 1440)
    } else if minutes.is_multiple_of(60) {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn print_quota(rows: &[report::QuotaRow], now_s: i64) {
    let mut t = table(&["gói", "cửa sổ", "đã dùng", "", "reset"]);
    for r in rows {
        let filled = ((r.used_percent / 10.0).round() as usize).min(10);
        let reset = if r.reset_passed {
            "đã reset".to_string()
        } else {
            let left = r.resets_at_s - now_s;
            format!("còn {}h{:02}m", left / 3600, left % 3600 / 60)
        };
        t.add_row(vec![
            r.provider.clone(),
            window_label(r.window_minutes),
            format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled)),
            format!("{:.0}%", r.used_percent),
            reset,
        ]);
    }
    println!("{t}");
}
