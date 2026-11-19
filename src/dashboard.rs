//! Data snapshot and ANSI text rendering for `agm tui` (spec §5.8). Pure — no stdin/stdout I/O.

use anyhow::Result;
use comfy_table::{presets::UTF8_FULL_CONDENSED, Attribute, Cell, CellAlignment, Color, Table};

use crate::actions::KINDS;
use crate::config::Config;
use crate::policy::{budget_value, enabled, session_context_pct};
use crate::report::{quota, spark_bar, usage_summary, QuotaRow, UsageBucket, UsageSummary};
use crate::store::{InboxRow, MessageRow, Store};

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const GRAY: &str = "\x1b[90m";
const WHITE: &str = "\x1b[97m";
const CYAN: &str = "\x1b[36m";
const BRIGHT_CYAN: &str = "\x1b[96m";
const BOLD_CYAN: &str = "\x1b[1;96m";
const BRIGHT_GREEN: &str = "\x1b[92m";
const BOLD_GREEN: &str = "\x1b[1;92m";
const BRIGHT_YELLOW: &str = "\x1b[93m";
const BOLD_YELLOW: &str = "\x1b[1;93m";
const BRIGHT_RED: &str = "\x1b[91m";
const BOLD_RED: &str = "\x1b[1;91m";
const BRIGHT_MAGENTA: &str = "\x1b[95m";

#[derive(Debug, Clone, PartialEq)]
pub struct TermView {
    pub address: String,
    pub name: String,
    pub app: String,
    pub status: String,
    pub task: String,
    /// None when the terminal is not linked to a log session (unknown, not 0).
    pub tokens: Option<u64>,
    pub context_pct: Option<f64>,
    pub handoff_state: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetView {
    pub scope: String,
    pub unit: String,
    pub value: Option<f64>,
    pub soft: f64,
    pub hard: f64,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub now_s: i64,
    pub policy_on: bool,
    pub quota: Vec<QuotaRow>,
    pub usage_summary: UsageSummary,
    pub terminals: Vec<TermView>,
    pub live_rotation_pairs: Vec<(String, String, String)>,
    pub budgets: Vec<BudgetView>,
    pub inbox: Vec<InboxRow>,
    pub messages: Vec<MessageRow>,
}

pub fn snapshot(store: &Store, cfg: &Config, now_ms: i64) -> Result<Snapshot> {
    let mut terminals = Vec::new();
    for t in store.terminals(true)? {
        let (tokens, ctx) = match t.session {
            Some(sid) => {
                let n: i64 = store.conn().query_row(
                    "SELECT COALESCE(SUM(input_uncached + cache_read + cache_write + output), 0) FROM usage WHERE session = ?1",
                    [sid],
                    |r| r.get(0),
                )?;
                (Some(n.max(0) as u64), session_context_pct(store, cfg, sid)?)
            }
            None => (None, None),
        };
        let mut status = match (t.status.as_deref(), t.kind.as_deref()) {
            (Some("blocked"), Some(k)) => format!("BLOCKED ({k})"),
            (Some("blocked"), None) => "BLOCKED".into(),
            (Some(s), _) => s.to_string(),
            (None, _) => "-".into(),
        };
        if t.handoff_state == "handed_off" {
            status.push_str(" [HANDED OFF]");
        }
        terminals.push(TermView {
            address: t.address,
            name: t.name.unwrap_or_default(),
            app: t.app.unwrap_or_else(|| "?".into()),
            status,
            task: t.task.unwrap_or_default(),
            tokens,
            context_pct: ctx,
            handoff_state: t.handoff_state,
        });
    }
    let mut stmt = store.conn().prepare(
        "SELECT r.old_terminal, r.new_terminal, COALESCE(r.cwd, o.cwd, '')
         FROM rotations r
         JOIN terminals o ON o.address = r.old_terminal
         JOIN terminals n ON n.address = r.new_terminal
         WHERE r.state = 'done' AND o.state = 'live' AND n.state = 'live'
         ORDER BY r.id",
    )?;
    let live_rotation_pairs = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<(String, String, String)>>>()?;
    let mut budgets = Vec::new();
    for b in store.budgets()? {
        budgets.push(BudgetView { value: budget_value(store, &b)?, scope: b.scope, unit: b.unit, soft: b.soft, hard: b.hard });
    }
    Ok(Snapshot {
        now_s: now_ms / 1000,
        policy_on: enabled(store)?,
        quota: quota(store, now_ms / 1000)?,
        usage_summary: usage_summary(store, cfg, now_ms)?,
        terminals,
        live_rotation_pairs,
        budgets,
        inbox: store.inbox(true)?,
        messages: store.messages(5)?,
    })
}

fn paint(style: &str, text: &str) -> String {
    format!("{style}{text}{RESET}")
}

fn pct_color(pct: f64) -> &'static str {
    if pct >= 85.0 {
        BRIGHT_RED
    } else if pct >= 60.0 {
        BRIGHT_YELLOW
    } else {
        BRIGHT_GREEN
    }
}

