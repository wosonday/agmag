//! Data snapshot and ANSI text rendering for `agm tui` (spec §5.8). Pure — no stdin/stdout I/O.

use anyhow::Result;
use comfy_table::{presets::UTF8_FULL_CONDENSED, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

fn pad_cells(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(text.width())))
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
    let clean_title = one_line(title, width.saturating_sub(10));
    let prefix_len = 3 + icon.chars().count() + 1 + clean_title.chars().count() + 1;
    let line = "─".repeat(width.saturating_sub(1).saturating_sub(prefix_len));
    format!("{CYAN}── {BOLD_CYAN}{icon} {clean_title}{RESET} {GRAY}{line}{RESET}\n")
}

fn terminals_table(width: usize) -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .enforce_styling()
        .set_width(width.saturating_sub(1) as u16);
    t.set_header(
        ["t", "name", "app", "status", "task", "tokens", "ctx"]
            .into_iter()
            .map(|h| Cell::new(h).fg(Color::Cyan).add_attribute(Attribute::Bold)),
    );
    t
}

fn app_cell(app: &str) -> Cell {
    let clean = sanitize_one_line(app);
    let c = Cell::new(&clean);
    match clean.as_str() {
        "claude-code" => c.fg(Color::Magenta),
        "codex" => c.fg(Color::Green),
        "opencode" => c.fg(Color::Cyan),
        "antigravity" => c.fg(Color::Yellow),
        _ => c.fg(Color::DarkGrey),
    }
}

fn status_cell(status: &str) -> Cell {
    let clean = sanitize_one_line(status);
    let c = Cell::new(&clean);
    if clean.starts_with("BLOCKED") {
        c.fg(Color::Red).add_attribute(Attribute::Bold)
    } else if clean.contains("[HANDED OFF]") {
        c.fg(Color::Yellow)
    } else if clean == "working" || clean == "running" {
        c.fg(Color::Green).add_attribute(Attribute::Bold)
    } else if clean == "idle" || clean == "done" {
        c.fg(Color::Cyan)
    } else if clean == "exited" || clean == "-" {
        c.fg(Color::DarkGrey)
    } else {
        c.fg(Color::White)
    }
}

