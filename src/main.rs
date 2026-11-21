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
    about = "Track agent tokens, subscription quotas, and conversations in herdr"
)]
struct Cli {
    /// Print JSON (the default when stdout is not a terminal)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Import new Codex/Claude/agy logs and OpenCode usage into the database
    Scan {
        /// Number of days of logs to scan
        #[arg(long, default_value_t = 7)]
        days: i64,
        /// Scan from a local date (YYYY-MM-DD)
        #[arg(long, conflicts_with = "days")]
        since: Option<String>,
    },
    /// List recently active sessions
    Ls {
        /// Number of days of session activity to include
        #[arg(long, default_value_t = 1)]
        days: i64,
    },
    /// Group token totals and USD values (API-priced models) or show a usage summary
    Cost {
        /// Group totals by model, local day, local month, source, or working directory
        #[arg(long, value_enum, default_value_t = GroupBy::Model)]
        by: GroupBy,
        /// Number of days of usage to include
        #[arg(long, default_value_t = 1)]
        days: i64,
        /// Time window in local time (mutually exclusive with --days)
        #[arg(long, value_enum, conflicts_with = "days")]
        window: Option<UsageWindow>,
        /// Show a summary table for 24h/today/this month and a 4-month bar chart
        #[arg(long)]
        summary: bool,
    },
    /// Show subscription quotas by time window (displayed as remaining)
    Quota {
        /// Refresh agy, Codex, and OpenCode quota data before printing (agy read can take ~3s)
        #[arg(long)]
        refresh: bool,
    },
    /// Show conversation turns for a session ID prefix
    Log {
        /// Session ID or unique prefix
        session: String,
        /// Maximum number of turns to show
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Print each turn's original log line (Codex/Claude only)
        #[arg(long)]
        full: bool,
    },
    /// Monitor hc events, scan logs, link terminals, enforce budgets, and rotate workers
    Daemon {
        /// Seconds between scans, linking, policy checks, and rotation
        #[arg(long, default_value_t = 10)]
        interval: u64,
        /// Replay hc events from a JSONL file and exit (for debugging)
        #[arg(long)]
        replay: Option<PathBuf>,
    },
    /// List herdr-core terminals known to agmag
    Ps {
        /// Include finished terminals
        #[arg(long)]
        all: bool,
    },
    /// Send a structured message: GIAO (assign), "BÁO CÁO" (report), LỖI (error), HỎI (ask), CHẶN (blocked), ĐẠT (pass), "CHƯA ĐẠT" (fail). English aliases ASSIGN|REPORT|ERROR|QUESTION|BLOCKED|PASS|FAIL are also accepted. Use destination `user` for the inbox
    Send {
        /// Destination terminal or `user`
        to: String,
        /// Message code or English alias (see command help)
        #[arg(long = "type")]
        kind: String,
        /// Task ID to attach to the message
        #[arg(long)]
        task: Option<String>,
        /// Message text
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Show blocked agents, questions, requests, and warnings in the user inbox
    Inbox {
        /// Include resolved inbox items
        #[arg(long)]
        all: bool,
    },
    /// Dismiss an inbox item without sending input to any terminal
    Done {
        /// Inbox item ID
        id: i64,
    },
    /// Reply to an inbox item by sending input to its terminal
    Reply {
        /// Inbox item ID
        id: i64,
        /// Reply text
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Spawn an agent through hc with worker-limit and budget checks
    Spawn {
        /// herdr-core template name
        template: String,
        /// Task ID for labels, budgets, and handoff records
        #[arg(long)]
        task: String,
        /// Agent role label (for example `worker`)
        #[arg(long, default_value = "worker")]
        role: String,
        /// Optional terminal name
        #[arg(long)]
        name: Option<String>,
        /// Working directory for the spawned terminal
        #[arg(long)]
        cwd: Option<String>,
        /// Initial prompt or text passed after `--`
        #[arg(last = true)]
        text: Vec<String>,
    },
    /// Stop a terminal. Codex/Claude and unknown apps still require a user to type y
    Kill {
        /// Terminal address or target
        target: String,
        /// Skip the interactive confirmation prompt (still requires a user caller)
        #[arg(long)]
        yes: bool,
    },
    /// Interrupt the current turn. Codex/Claude and unknown apps still require a user to type y
    Interrupt {
        /// Terminal address or target
        target: String,
        /// Skip the interactive confirmation prompt (still requires a user caller)
        #[arg(long)]
        yes: bool,
    },
    /// Rotate a session: OpenCode handoff+kill+spawn, or managed Claude/Codex handoff+open without kill
    Rotate {
        /// Terminal address or target
        target: String,
        /// Accepted for compatibility; rotation queues a handoff and never prompts
        #[arg(long)]
        yes: bool,
    },
    /// Enable or disable managed rotation for a manually opened Claude Code or Codex session
    Manage {
        /// Terminal address or target
        target: String,
        /// Disable managed rotation for the target
        #[arg(long)]
        off: bool,
        /// Launch command string (alternative to argv after `--`)
        #[arg(long)]
        cmd: Option<String>,
        /// Launch argv passed after `--`
        #[arg(last = true)]
        argv: Vec<String>,
    },
    /// Manually clean up old handed-off sessions (requires one typed y/N confirmation)
    Cleanup {
        /// Terminal addresses to clean up
        targets: Vec<String>,
        /// Select all eligible handed-off terminals
        #[arg(long)]
        all: bool,
        /// Compatibility flag; cleanup still requires one typed confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Manage budgets: task:<ID> (tokens) or provider:<name>:<minutes> (quota percent)
    Budget {
        #[command(subcommand)]
        cmd: BudgetCmd,
    },
    /// Enable or disable all automatic actions (emergency stop)
    Policy {
        /// One of `on`, `off`, or `status`
        #[arg(value_parser = ["on", "off", "status"])]
        state: String,
    },
    /// Manually link a terminal to a session
    Link {
        /// Terminal address
        terminal: String,
        /// Session ID or prefix
        session: String,
    },
    /// Ask agy (Gemini) once in read-only mode and record token usage
    Ask {
        /// agy model name
        #[arg(long)]
        model: Option<String>,
        /// Timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
        /// Question text
        #[arg(required = true, num_args = 1..)]
        question: Vec<String>,
    },
    /// Claude Code hooks
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    /// RAM / CPU / disk of this machine and how much each agent's process tree uses (live, not stored)
    Sys,
    /// Run the monitoring UI in a herdr pane
    Tui {
        /// Refresh interval in seconds
        #[arg(long, default_value_t = 2)]
        refresh: u64,
    },
}

#[derive(Subcommand)]
enum BudgetCmd {
    /// Set a soft/hard budget for a scope
    Set {
        /// Budget scope: `task:<ID>` or `provider:<name>:<minutes>`
        scope: String,
        /// Soft limit (tokens or quota percent, depending on scope)
        #[arg(long)]
        soft: f64,
        /// Hard limit (tokens or quota percent, depending on scope)
        #[arg(long)]
        hard: f64,
        /// Action to take when a limit is hit (for example `notify`)
        #[arg(long, default_value = "notify")]
        action: String,
    },
    /// List configured budgets
    Ls,
    /// Remove a configured budget
    Rm {
        /// Budget scope to remove
        scope: String,
    },
}

#[derive(Subcommand)]
enum HookCmd {
    /// Read statusline JSON from stdin, record quota/context, and print one status line
    Statusline,
    /// Add statusLine to ~/.claude/settings.json without overwriting an existing one
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
    // No database needed: `agm sys` still works when ~/.agmag is missing or locked.
    if let Cmd::Sys = cli.cmd {
        let json = cli.json || !std::io::stdout().is_terminal();
        return match agmag::sysmon::read_once() {
            Some(stats) => match emit(json, &stats, || print_sys(&stats)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("agm: error: {e:#}");
                    ExitCode::from(1)
                }
            },
            None => {
                eprintln!("agm: error: system metrics are not available on this platform");
                ExitCode::from(1)
            }
        };
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
        }
        | Cmd::Sys => unreachable!("xử lý trong main"),
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
            let mut codex_quota = agmag::codex_quota::CodexQuotaPoller::new(
                cfg.policy.codex_quota_poll_min,
                std::sync::Arc::new(agmag::codex_quota::fetch_usage),
            );
            let mut opencode_quota = agmag::opencode_quota::OpenCodeQuotaPoller::new(
                cfg.policy.opencode_quota_poll_min,
                std::sync::Arc::new(agmag::opencode_quota::fetch_usage),
            );
            ops::daemon_loop(&store, &cfg, interval, |s| {
                scan(s, now_ms() - SCAN_DAYS * DAY_MS)?;
                agy_quota.tick(s, now_ms())?;
                codex_quota.tick(s, now_ms())?;
                opencode_quota.tick(s, now_ms())
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
        } => ops::spawn(
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
        ),
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
                match agmag::codex_quota::fetch_usage()
                    .and_then(|out| agmag::codex_quota::record_usage(&store, &out, now))
                {
                    Ok(_) => {}
                    Err(e) => eprintln!("agm: could not read codex quota: {e:#}"),
                }
                match agmag::opencode_quota::fetch_usage()
                    .and_then(|out| agmag::opencode_quota::record_usage(&store, &out, now))
                {
                    Ok(p) if p.unknown > 0 => eprintln!(
                        "agm: skipped {} unrecognized opencode-go quota meter(s)",
                        p.unknown
                    ),
                    Ok(_) => {}
                    Err(e) => eprintln!("agm: could not read opencode-go quota: {e:#}"),
                }
            }
            let cfg_loaded = cfg()?;
            let rows = report::quota_with_config(&store, &cfg_loaded, now / 1000)?;
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
    println!("{}", separate_sum_row(&t.to_string()));
    println!("{USD_NOTE}");
}

/// Repeats the header rule (`╞═╪═╡`) just above the SUM row so the total stands apart from the data rows.
/// Left unchanged when either line is missing (e.g. a style without a header rule).
fn separate_sum_row(rendered: &str) -> String {
    let lines: Vec<&str> = rendered.lines().collect();
    let Some(rule) = lines.iter().find(|l| l.starts_with('╞')) else {
        return rendered.to_string();
    };
    let Some(sum_at) = lines.iter().rposition(|l| {
        strip_sgr(l)
            .trim_start_matches('│')
            .trim_start()
            .starts_with("SUM")
    }) else {
        return rendered.to_string();
    };
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + 1);
    out.extend_from_slice(&lines[..sum_at]);
    out.push(rule);
    out.extend_from_slice(&lines[sum_at..]);
    out.join("\n")
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
    if minutes == 43200 {
        "mo".to_string()
    } else if minutes.is_multiple_of(1440) {
        format!("{}d", minutes / 1440)
    } else if minutes.is_multiple_of(60) {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn quota_table(rows: &[report::QuotaRow], now_s: i64) -> Table {
    // Hiển thị phần CÒN LẠI (giảm dần từ 100%); JSON vẫn có cả `used_percent` lẫn `left_percent`.
    let mut t = table(&["plan", "window", "left", "", "reset"]);
    for r in rows {
        let reset = if r.reset_passed {
            "reset passed".to_string()
        } else {
            let left = r.resets_at_s - now_s;
            format!("in {}h{:02}m", left / 3600, left % 3600 / 60)
        };
        if r.stale {
            // Cùng nhãn với TUI: số cũ không được trình bày như số tươi (không bar, không %).
            let age = agmag::dashboard::format_age_s(r.observed_age_s);
            t.add_row(vec![
                r.provider.clone(),
                window_label(r.window_minutes),
                "··········".into(),
                format!(
                    "stale · seen {age} ago{}",
                    if r.limited { " [91mlimited[0m" } else { "" }
                ),
                reset,
            ]);
            continue;
        }
        let left = if r.left_percent.is_finite() {
            r.left_percent
        } else {
            0.0
        };
        let filled = ((left / 10.0).round() as usize).min(10);
        let limited = if r.limited {
            " \x1b[91mlimited\x1b[0m"
        } else {
            ""
        };
        let left_text = if r.left_percent.is_finite() {
            format!("{:.0}%{limited}", r.left_percent)
        } else if r.limited {
            "?\x1b[91m limited\x1b[0m".to_string()
        } else {
            "?".into()
        };
        t.add_row(vec![
            r.provider.clone(),
            window_label(r.window_minutes),
            format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled)),
            left_text,
            reset,
        ]);
    }
    t
}

fn print_quota(rows: &[report::QuotaRow], now_s: i64) {
    println!("{}", quota_table(rows, now_s));
}

/// Removes ANSI SGR sequences (`ESC [ ... m`) for `NO_COLOR` output.
fn strip_sgr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            for ch in chars.by_ref() {
                if ch == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Colored MACHINE / DISKS / AGENTS report, sized to the terminal; plain text when `NO_COLOR` is set.
fn print_sys(s: &agmag::sysmon::SysStats) {
    let report = agmag::dashboard::render_sys_report(s, tui::pane_width());
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        print!("{}", strip_sgr(&report));
    } else {
        print!("{report}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota_row(stale: bool, age_s: i64) -> report::QuotaRow {
        report::QuotaRow {
            provider: "codex".into(),
            window_minutes: 10080,
            used_percent: 22.0,
            left_percent: 78.0,
            resets_at_s: 1_000_000 + 3600,
            observed_ms: 0,
            reset_passed: false,
            stale,
            observed_age_s: age_s,
            limited: false,
        }
    }

    #[test]
    fn quota_table_marks_stale_rows_without_fresh_bar() {
        let now_s = 1_000_000;
        let stale = quota_table(&[quota_row(true, 90_000)], now_s).to_string();
        assert!(
            stale.contains("stale · seen 1d01h ago"),
            "bảng stale phải có nhãn tuổi như TUI: {stale}"
        );
        assert!(
            !stale.contains('█') && !stale.contains("78%"),
            "hàng stale không được trình bày như số tươi: {stale}"
        );

        let fresh = quota_table(&[quota_row(false, 0)], now_s).to_string();
        assert!(
            fresh.contains("78%"),
            "hàng tươi vẫn hiện phần trăm: {fresh}"
        );
        assert!(
            !fresh.contains("stale"),
            "hàng tươi không có nhãn stale: {fresh}"
        );
    }

    #[test]
    fn sum_row_gets_its_own_rule() {
        let mut t = table(&["model", "total"]);
        t.add_row(vec!["m1", "1"]);
        t.add_row(vec![
            Cell::new("SUM").add_attribute(Attribute::Bold),
            Cell::new("1"),
        ]);
        let out = separate_sum_row(&t.to_string());
        let lines: Vec<&str> = out.lines().collect();
        let rules: Vec<usize> = (0..lines.len())
            .filter(|&i| lines[i].starts_with('╞'))
            .collect();
        assert_eq!(rules.len(), 2, "{out}");
        assert!(strip_sgr(lines[rules[1] + 1]).contains("SUM"), "{out}");
        assert!(strip_sgr(lines[rules[1] - 1]).contains("m1"), "{out}");
        // No SUM row: unchanged.
        let plain = table(&["a"]).to_string();
        assert_eq!(separate_sum_row(&plain), plain);
    }
}