fn colored_bar(pct: f64, color: &str) -> String {
    let filled = ((pct / 10.0).round().clamp(0.0, 10.0)) as usize;
    let empty = 10 - filled;
    if empty == 0 {
        format!("{color}{}{RESET}", "█".repeat(filled))
    } else if filled == 0 {
        format!("{GRAY}{}{RESET}", "░".repeat(empty))
    } else {
        format!("{color}{}{GRAY}{}{RESET}", "█".repeat(filled), "░".repeat(empty))
    }
}

fn tokens(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

fn month_abbr(mo: &str) -> &str {
    match mo {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        other => other,
    }
}

fn section(icon: &str, title: &str, width: usize) -> String {
    let prefix_len = 3 + icon.chars().count() + 1 + title.chars().count() + 1;
    let line = "─".repeat(width.saturating_sub(1).saturating_sub(prefix_len));
    format!("{CYAN}── {BOLD_CYAN}{icon} {title}{RESET} {GRAY}{line}{RESET}\n")
}

fn terminals_table(width: usize) -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED).enforce_styling().set_width(width as u16);
    t.set_header(
        ["t", "name", "app", "status", "task", "tokens", "ctx"]
            .into_iter()
            .map(|h| Cell::new(h).fg(Color::Cyan).add_attribute(Attribute::Bold)),
    );
    t
}

fn app_cell(app: &str) -> Cell {
    let c = Cell::new(app);
    match app {
        "claude-code" => c.fg(Color::Magenta),
        "codex" => c.fg(Color::Green),
        "opencode" => c.fg(Color::Cyan),
        "antigravity" => c.fg(Color::Yellow),
        _ => c.fg(Color::DarkGrey),
    }
}

fn status_cell(status: &str) -> Cell {
    let c = Cell::new(status);
    if status.starts_with("BLOCKED") {
        c.fg(Color::Red).add_attribute(Attribute::Bold)
    } else if status.contains("[HANDED OFF]") {
        c.fg(Color::Yellow)
    } else if status == "working" || status == "running" {
        c.fg(Color::Green).add_attribute(Attribute::Bold)
    } else if status == "idle" || status == "done" {
        c.fg(Color::Cyan)
    } else if status == "exited" || status == "-" {
        c.fg(Color::DarkGrey)
    } else {
        c.fg(Color::White)
    }
}

fn ctx_cell(ctx: Option<f64>) -> Cell {
    let c = match ctx {
        Some(p) => {
            let cell = Cell::new(format!("{p:.0}%"));
            if p >= 70.0 {
                cell.fg(Color::Red).add_attribute(Attribute::Bold)
            } else if p >= 50.0 {
                cell.fg(Color::Yellow)
            } else {
                cell.fg(Color::Green)
            }
        }
        None => Cell::new("?").fg(Color::DarkGrey),
    };
    c.set_alignment(CellAlignment::Right)
}

fn inbox_source_badge(source: &str) -> String {
    let tag = format!("[{source}]");
    match source {
        "blocked" => paint(BOLD_RED, &tag),
        "budget" => paint(BOLD_YELLOW, &tag),
        "rotation" => paint(BRIGHT_MAGENTA, &tag),
        "request" => paint(BRIGHT_CYAN, &tag),
        _ => paint(WHITE, &tag),
    }
}

fn msg_kind_badge(kind: &str) -> String {
    let tag = format!("[{kind}]");
    match kind {
        "GIAO" => paint(BRIGHT_CYAN, &tag),
        "BÁO CÁO" => paint(BRIGHT_GREEN, &tag),
        "ĐẠT" => paint(BOLD_GREEN, &tag),
        "HỎI" => paint(BRIGHT_YELLOW, &tag),
        "LỖI" | "CHẶN" | "CHƯA ĐẠT" => paint(BRIGHT_RED, &tag),
        _ => paint(WHITE, &tag),
    }
}

