use std::fs::File;
use std::io::{BufRead, BufReader, IsTerminal, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{Attribute, Cell, CellAlignment, Table, presets::UTF8_FULL_CONDENSED};
use serde::Serialize;

use agmag::config::Config;
use agmag::ingest::{Roots, ingest_source};
use agmag::model::Source;
use agmag::report::{
    self, GroupBy, UsageSummary, UsageWindow, compute_local_bounds, coverage_status,
    days_from_civil, render_bar, sqlite_offset_at, window_since_ms,
};
use agmag::store::Store;

mod ops;
mod tui;

#[derive(Parser)]
#[command(
    name = "agm",
    version,
    about = "Theo dõi token, hạn mức gói và hội thoại của các agent trong herdr"
)]
struct Cli {
    /// In JSON (mặc định khi đầu ra không phải terminal)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Quét log Codex/Claude/agy mới vào CSDL
    Scan {
        #[arg(long, default_value_t = 7)]
        days: i64,
        /// Quét từ ngày địa phương YYYY-MM-DD
        #[arg(long, conflicts_with = "days")]
        since: Option<String>,
    },
    /// Các phiên hoạt động gần đây
    Ls {
        #[arg(long, default_value_t = 1)]
        days: i64,
    },
    /// Tổng token (và USD cho model trả theo API) theo nhóm hoặc tóm tắt 24h/ngày/4 tháng
    Cost {
        #[arg(long, value_enum, default_value_t = GroupBy::Model)]
        by: GroupBy,
        #[arg(long, default_value_t = 1)]
        days: i64,
        /// Phạm vi thời gian theo giờ máy (loại trừ với --days)
        #[arg(long, value_enum, conflicts_with = "days")]
        window: Option<UsageWindow>,
        /// Bảng tóm tắt 24h / Hôm nay / Tháng này kèm bar chart 4 tháng gần nhất
        #[arg(long)]
        summary: bool,
    },
    /// Hạn mức gói theo cửa sổ thời gian (hiển thị phần còn lại)
    Quota {
        /// Đọc ngay hạn mức agy (`agy --print /usage`, khoảng 3 giây) trước khi in
        #[arg(long)]
        refresh: bool,
    },
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
    /// Xoay vòng phiên: OpenCode (bàn giao + kill + spawn) hoặc Claude/Codex đã quản lý (bàn giao + mở mới, không kill)
    Rotate {
        target: String,
        #[arg(long)]
        yes: bool,
    },
    /// Bật/tắt quản lý xoay vòng cho phiên Claude Code hoặc Codex mở tay
    Manage {
        target: String,
        #[arg(long)]
        off: bool,
        #[arg(long)]
        cmd: Option<String>,
        #[arg(last = true)]
        argv: Vec<String>,
    },
    /// Dọn dẹp thủ công các phiên cũ đã bàn giao (1 lần xác nhận y/N)
    Cleanup {
        targets: Vec<String>,
        #[arg(long)]
        all: bool,
        /// Compatibility flag; cleanup still requires one typed confirmation
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
    Rm {
        scope: String,
    },
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    if let Cmd::Hook {
        cmd: HookCmd::Statusline,
    } = cli.cmd
    {
        println!("{}", ops::hook_statusline(&agm_home(), now_ms()));
        return ExitCode::SUCCESS;
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agm: error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let home = agm_home();
    let cfg_path = home.join("providers.toml");
    // Chỉ nạp cấu hình khi lệnh cần: `policy off`, `kill`, `inbox`, `done` phải chạy được cả khi providers.toml hỏng.
    let cfg =
        || Config::load(&cfg_path).with_context(|| format!("đọc cấu hình {}", cfg_path.display()));
    let db_path = home.join("agm.db");
    let store = Store::open(&db_path)?;
    let json = cli.json || !std::io::stdout().is_terminal();
    let now = now_ms();

    let scan_since_ms = match &cli.cmd {
        Cmd::Scan { days, since } => Some(match since {
            Some(s) => parse_local_date_ms(s, store.conn())?,
            None => now - (*days).clamp(0, 36_500) * DAY_MS,
        }),
        Cmd::Ls { days } => Some(now - (*days).clamp(0, 36_500).max(SCAN_DAYS) * DAY_MS),
        Cmd::Cost {
            by,
            days,
            window,
            summary,
        } => {
            let cfg_loaded = cfg()?;
            let target_since = cost_target_since_ms(
                store.conn(),
                &cfg_loaded,
                *by,
                *days,
                *window,
                *summary,
                now,
            );
            let need_backfill = coverage_status(&store, target_since)?.1;
            let default_since = now - SCAN_DAYS * DAY_MS;
            Some(if need_backfill {
                target_since.min(default_since)
            } else {
                default_since
            })
        }
        Cmd::Quota { .. } | Cmd::Log { .. } => Some(now - SCAN_DAYS * DAY_MS),
        _ => None,
    };
    let stats = if let Some(since_ms) = scan_since_ms {
        scan(&store, since_ms)?
    } else {
        Default::default()
    };

    match cli.cmd {
        Cmd::Hook {
            cmd: HookCmd::Statusline,
        } => unreachable!("xử lý trong main"),
        Cmd::Hook {
            cmd: HookCmd::Install,
        } => ops::hook_install(&std::env::current_exe()?),
        Cmd::Daemon {
            replay: Some(file), ..
        } => {
            let n = ops::daemon_replay(&store, &file, now)?;
            agmag::correlate::link_all(&store)?;
            emit(json, &serde_json::json!({ "applied": n }), || {
                println!("Applied {n} events.")
            })
        }
        Cmd::Daemon {
            interval,
            replay: None,
        } => {
            let cfg = cfg()?;
            // Hạn mức agy: gọi `agy --print /usage` thưa và trên luồng riêng; lỗi chỉ cảnh báo.
            let mut agy_quota = agmag::agy_quota::AgyQuotaPoller::new(
                cfg.policy.agy_quota_poll_min,
                std::sync::Arc::new(agmag::agy_quota::fetch_usage),
            );
            ops::daemon_loop(&store, &cfg, interval, |s| {
                scan(s, now_ms() - SCAN_DAYS * DAY_MS)?;
                agy_quota.tick(s, now_ms())
            })
        }
        Cmd::Ps { all } => {
            let rows = store.terminals(!all)?;
            emit(json, &rows, || print_terminals(&rows))
        }
        Cmd::Send {
            to,
            kind,
            task,
            text,
        } => ops::send(
            &store,
            &to,
            &kind,
            task.as_deref(),
            &text.join(" "),
            json,
            now,
        ),
        Cmd::Inbox { all } => ops::inbox(&store, all, json),
        Cmd::Done { id } => {
            agmag::actions::dismiss(&store, id, now)?;
            emit(json, &serde_json::json!({ "done": id }), || {
                println!("Dismissed #{id}.")
            })
        }
        Cmd::Reply { id, text } => ops::reply(&store, id, &text.join(" "), now),
        Cmd::Spawn {
            template,
            task,
            role,
            name,
            cwd,
            text,
        } => {
            let text = (!text.is_empty()).then(|| text.join(" "));
            ops::spawn(
                &store,
                &cfg()?,
                &agmag::actions::SpawnReq {
                    template,
                    task,
                    role,
                    name,
                    cwd,
                    text,
                },
                now,
            )
        }
        Cmd::Kill { target, yes } => ops::stop(&store, &target, "kill", yes, now),
        Cmd::Interrupt { target, yes } => ops::stop(&store, &target, "interrupt", yes, now),
        Cmd::Rotate { target, yes } => ops::rotate(&store, &target, yes, now),
        Cmd::Manage {
            target,
            off,
            cmd,
            argv,
        } => ops::manage(&store, &target, off, cmd.as_deref(), &argv, now),
        Cmd::Cleanup { targets, all, yes } => ops::cleanup(&store, &targets, all, yes, json, now),
        Cmd::Budget {
            cmd:
                BudgetCmd::Set {
                    scope,
                    soft,
                    hard,
                    action,
                },
        } => ops::budget_set(&store, &scope, soft, hard, &action),
        Cmd::Budget { cmd: BudgetCmd::Ls } => ops::budget_ls(&store, json, now),
        Cmd::Budget {
            cmd: BudgetCmd::Rm { scope },
        } => {
            if !store.rm_budget(&scope)? {
                anyhow::bail!("không có ngân sách {scope}");
            }
            Ok(())
        }
        Cmd::Policy { state } => match state.as_str() {
            "status" => {
                let on = agmag::policy::enabled(&store)?;
                emit(json, &serde_json::json!({ "enabled": on }), || {
                    println!("policy: {}", if on { "ON" } else { "OFF" })
                })
            }
            s => {
                store.set_setting("policy", s)?;
                store.log_action(
                    &ops::hc_terminal().unwrap_or_else(|| "user".into()),
                    "policy",
                    s,
                    "",
                    true,
                    now,
                )?;
                emit(json, &serde_json::json!({ "enabled": s == "on" }), || {
                    println!(
                        "policy: {}",
                        if s == "on" {
                            "ON"
                        } else {
                            "OFF — no automatic actions"
                        }
                    )
                })
            }
        },
        Cmd::Link { terminal, session } => ops::link(&store, &terminal, &session),
        Cmd::Ask {
            model,
            timeout,
            question,
        } => {
            let q = question.join(" ");
            let out = agmag::ask::run_agy(&q, model.as_deref(), timeout)?;
            let text = agmag::ask::record(&store, &out, &q, model.as_deref(), now)?;
            emit(json, &out, || println!("{text}"))
        }
        Cmd::Tui { refresh } => tui::run(
            &store,
            &db_path,
            &cfg()?,
            refresh,
            |s| scan(s, now_ms() - SCAN_DAYS * DAY_MS).map(|_| ()),
            |s, since| scan(s, since).map(|_| ()),
        ),
        Cmd::Scan { .. } => emit(json, &stats_json(&stats), || {
            println!(
                "Scanned {} files, {} new lines ({} events, {} unknown lines, {} bad lines).",
                stats.files, stats.lines, stats.events, stats.unknown, stats.bad
            )
        }),
        Cmd::Ls { days } => {
            let rows = report::sessions(&store, &cfg()?, now - days.clamp(0, 36_500) * DAY_MS)?;
            emit(json, &rows, || print_sessions(&rows))
        }
        Cmd::Cost {
            by,
            days,
            window,
            summary,
        } => {
            let cfg_loaded = cfg()?;
            if summary {
                let sum = report::usage_summary(&store, &cfg_loaded, now)?;
                emit(json, &sum, || print_summary(&sum))
            } else {
                let since_ms =
                    cost_target_since_ms(store.conn(), &cfg_loaded, by, days, window, false, now);
                let rows = report::cost(&store, &cfg_loaded, by, since_ms)?;
                emit(json, &rows, || print_cost(&rows, by))
            }
        }
        Cmd::Quota { refresh } => {
            if refresh {
                // Lỗi agy (chưa cài, mất mạng, chưa đăng nhập) không chặn việc in hạn mức Codex/Claude.
                match agmag::agy_quota::fetch_usage()
                    .and_then(|out| agmag::agy_quota::record_usage(&store, &out, now))
                {
                    Ok(p) if p.unknown > 0 => eprintln!(
                        "agm: skipped {} unrecognized agy quota bucket(s)",
                        p.unknown
                    ),
                    Ok(_) => {}
                    Err(e) => eprintln!("agm: could not read agy quota: {e:#}"),
                }
            }
            let rows = report::quota(&store, now / 1000)?;
            emit(json, &rows, || print_quota(&rows, now / 1000))
        }
        Cmd::Log {
            session,
            limit,
            full,
        } => {
            let turns = report::turns(&store, &session, limit)?;
            if full {
                let (_, path) = report::resolve_session(&store, &session)?;
                if path.starts_with("opencode://") || path.starts_with("agy://") {
                    anyhow::bail!(
                        "--full chỉ dùng cho phiên Codex/Claude (log jsonl); phiên này lưu trong CSDL của {}",
                        path.split(':').next().unwrap_or("")
                    );
                }
                let raw = turns
                    .iter()
                    .map(|t| raw_line(&path, t.byte_offset))
                    .collect::<Result<Vec<_>>>()?;
                emit(true, &raw, || {})
            } else {
                emit(json, &turns, || {
                    let mut t = table(&["time (UTC)", "role", "summary"]);
                    for turn in &turns {
                        t.add_row(vec![
                            hhmm(turn.ts_ms),
                            format!("{:?}", turn.role).to_lowercase(),
                            turn.summary.clone(),
                        ]);
                    }
                    println!("{t}");
                })
            }
        }
    }
}

fn parse_local_date_ms(s: &str, conn: &rusqlite::Connection) -> Result<i64> {
    let parts: Vec<&str> = s.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        anyhow::bail!("ngày '{s}' không đúng định dạng YYYY-MM-DD");
    };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i64>(), m.parse::<i64>(), d.parse::<i64>()) else {
        anyhow::bail!("ngày '{s}' không đúng định dạng YYYY-MM-DD");
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        anyhow::bail!("ngày '{s}' không hợp lệ");
    }
    let mid_s = days_from_civil(y, m, d) * 86_400;
    let mut off = sqlite_offset_at(conn);
    let o = off(mid_s);
    let o2 = off(mid_s - o);
    Ok((mid_s - o2) * 1000)
}

