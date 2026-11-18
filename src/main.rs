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
}

const SCAN_DAYS: i64 = 7;
const DAY_MS: i64 = 86_400_000;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn agm_home() -> PathBuf {
    std::env::var_os("AGM_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().expect("không tìm thấy thư mục home").join(".agmag"))
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
    let cfg = Config::load(&cfg_path).with_context(|| format!("đọc cấu hình {}", cfg_path.display()))?;
    let store = Store::open(&home.join("agm.db"))?;
    let json = cli.json || !std::io::stdout().is_terminal();
    let now = now_ms();

    let scan_days = match cli.cmd {
        Cmd::Scan { days } => days,
        _ => SCAN_DAYS,
    };
    let mut stats = ingest_all(&store, &roots(), now - scan_days * DAY_MS)?;
    match agmag::opencode::ingest_opencode(&store, &agmag::opencode::default_db()) {
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

    match cli.cmd {
        Cmd::Scan { .. } => emit(json, &stats_json(&stats), || {
            println!(
                "Đã quét {} tệp, {} dòng mới ({} sự kiện, {} dòng loại lạ, {} dòng hỏng).",
                stats.files, stats.lines, stats.events, stats.unknown, stats.bad
            )
        }),
        Cmd::Ls { days } => {
            let rows = report::sessions(&store, &cfg, now - days * DAY_MS)?;
            emit(json, &rows, || print_sessions(&rows))
        }
        Cmd::Cost { by, days } => {
            let rows = report::cost(&store, &cfg, by, now - days * DAY_MS)?;
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