fn render_header(s: &Snapshot) -> String {
    let policy = if s.policy_on {
        paint(BOLD_GREEN, "● POLICY ON")
    } else {
        paint(BOLD_RED, "○ POLICY OFF")
    };
    let blocked_cnt = s.terminals.iter().filter(|t| t.status.starts_with("BLOCKED")).count();
    let term_noun = if s.terminals.len() == 1 { "terminal" } else { "terminals" };
    let term_str = if blocked_cnt > 0 {
        format!(
            "{WHITE}{} {term_noun}{RESET} {}",
            s.terminals.len(),
            paint(BOLD_RED, &format!("({blocked_cnt} blocked)"))
        )
    } else {
        paint(WHITE, &format!("{} {term_noun}", s.terminals.len()))
    };
    let inbox_str = if s.inbox.is_empty() {
        paint(GRAY, "inbox 0")
    } else {
        paint(BOLD_YELLOW, &format!("inbox {}", s.inbox.len()))
    };
    let sep = paint(GRAY, " │ ");
    format!(
        " {}  {policy}{sep}{term_str}{sep}{inbox_str}{sep}{}\n",
        paint(BOLD_CYAN, "◆ agmag"),
        paint(GRAY, &s.usage_summary.offset_label),
    )
}

fn wrap_styled_items(items: &[(usize, String)], sep: &str, indent: usize, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut cur_len = indent;
    for (plain_len, styled) in items {
        let add_len = if cur.is_empty() { *plain_len } else { 3 + *plain_len };
        if !cur.is_empty() && cur_len + add_len > max_width {
            lines.push(cur.join(sep));
            cur.clear();
            cur_len = indent;
        }
        cur_len += if cur.is_empty() { *plain_len } else { 3 + *plain_len };
        cur.push(styled.as_str());
    }
    if !cur.is_empty() {
        lines.push(cur.join(sep));
    }
    lines
}

fn render_footer(width: usize) -> String {
    let cmd = |k: &str, args: &str| {
        if args.is_empty() {
            paint(BOLD_CYAN, k)
        } else {
            format!("{} {}", paint(BOLD_CYAN, k), paint(DIM, args))
        }
    };
    let sep = paint(GRAY, " · ");
    let items: Vec<(usize, String)> = [
        ("r", "<#> <reply>"),
        ("d", "<#> (done)"),
        ("s", "<t> <TYPE> <msg>"),
        ("c", "[1,2|all] (cleanup)"),
        ("i", "<t>"),
        ("k", "<t>"),
        ("p", "on|off"),
        ("q", ""),
    ]
    .into_iter()
    .map(|(k, args)| {
        let plain_len = k.chars().count() + if args.is_empty() { 0 } else { 1 + args.chars().count() };
        (plain_len, cmd(k, args))
    })
    .collect();
    let lines = wrap_styled_items(&items, &sep, 5, width.saturating_sub(1));
    let rule = paint(GRAY, &"─".repeat(width.saturating_sub(1)));
    let mut out = format!("{rule}\n");
    for (idx, l) in lines.into_iter().enumerate() {
        let prefix = if idx == 0 { paint(GRAY, "cmd:") } else { "    ".into() };
        out += &format!("{prefix} {l}\n");
    }
    out
}