/// Quét log Codex/Claude/agy (theo tệp) và OpenCode (theo CSDL). Một nguồn lỗi không làm hỏng nguồn khác.
/// Chỉ tiến mốc `covered_since` cho từng nguồn tệp khi nguồn đó quét xong với `skipped == 0`.
fn scan(store: &Store, since_ms: i64) -> Result<agmag::ingest::IngestStats> {
    let r = roots();
    let mut stats = agmag::ingest::IngestStats::default();
    for (src_name, src, root) in [
        ("codex", Source::Codex, &r.codex),
        ("claude", Source::Claude, &r.claude),
    ] {
        let s = ingest_source(store, src, root, since_ms)?;
        if s.skipped == 0 {
            store.update_covered_since(src_name, since_ms)?;
        }
        stats.add(&s);
    }
    match agmag::opencode::ingest_opencode(store, &agmag::opencode::default_db()) {
        Ok(oc) => stats.add(&oc),
        Err(err) => {
            eprintln!("agm: skipped opencode.db: {err:#}");
            stats.skipped += 1;
        }
    }
    match agmag::agy::ingest_agy(store, &agmag::agy::default_dir(), since_ms) {
        Ok(ag) => {
            if ag.skipped == 0 {
                store.update_covered_since("agy", since_ms)?;
            }
            stats.add(&ag);
        }
        Err(err) => {
            eprintln!("agm: skipped agy: {err:#}");
            stats.skipped += 1;
        }
    }
    Ok(stats)
}