fn ctx_cell(ctx: Option<f64>) -> Cell {
    let c = match ctx.filter(|p| p.is_finite()) {
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
    let clean = sanitize_one_line(source);
    let tag = format!("[{clean}]");
    match clean.as_str() {
        "blocked" => paint(BOLD_RED, &tag),
        "budget" => paint(BOLD_YELLOW, &tag),
        "rotation" => paint(BRIGHT_MAGENTA, &tag),
        "request" => paint(BRIGHT_CYAN, &tag),
        _ => paint(WHITE, &tag),
    }
}

fn msg_kind_badge(kind: &str) -> String {
    let clean = sanitize_one_line(kind);
    let tag = format!("[{clean}]");
    match clean.as_str() {
        "GIAO" => paint(BRIGHT_CYAN, &tag),
        "BÁO CÁO" => paint(BRIGHT_GREEN, &tag),
        "ĐẠT" => paint(BOLD_GREEN, &tag),
        "HỎI" => paint(BRIGHT_YELLOW, &tag),
        "LỖI" | "CHẶN" | "CHƯA ĐẠT" => paint(BRIGHT_RED, &tag),
        _ => paint(WHITE, &tag),
    }
}

fn wrap_styled_items(items: &[(usize, String)], sep: &str, sep_plain_len: usize, indent: usize, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut cur_len = indent;
    for (plain_len, styled) in items {
        let add_len = if cur.is_empty() { *plain_len } else { sep_plain_len + *plain_len };
        if !cur.is_empty() && cur_len + add_len > max_width {
            lines.push(cur.join(sep));
            cur.clear();
            cur_len = indent;
        }
        cur_len += if cur.is_empty() { *plain_len } else { sep_plain_len + *plain_len };
        cur.push(styled.as_str());
    }
    if !cur.is_empty() {
        lines.push(cur.join(sep));
    }
    lines
}

fn wrap_plain_words(text: &str, first_prefix: &str, cont_prefix: &str, max_width: usize) -> Vec<String> {
    let clean = sanitize_one_line(text);
    let mut out = Vec::new();
    let mut cur = first_prefix.to_string();
    let mut cur_len = first_prefix.width();
    let mut first_word = true;
    for word in clean.split_whitespace() {
        let wlen = word.width();
        let need = if first_word { wlen } else { 1 + wlen };
        if !first_word && cur_len + need > max_width {
            out.push(cur);
            cur = format!("{cont_prefix}{}", one_line(word, max_width.saturating_sub(cont_prefix.width())));
            cur_len = cur.width();
        } else if first_word && cur_len + wlen > max_width {
            let avail = max_width.saturating_sub(cur_len);
            cur.push_str(&one_line(word, avail));
            first_word = false;
            cur_len = cur.width();
        } else {
            if !first_word {
                cur.push(' ');
                cur_len += 1;
            }
            cur.push_str(word);
            cur_len += wlen;
            first_word = false;
        }
    }
    if first_word || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn render_header(s: &Snapshot, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    let policy_plain = if s.policy_on { "● POLICY ON" } else { "○ POLICY OFF" };
    let policy_styled = if s.policy_on {
        paint(BOLD_GREEN, policy_plain)
    } else {
        paint(BOLD_RED, policy_plain)
    };
    let brand_and_policy = (
        7 + 2 + policy_plain.chars().count(), // "◆ agmag  ● POLICY ON"
        format!("{}  {policy_styled}", paint(BOLD_CYAN, "◆ agmag")),
    );

    let blocked_cnt = s.terminals.iter().filter(|t| t.status.starts_with("BLOCKED")).count();
    let term_noun = if s.terminals.len() == 1 { "terminal" } else { "terminals" };
    let term_item = if blocked_cnt > 0 {
        let blk_txt = format!("({blocked_cnt} blocked)");
        let plain = format!("{} {term_noun} {blk_txt}", s.terminals.len());
        (
            plain.chars().count(),
            format!("{} {}", paint(WHITE, &format!("{} {term_noun}", s.terminals.len())), paint(BOLD_RED, &blk_txt)),
        )
    } else {
        let plain = format!("{} {term_noun}", s.terminals.len());
        (plain.chars().count(), paint(WHITE, &plain))
    };

    let inbox_plain = format!("inbox {}", s.inbox.len());
    let inbox_styled = if s.inbox.is_empty() {
        paint(GRAY, &inbox_plain)
    } else {
        paint(BOLD_YELLOW, &inbox_plain)
    };
    let offset_plain = sanitize_one_line(&s.usage_summary.offset_label);

    let sep = paint(GRAY, " │ ");
    let items = [
        brand_and_policy,
        term_item,
        (inbox_plain.chars().count(), inbox_styled),
        (offset_plain.chars().count(), paint(GRAY, &offset_plain)),
    ];
    let mut out = String::new();
    for line in wrap_styled_items(&items, &sep, 3, 1, max_w) {
        out += &format!(" {line}\n");
    }
    out
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
    let lines = wrap_styled_items(&items, &sep, 3, 5, width.saturating_sub(1));
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
    let max_w = width.saturating_sub(1);
    let mut out = render_header(s, width);

    out += &section("◈", "PLAN QUOTA", width);
    if s.quota.is_empty() {
        for l in wrap_plain_words("(no quota data — run `agm scan`; Claude needs `agm hook install`)", "  ", "  ", max_w) {
            out += &format!("{}\n", paint(GRAY, &l));
        }
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
        let prov = one_line(&q.provider, 8);
        if q.used_percent.is_finite() {
            let col = pct_color(q.used_percent);
            let pct_str = paint(col, &format!("{:>3.0}%", q.used_percent));
            out += &format!(
                "  {} {}  {} {pct_str}  {reset}\n",
                paint(WHITE, &pad_cells(&prov, 8)),
                paint(CYAN, &format!("{:>3}", win)),
                colored_bar(q.used_percent, col),
            );
        } else {
            out += &format!(
                "  {} {}  {} {}  {reset}\n",
                paint(WHITE, &pad_cells(&prov, 8)),
                paint(CYAN, &format!("{:>3}", win)),
                paint(GRAY, "··········"),
                paint(GRAY, "   ?"),
            );
        }
    }

    out += &section("▪", &format!("USAGE ({})", s.usage_summary.offset_label), width);
    let fmt_bucket = |label: &str, b: &UsageBucket| -> (usize, String) {
        let tok = tokens(b.usage.total());
        let usd_plain = b.api_usd.map(|u| format!(" (${u:.2})")).unwrap_or_default();
        let usd_styled = if usd_plain.is_empty() {
            String::new()
        } else {
            paint(BRIGHT_GREEN, &usd_plain)
        };
        let plain_len = label.chars().count() + 1 + tok.chars().count() + usd_plain.chars().count();
        (plain_len, format!("{} {}{usd_styled}", paint(CYAN, label), paint(WHITE, &tok)))
    };
    let sep = paint(GRAY, " │ ");
    let bucket_items = [
        fmt_bucket("24h", &s.usage_summary.last_24h),
        fmt_bucket("Today", &s.usage_summary.today),
        fmt_bucket("This month", &s.usage_summary.this_month),
    ];
    for line in wrap_styled_items(&bucket_items, &sep, 3, 2, max_w) {
        out += &format!("  {line}\n");
    }

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
                [y, mo] if cross_year && y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) => {
                    format!("{}'{}", sanitize_one_line(month_abbr(mo)), &y[2..])
                }
                [_, mo] => sanitize_one_line(month_abbr(mo)),
                _ => sanitize_one_line(&m.key),
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
    for line in wrap_styled_items(&month_pairs, &sep, 3, 2, max_w) {
        out += &format!("  {line}\n");
    }
    if s.usage_summary.incomplete_coverage {
        let msg = format!(
            "! incomplete 4-month coverage — run `agm scan --since {}`",
            s.usage_summary.four_months_start_day
        );
        for l in wrap_plain_words(&msg, "  ", "    ", max_w) {
            out += &format!("{}\n", paint(BRIGHT_YELLOW, &l));
        }
    }

    out += &section("▸", "TERMINALS", width);
    for (old, new, cwd) in &s.live_rotation_pairs {
        let msg = format!(
            "! WARNING: {} (handed off) and {} are both running in {} — avoid conflicting edits; type 'c' to clean up",
            sanitize_one_line(old),
            sanitize_one_line(new),
            sanitize_one_line(cwd),
        );
        for l in wrap_plain_words(&msg, "  ", "    ", max_w) {
            out += &format!("{BOLD_RED}{l}{RESET}\n");
        }
    }
    if s.terminals.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(no active terminals)"));
    } else {
        let mut t = terminals_table(width);
        for v in &s.terminals {
            let addr = sanitize_one_line(&v.address);
            let addr_cell = if v.status.starts_with("BLOCKED") {
                Cell::new(addr).fg(Color::Red).add_attribute(Attribute::Bold)
            } else {
                Cell::new(addr).fg(Color::White)
            };
            let name_clean = sanitize_one_line(&v.name);
            let name_cell = if name_clean.is_empty() {
                Cell::new("-").fg(Color::DarkGrey)
            } else {
                Cell::new(name_clean).fg(Color::White)
            };
            let task_clean = sanitize_one_line(&v.task);
            let task_cell = if task_clean.is_empty() {
                Cell::new("-").fg(Color::DarkGrey)
            } else {
                Cell::new(task_clean).fg(Color::Cyan)
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
        for l in wrap_plain_words("(none set — agm budget set task:<ID> --soft … --hard …)", "  ", "  ", max_w) {
            out += &format!("{}\n", paint(GRAY, &l));
        }
    }
    let scope_w = if width >= 76 { 24 } else { 14 };
    for b in &s.budgets {
        let pct = b
            .value
            .filter(|v| v.is_finite() && b.hard.is_finite() && b.hard > 0.0)
            .map(|v| v / b.hard * 100.0);
        let (bar_col, flag) = match b.value.filter(|v| v.is_finite() && b.hard.is_finite() && b.hard > 0.0) {
            Some(v) if v >= b.hard => (BRIGHT_RED, paint(BOLD_RED, " HARD")),
            Some(v) if b.soft.is_finite() && v >= b.soft => (BRIGHT_YELLOW, paint(BRIGHT_YELLOW, " soft")),
            _ => (BRIGHT_GREEN, String::new()),
        };
        // Missing or non-finite value → "?" (never render a fake 0%). Percent budgets keep decimals.
        let num = |v: f64| if b.unit == "tokens" { tokens(v.max(0.0) as u64) } else { format!("{v:.1}") };
        let bar_str = pct.map(|p| colored_bar(p, bar_col)).unwrap_or_else(|| paint(GRAY, "··········"));
        let pct_str = match pct {
            Some(p) if p > 999.0 => paint(bar_col, ">999%"),
            Some(p) => paint(bar_col, &format!("{p:>3.0}%")),
            None => paint(GRAY, "   ?"),
        };
        let scope_padded = pad_cells(&one_line(&b.scope, scope_w), scope_w);
        let val_str = b.value.filter(|v| v.is_finite()).map(num).unwrap_or_else(|| "?".into());
        let hard_str = if b.hard.is_finite() { num(b.hard) } else { "?".into() };
        let unit = sanitize_one_line(&b.unit);
        out += &format!(
            "  {} {bar_str} {pct_str}  {val_str} / {hard_str} {}{flag}\n",
            paint(WHITE, &scope_padded),
            paint(GRAY, &unit),
        );
    }

    let inbox_icon = if s.inbox.is_empty() { "○" } else { "●" };
    out += &section(inbox_icon, "INBOX", width);
    if s.inbox.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(empty)"));
    }
    for i in &s.inbox {
        let term = one_line(i.terminal.as_deref().unwrap_or("-"), max_w / 5);
        let src = one_line(&i.source, max_w / 5);
        let prefix_len = 2 + format!("#{}", i.id).width() + 1 + src.width() + 2 + 1 + term.width() + 3;
        let max_text = max_w.saturating_sub(prefix_len).min(200);
        out += &format!(
            "  {} {} {} {} {}\n",
            paint(BOLD_YELLOW, &format!("#{}", i.id)),
            inbox_source_badge(&src),
            paint(WHITE, &term),
            paint(GRAY, "—"),
            one_line(&i.text, max_text),
        );
    }

    out += &section("»", "RECENT MESSAGES", width);
    if s.messages.is_empty() {
        out += &format!("  {}\n", paint(GRAY, "(no messages)"));
    }
    for m in &s.messages {
        let from = one_line(&m.from, max_w / 6);
        let to = one_line(&m.to, max_w / 6);
        let kind = one_line(&m.kind, max_w / 6);
        let status = one_line(&m.status, max_w / 6);
        let st_col = match status.as_str() {
            "delivered" | "sent" => BRIGHT_GREEN,
            "failed" => BRIGHT_RED,
            "queued" => BRIGHT_YELLOW,
            _ => GRAY,
        };
        let fixed_len = 2
            + from.width()
            + 3
            + to.width()
            + 2
            + kind.width()
            + 2
            + 2
            + 2
            + status.width()
            + 2;
        let max_text = max_w.saturating_sub(fixed_len).min(60);
        out += &format!(
            "  {} {} {}  {}  {}  {}\n",
            paint(CYAN, &from),
            paint(GRAY, "→"),
            paint(WHITE, &to),
            msg_kind_badge(&kind),
            one_line(&m.text, max_text),
            paint(st_col, &format!("({status})")),
        );
    }
    out += &render_footer(width);
    out.lines().map(|line| format!("{}\n", bounded_styled_line(line, max_w))).collect()
}

/// Strip ANSI CSI/OSC/escape sequences and replace newlines/control chars with spaces.
fn sanitize_one_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for ch in chars.by_ref() {
                        if ('@'..='~').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' || (ch == '\x1b' && chars.peek() == Some(&'\\')) {
                            if ch == '\x1b' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                Some(ch) if (' '..='/').contains(&ch) => {
                    chars.next();
                    for final_ch in chars.by_ref() {
                        if ('0'..='~').contains(&final_ch) {
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else if c.is_control() {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Single-line display bounded to terminal cells without splitting Unicode graphemes.
pub fn one_line(s: &str, max: usize) -> String {
    let clean = sanitize_one_line(s);
    if max == 0 {
        return String::new();
    }
    if clean.width() <= max {
        clean
    } else if max == 1 {
        "…".into()
    } else {
        let mut out = String::new();
        let mut cells = 0;
        for grapheme in clean.graphemes(true) {
            let width = grapheme.width();
            if cells + width > max - 1 {
                break;
            }
            out.push_str(grapheme);
            cells += width;
        }
        out.push('…');
        out
    }
}

/// Bound trusted styled output to terminal cells, preserving SGR but no other escapes.
pub fn bounded_styled_line(line: &str, max: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    let mut cells = 0;
    while !rest.is_empty() {
        if let Some(params) = rest.strip_prefix("\x1b[")
            && let Some(end) = params.find('m')
            && params[..end].bytes().all(|b| b.is_ascii_digit() || b == b';' || b == b':')
        {
            let consumed = 2 + end + 1;
            out.push_str(&rest[..consumed]);
            rest = &rest[consumed..];
            continue;
        }
        if rest.starts_with('\x1b') {
            out.push_str(&one_line(rest, max.saturating_sub(cells)));
            break;
        }
        let end = rest.find('\x1b').unwrap_or(rest.len());
        for grapheme in rest[..end].graphemes(true) {
            let width = grapheme.width();
            if cells + width > max {
                out.push_str(RESET);
                return out;
            }
            out.push_str(grapheme);
            cells += width;
        }
        rest = &rest[end..];
    }
    out.push_str(RESET);
    out
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