/// Full-screen ANSI text render (includes section headers and command bar).
pub fn render(s: &Snapshot, width: usize) -> String {
    let mut out = render_header(s);

    out += &section("◈", "PLAN QUOTA", width);
    if s.quota.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(no quota data — run `agm scan`; Claude needs `agm hook install`)"));
    }
    for q in &s.quota {
        let win = if q.window_minutes % 1440 == 0 {
            format!("{}d", q.window_minutes / 1440)
        } else {
            format!("{}h", q.window_minutes / 60)
        };
        let reset = if q.reset_passed {
            paint(BRIGHT_GREEN, "reset ready")
        } else {
            let left = q.resets_at_s - s.now_s;
            paint(CYAN, &format!("resets in {}h{:02}m", left / 3600, left % 3600 / 60))
        };
        let col = pct_color(q.used_percent);
        let pct_str = paint(col, &format!("{:>3.0}%", q.used_percent));
        out += &format!(
            "  {} {}  {} {pct_str}  {reset}\n",
            paint(WHITE, &format!("{:<8}", q.provider)),
            paint(CYAN, &format!("{:>3}", win)),
            colored_bar(q.used_percent, col),
        );
    }

    out += &section("▪", &format!("USAGE ({})", s.usage_summary.offset_label), width);
    let fmt_bucket = |label: &str, b: &UsageBucket| -> String {
        let usd = b.api_usd.map(|u| paint(BRIGHT_GREEN, &format!(" (${u:.2})"))).unwrap_or_default();
        format!("{} {}{usd}", paint(CYAN, label), paint(WHITE, &tokens(b.usage.total())))
    };
    let sep = paint(GRAY, " │ ");
    out += &format!(
        "  {}{sep}{}{sep}{}\n",
        fmt_bucket("24h", &s.usage_summary.last_24h),
        fmt_bucket("Today", &s.usage_summary.today),
        fmt_bucket("This month", &s.usage_summary.this_month),
    );

    let max_m = s.usage_summary.recent_months.iter().map(|m| m.usage.total()).max().unwrap_or(0);
    let cross_year = s
        .usage_summary
        .recent_months
        .first()
        .zip(s.usage_summary.recent_months.last())
        .is_some_and(|(a, b)| a.key.get(..4) != b.key.get(..4));
    let month_pairs: Vec<(usize, String)> = s
        .usage_summary
        .recent_months
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let ym = m.key.split('-').collect::<Vec<_>>();
            let lbl = match ym.as_slice() {
                [y, mo] if cross_year && y.len() >= 4 => format!("{}'{}", month_abbr(mo), &y[2..]),
                [_, mo] => month_abbr(mo).to_string(),
                _ => m.key.clone(),
            };
            let val = m.usage.total();
            let tok = tokens(val);
            let bar_ch = spark_bar(val, max_m, false).to_string();
            let is_peak = max_m > 0 && val == max_m;
            let is_latest = idx + 1 == s.usage_summary.recent_months.len();
            let plain_len = format!("{lbl} {bar_ch} {tok}").chars().count();
            let styled = format!(
                "{} {} {}",
                if is_latest { paint(WHITE, &lbl) } else { paint(GRAY, &lbl) },
                paint(if is_peak { BRIGHT_GREEN } else { BRIGHT_CYAN }, &bar_ch),
                if is_latest { paint(WHITE, &tok) } else { tok },
            );
            (plain_len, styled)
        })
        .collect();
    for line in wrap_styled_items(&month_pairs, &sep, 2, width.saturating_sub(1)) {
        out += &format!("  {line}\n");
    }
    if s.usage_summary.incomplete_coverage {
        out += &format!(
            "  {}\n",
            paint(
                BRIGHT_YELLOW,
                &format!("! incomplete 4-month coverage — run `agm scan --since {}`", s.usage_summary.four_months_start_day),
            )
        );
    }

    out += &section("▸", "TERMINALS", width);
    for (old, new, cwd) in &s.live_rotation_pairs {
        out += &format!(
            "{BOLD_RED}  ! WARNING: {old} (handed off) and {new} are both running in {cwd} — avoid conflicting edits; type 'c' to clean up{RESET}\n"
        );
    }
    if s.terminals.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(no active terminals)"));
    } else {
        let mut t = terminals_table(width);
        for v in &s.terminals {
            let addr_cell = if v.status.starts_with("BLOCKED") {
                Cell::new(&v.address).fg(Color::Red).add_attribute(Attribute::Bold)
            } else {
                Cell::new(&v.address).fg(Color::White)
            };
            let name_cell = if v.name.is_empty() {
                Cell::new("-").fg(Color::DarkGrey)
            } else {
                Cell::new(&v.name).fg(Color::White)
            };
            let task_cell = if v.task.is_empty() {
                Cell::new("-").fg(Color::DarkGrey)
            } else {
                Cell::new(&v.task).fg(Color::Cyan)
            };
            let tok_cell = match v.tokens {
                Some(n) => Cell::new(tokens(n)).fg(Color::White),
                None => Cell::new("?").fg(Color::DarkGrey),
            }
            .set_alignment(CellAlignment::Right);
            t.add_row(vec![
                addr_cell,
                name_cell,
                app_cell(&v.app),
                status_cell(&v.status),
                task_cell,
                tok_cell,
                ctx_cell(v.context_pct),
            ]);
        }
        out += &format!("{t}\n");
    }

    out += &section("◆", "BUDGETS", width);
    if s.budgets.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(none set — agm budget set task:<ID> --soft … --hard …)"));
    }
    for b in &s.budgets {
        let pct = b.value.filter(|_| b.hard > 0.0).map(|v| v / b.hard * 100.0);
        let (bar_col, flag) = match b.value {
            Some(v) if v >= b.hard => (BRIGHT_RED, paint(BOLD_RED, " HARD")),
            Some(v) if v >= b.soft => (BRIGHT_YELLOW, paint(BRIGHT_YELLOW, " soft")),
            _ => (BRIGHT_GREEN, String::new()),
        };
        // Missing value → "?" (never render a fake 0%). Percent budgets keep decimals.
        let num = |v: f64| if b.unit == "tokens" { tokens(v as u64) } else { format!("{v:.1}") };
        let bar_str = pct.map(|p| colored_bar(p, bar_col)).unwrap_or_else(|| paint(GRAY, "··········"));
        let pct_str = pct
            .map(|p| paint(bar_col, &format!("{p:>3.0}%")))
            .unwrap_or_else(|| paint(GRAY, "   ?"));
        out += &format!(
            "  {} {bar_str} {pct_str}  {} / {} {}{flag}\n",
            paint(WHITE, &format!("{:<24}", b.scope)),
            b.value.map(num).unwrap_or_else(|| "?".into()),
            num(b.hard),
            paint(GRAY, &b.unit),
        );
    }

    let inbox_icon = if s.inbox.is_empty() { "○" } else { "●" };
    out += &section(inbox_icon, "INBOX", width);
    if s.inbox.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(empty)"));
    }
    for i in &s.inbox {
        let term = i.terminal.as_deref().unwrap_or("-");
        out += &format!(
            "  {} {} {} {} {}\n",
            paint(BOLD_YELLOW, &format!("#{}", i.id)),
            inbox_source_badge(&i.source),
            paint(WHITE, term),
            paint(GRAY, "—"),
            one_line(&i.text, 200),
        );
    }

    out += &section("»", "RECENT MESSAGES", width);
    if s.messages.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(no messages)"));
    }
    for m in &s.messages {
        let st_col = match m.status.as_str() {
            "delivered" | "sent" => BRIGHT_GREEN,
            "failed" => BRIGHT_RED,
            "queued" => BRIGHT_YELLOW,
            _ => GRAY,
        };
        out += &format!(
            "  {} {} {}  {}  {}  {}\n",
            paint(CYAN, &m.from),
            paint(GRAY, "→"),
            paint(WHITE, &m.to),
            msg_kind_badge(&m.kind),
            one_line(&m.text, 60),
            paint(st_col, &format!("({})", m.status)),
        );
    }
    out += &render_footer(width);
    out
}