fn print_terminals(rows: &[agmag::store::TerminalRow]) {
    let mut t = table(&[
        "terminal", "name", "agent", "status", "task", "role", "session", "where",
    ]);
    for r in rows {
        t.add_row(vec![
            r.address.clone(),
            r.name.clone().unwrap_or_default(),
            r.app.clone().unwrap_or_else(|| "?".into()),
            format!(
                "{}{}",
                r.status.clone().unwrap_or_else(|| r.state.clone()),
                r.kind
                    .as_deref()
                    .map(|k| format!(" ({k})"))
                    .unwrap_or_default()
            ),
            r.task.clone().unwrap_or_default(),
            r.role.clone().unwrap_or_default(),
            r.session
                .map(|s| format!("#{s} {}", r.link_source.as_deref().unwrap_or("")))
                .unwrap_or_default(),
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
    t.load_style(UTF8_FULL_CONDENSED)
        .set_header(header.to_vec());
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
    let mut t = table(&[
        "source",
        "session",
        "model",
        "tokens",
        "context",
        "last (UTC)",
        "where",
        "warning",
    ]);
    for r in rows {
        t.add_row(vec![
            r.source.as_str().to_string(),
            r.session_id
                .as_deref()
                .unwrap_or("?")
                .chars()
                .take(8)
                .collect(),
            r.model.clone().unwrap_or_else(|| "?".into()),
            human_tokens(r.usage.total()),
            r.context_pct
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "?".into()),
            r.last_ms.map(hhmm).unwrap_or_else(|| "?".into()),
            r.cwd.clone().unwrap_or_else(|| "?".into()),
            r.warning.clone().unwrap_or_default(),
        ]);
    }
    println!("{t}");
}

const USD_NOTE: &str = "USD = tokens × each model's list API price (built-in table, or [models.\"<id>\".price] in \
providers.toml). Subscription plans (codex, claude, agy, opencode-go) are not billed per token: this is what the \
same usage would cost on the API. ≥ = some tokens have no known price; ? = no price.";

fn print_cost(rows: &[report::CostRow], by: GroupBy) {
    let key = match by {
        GroupBy::Model => "model",
        GroupBy::Day => "day (local)",
        GroupBy::Month => "month (local)",
        GroupBy::Source => "source",
        GroupBy::Cwd => "where",
    };
    let with_bar = matches!(by, GroupBy::Day | GroupBy::Month);
    let max_tok = rows.iter().map(|r| r.usage.total()).max().unwrap_or(0);
    let mut header = vec![key, "input", "cache read", "cache write", "output", "total"];
    if with_bar {
        header.push("chart (tokens)");
    }
    header.push("USD");
    let mut t = table(&header);
    let cells = |r: &report::CostRow, bar: Option<String>| {
        let mut row = vec![
            r.key.clone(),
            human_tokens(r.usage.input_uncached),
            human_tokens(r.usage.cache_read),
            human_tokens(r.usage.cache_write),
            human_tokens(r.usage.output),
            human_tokens(r.usage.total()),
        ];
        row.extend(bar);
        row.push(report::value_label(r.value_usd, r.unpriced_tokens));
        row
    };
    for r in rows {
        t.add_row(cells(
            r,
            with_bar.then(|| render_bar(r.usage.total(), max_tok, 16, false)),
        ));
    }
    // SUM line: every numeric column added up, in bold.
    let sum = report::cost_totals(rows);
    t.add_row(
        cells(&sum, with_bar.then(String::new))
            .into_iter()
            .map(|c| Cell::new(c).add_attribute(Attribute::Bold))
            .collect::<Vec<_>>(),
    );
    // Numbers read better right-aligned (every column except the key and the chart).
    for idx in (1..header.len()).filter(|&i| !(with_bar && i == 6)) {
        if let Some(col) = t.column_mut(idx) {
            col.set_cell_alignment(CellAlignment::Right);
        }
    }
    println!("{t}");
    println!("{USD_NOTE}");
}

fn cost_target_since_ms(
    conn: &rusqlite::Connection,
    cfg: &Config,
    by: GroupBy,
    days: i64,
    window: Option<report::UsageWindow>,
    summary: bool,
    now: i64,
) -> i64 {
    let mut off = sqlite_offset_at(conn);
    if summary {
        return compute_local_bounds(now, cfg.policy.stats_months, &mut off).four_months_start_ms;
    }
    if let Some(w) = window {
        return window_since_ms(w, now, cfg.policy.stats_months, &mut off);
    }
    if by == GroupBy::Month {
        return compute_local_bounds(now, cfg.policy.stats_months, &mut off).four_months_start_ms;
    }
    now - days.clamp(0, 36_500) * DAY_MS
}

fn print_summary(sum: &UsageSummary) {
    println!("USAGE ({})", sum.offset_label);
    let mut t = table(&[
        "window",
        "input",
        "cache read",
        "cache write",
        "output",
        "total",
        "USD",
        "billed (API)",
    ]);
    for (label, b) in [
        ("Last 24h", &sum.last_24h),
        ("Today", &sum.today),
        ("This month", &sum.this_month),
    ] {
        let priced = b.usage.total() > b.unpriced_tokens;
        t.add_row(vec![
            label.into(),
            human_tokens(b.usage.input_uncached),
            human_tokens(b.usage.cache_read),
            human_tokens(b.usage.cache_write),
            human_tokens(b.usage.output),
            human_tokens(b.usage.total()),
            report::value_label(priced.then_some(b.value_usd), b.unpriced_tokens),
            b.api_usd
                .map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "—".into()),
        ]);
    }
    for idx in 1..8 {
        if let Some(col) = t.column_mut(idx) {
            col.set_cell_alignment(CellAlignment::Right);
        }
    }
    println!("{t}");

    println!(
        "\nLAST {} MONTHS (total tokens, USD at list prices)",
        sum.recent_months.len()
    );
    let max_m = sum
        .recent_months
        .iter()
        .map(|r| r.usage.total())
        .max()
        .unwrap_or(0);
    for r in &sum.recent_months {
        let bar = render_bar(r.usage.total(), max_m, 20, false);
        let usd = if r.usage.total() == 0 {
            String::new()
        } else {
            format!("  {}", report::value_label(r.value_usd, r.unpriced_tokens))
        };
        println!(
            "  {}  {}  {:>6}{usd}",
            r.key,
            bar,
            human_tokens(r.usage.total())
        );
    }
    println!("\n{USD_NOTE}");
    if sum.incomplete_coverage {
        println!(
            "\n! Older log data may be incomplete since {} (run `agm scan --since {}`).",
            sum.four_months_start_day, sum.four_months_start_day,
        );
    }
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
    // Hiển thị phần CÒN LẠI (giảm dần từ 100%); JSON vẫn có cả `used_percent` lẫn `left_percent`.
    let mut t = table(&["plan", "window", "left", "", "reset"]);
    for r in rows {
        let left = if r.left_percent.is_finite() {
            r.left_percent
        } else {
            0.0
        };
        let filled = ((left / 10.0).round() as usize).min(10);
        let reset = if r.reset_passed {
            "reset passed".to_string()
        } else {
            let left = r.resets_at_s - now_s;
            format!("in {}h{:02}m", left / 3600, left % 3600 / 60)
        };
        t.add_row(vec![
            r.provider.clone(),
            window_label(r.window_minutes),
            format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled)),
            if r.left_percent.is_finite() {
                format!("{:.0}%", r.left_percent)
            } else {
                "?".into()
            },
            reset,
        ]);
    }
    println!("{t}");
}