/// Single-line display: replace newlines and control/ANSI characters with spaces so the TUI layout stays intact.
fn one_line(s: &str, max: usize) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(max).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum TuiCmd {
    Refresh,
    Quit,
    Reply(i64, String),
    Done(i64),
    Send(String, String, String),
    Cleanup(Option<String>),
    Interrupt(String),
    Kill(String),
    Policy(bool),
}

pub fn parse_command(line: &str) -> std::result::Result<TuiCmd, String> {
    let line = line.trim();
    let (head, rest) = line.split_once(' ').map_or((line, ""), |(h, r)| (h, r.trim()));
    Ok(match head {
        "" => TuiCmd::Refresh,
        "q" => TuiCmd::Quit,
        "r" => {
            let (id, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let Ok(id) = id.parse() else { return Err("usage: r <id> <reply>".into()) };
            if text.trim().is_empty() {
                return Err("missing reply text".into());
            }
            TuiCmd::Reply(id, text.trim().into())
        }
        "d" => match rest.parse() {
            Ok(id) => TuiCmd::Done(id),
            Err(_) => return Err("usage: d <id>".into()),
        },
        "s" => {
            let (target, after) = rest.split_once(' ').unwrap_or((rest, ""));
            let after = after.trim_start();
            // Kinds may contain two words (BÁO CÁO, CHƯA ĐẠT); require a space boundary right after the kind.
            let Some(kind) = KINDS
                .iter()
                .filter(|k| after.strip_prefix(**k).is_some_and(|r| r.starts_with(' ')))
                .max_by_key(|k| k.len())
            else {
                return Err(format!("usage: s <terminal> <{}> <message>", KINDS.join("|")));
            };
            let text = after[kind.len()..].trim();
            if target.is_empty() || text.is_empty() {
                return Err("usage: s <terminal> <TYPE> <message>".into());
            }
            TuiCmd::Send(target.into(), kind.to_string(), text.into())
        }
        "c" => TuiCmd::Cleanup((!rest.is_empty()).then(|| rest.to_string())),
        "i" if !rest.is_empty() => TuiCmd::Interrupt(rest.into()),
        "k" if !rest.is_empty() => TuiCmd::Kill(rest.into()),
        "p" if rest == "on" || rest == "off" => TuiCmd::Policy(rest == "on"),
        _ => return Err(format!("unknown command '{line}'")),
    })
}
