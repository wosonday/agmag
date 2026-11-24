//! Data snapshot and ANSI text rendering for `agm tui` (spec §5.8). Pure — no stdin/stdout I/O.

use anyhow::Result;
use comfy_table::{
    Attribute, Cell, CellAlignment, Color, ContentArrangement, Table, presets::UTF8_FULL_CONDENSED,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::actions::{KINDS, KINDS_EN, kind_to_english};
use crate::config::Config;
use crate::policy::{budget_value, enabled, session_context_pct};
use crate::report::{QuotaRow, UsageBucket, UsageSummary, spark_bar, usage_summary};
use crate::store::{InboxRow, MessageRow, Store};
use crate::sysmon::{CpuStats, SysStats, human_bytes, mount_label};

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const DIM: &str = "\x1b[2m";
pub(crate) const GRAY: &str = "\x1b[90m";
pub(crate) const WHITE: &str = "\x1b[97m";
pub(crate) const CYAN: &str = "\x1b[36m";
pub(crate) const BRIGHT_CYAN: &str = "\x1b[96m";
pub(crate) const BOLD_CYAN: &str = "\x1b[1;96m";
pub(crate) const BRIGHT_GREEN: &str = "\x1b[92m";
pub(crate) const BOLD_GREEN: &str = "\x1b[1;92m";
pub(crate) const BRIGHT_YELLOW: &str = "\x1b[93m";
pub(crate) const BOLD_YELLOW: &str = "\x1b[1;93m";
pub(crate) const BRIGHT_RED: &str = "\x1b[91m";
pub(crate) const BOLD_RED: &str = "\x1b[1;91m";
pub(crate) const BRIGHT_MAGENTA: &str = "\x1b[95m";
pub(crate) const MAGENTA: &str = "\x1b[35m";
pub(crate) const GREEN: &str = "\x1b[32m";
pub(crate) const YELLOW: &str = "\x1b[33m";

#[derive(Debug, Clone, PartialEq)]
pub struct TermView {
    pub address: String,
    pub name: String,
    pub app: String,
    pub status: String,
    pub task: String,
    /// Model of the linked session ("" when unknown), without an effort suffix.
    pub model: String,
    /// Reasoning effort / variant (`high`, `medium`, ...); "" when unknown.
    pub effort: String,
    /// Working directory: the linked session's cwd (where the agent works now), else the terminal's
    /// cwd (the pane's start directory, which herdr keeps even after the agent moves elsewhere).
    pub cwd: String,
    /// herdr location like `hc ls` shows it (`agmag / agm-ceo`); "" when unknown.
    pub place: String,
    /// Latest message sent or received by this terminal: (English kind, sent_by_this_terminal).
    pub last_msg: Option<(String, bool)>,
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
    /// Live RAM/CPU/disk readings; `agm tui` fills this in (the database has none). `None` = not measured.
    pub system: Option<SysStats>,
}

/// Stored `(status, kind)` → the dashboard's status text: `blocked` with a kind becomes
/// `BLOCKED (kind)`, otherwise the stored status, `-` when unknown.
pub fn format_status(status: Option<&str>, kind: Option<&str>) -> String {
    match (status, kind) {
        (Some("blocked"), Some(k)) => format!("BLOCKED ({k})"),
        (Some("blocked"), None) => "BLOCKED".into(),
        (Some(s), _) => s.to_string(),
        (None, _) => "-".into(),
    }
}

/// Messages a snapshot loads (enough to fill a tall Messages panel) and how many `--plain` prints.
pub const SNAPSHOT_MESSAGES: usize = 50;
pub const PLAIN_MESSAGES: usize = 5;

pub fn snapshot(store: &Store, cfg: &Config, now_ms: i64) -> Result<Snapshot> {
    let mut terminals = Vec::new();
    for t in store.terminals(true)? {
        let (tokens, ctx, model, effort, session_cwd) = match t.session {
            Some(sid) => {
                let (n, model, effort, cwd): (i64, Option<String>, Option<String>, Option<String>) = store.conn().query_row(
                    "SELECT (SELECT COALESCE(SUM(input_uncached + cache_read + cache_write + output), 0) FROM usage WHERE session = ?1),
                            model, effort, cwd FROM sessions WHERE id = ?1",
                    [sid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?;
                (
                    Some(n.max(0) as u64),
                    session_context_pct(store, cfg, sid)?,
                    model,
                    effort,
                    cwd,
                )
            }
            None => (None, None, None, None, None),
        };
        let (model, effort) = split_effort(model.as_deref().unwrap_or(""), effort.as_deref());
        // Messages address terminals by address or by name, so match both.
        let name = t
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&t.address);
        let last_msg = store
            .conn()
            .query_row(
                "SELECT kind, from_t IN (?1, ?2) FROM messages
                 WHERE from_t IN (?1, ?2) OR to_t IN (?1, ?2) ORDER BY id DESC LIMIT 1",
                [&t.address, name],
                |r| {
                    Ok((
                        kind_to_english(&r.get::<_, String>(0)?).to_string(),
                        r.get::<_, bool>(1)?,
                    ))
                },
            )
            .ok();
        let mut status = format_status(t.status.as_deref(), t.kind.as_deref());
        if t.handoff_state == "handed_off" {
            status.push_str(" [HANDED OFF]");
        }
        terminals.push(TermView {
            address: t.address,
            name: t.name.unwrap_or_default(),
            app: t.app.unwrap_or_else(|| "?".into()),
            status,
            task: t.task.unwrap_or_default(),
            model,
            effort,
            place: t.labels.get("where").cloned().unwrap_or_default(),
            cwd: session_cwd
                .filter(|c| !c.is_empty())
                .or(t.cwd)
                .unwrap_or_default(),
            last_msg,
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
        budgets.push(BudgetView {
            value: budget_value(store, cfg, &b, now_ms)?,
            scope: b.scope,
            unit: b.unit,
            soft: b.soft,
            hard: b.hard,
        });
    }
    Ok(Snapshot {
        now_s: now_ms / 1000,
        policy_on: enabled(store)?,
        quota: crate::report::quota_with_config(store, cfg, now_ms / 1000)?,
        usage_summary: usage_summary(store, cfg, now_ms)?,
        terminals,
        live_rotation_pairs,
        budgets,
        inbox: store.inbox(true)?,
        // The full-screen app's Messages panel grows with free space; `--plain` still shows the newest 5.
        messages: store.messages(SNAPSHOT_MESSAGES)?,
        system: None,
    })
}

pub(crate) fn paint(style: &str, text: &str) -> String {
    format!("{style}{text}{RESET}")
}

fn pad_cells(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(text.width())))
}

/// Định dạng tuổi dữ liệu (giây) cho cả TUI và bảng `agm quota`: `1d05h`, `2h30m`, `12m`.
pub fn format_age_s(s: i64) -> String {
    let s = s.max(0);
    let days = s / 86400;
    let hours = (s % 86400) / 3600;
    let mins = (s % 3600) / 60;
    if days > 0 {
        format!("{days}d{hours:02}h")
    } else if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else {
        format!("{mins}m")
    }
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
        format!(
            "{color}{}{GRAY}{}{RESET}",
            "█".repeat(filled),
            "░".repeat(empty)
        )
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

pub(crate) fn section(icon: &str, title: &str, width: usize) -> String {
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
        [
            "name", "agent", "model", "effort", "where", "task", "tokens", "ctx", "last msg",
            "status",
        ]
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

pub(crate) fn status_color(status: &str) -> Color {
    if status.starts_with("BLOCKED") {
        Color::Red
    } else if status.contains("[HANDED OFF]") {
        Color::Yellow
    } else if status == "working" || status == "running" {
        Color::Green
    } else if status == "idle" || status == "done" {
        Color::Cyan
    } else if status == "exited" || status == "-" {
        Color::DarkGrey
    } else {
        Color::White
    }
}

fn status_cell(status: &str) -> Cell {
    let clean = sanitize_one_line(status);
    let bold = clean.starts_with("BLOCKED") || clean == "working" || clean == "running";
    let c = Cell::new(&clean).fg(status_color(&clean));
    if bold {
        c.add_attribute(Attribute::Bold)
    } else {
        c
    }
}

/// Effort levels seen in Codex, Claude Code, OpenCode variants and agy model names.
use crate::config::EFFORT_LEVELS as EFFORTS;

/// Splits `model` / `effort` for display. agy encodes the level in the model name (`gemini-4-argon-high`),
/// so a known level suffix becomes the effort when the log reported none.
pub fn split_effort(model: &str, effort: Option<&str>) -> (String, String) {
    if let Some(e) = effort.filter(|e| !e.is_empty()) {
        return (model.to_string(), e.to_string());
    }
    match model.rsplit_once('-') {
        Some((base, level)) if EFFORTS.contains(&level) && !base.is_empty() => {
            (base.to_string(), level.to_string())
        }
        _ => (model.to_string(), String::new()),
    }
}

fn effort_cell(effort: &str) -> Cell {
    let clean = sanitize_one_line(effort);
    let c = Cell::new(if clean.is_empty() { "-" } else { &clean });
    match clean.as_str() {
        "" => c.fg(Color::DarkGrey),
        "xhigh" | "max" => c.fg(Color::Magenta).add_attribute(Attribute::Bold),
        "high" => c.fg(Color::Magenta),
        "medium" => c.fg(Color::Yellow),
        _ => c.fg(Color::Green),
    }
}

/// Width of the terminal name in the TERMINALS table (after the `● ` dot); longer names end with `…`.
const NAME_CELLS: usize = 12;

/// Width of the `where` cell; longer locations end with `…`.
const WHERE_CELLS: usize = 24;

/// `where` cell: herdr's `workspace / tab` (as in `hc ls`), else the working folder name.
pub fn where_label(place: &str, cwd: &str) -> String {
    let place = sanitize_one_line(place);
    if place.trim().is_empty() {
        folder_name(cwd)
    } else {
        place.trim().to_string()
    }
}

/// Working folder only: the last path component (`D:\Project\agmag\.wt\login-1` → `login-1`), any separator.
pub fn folder_name(path: &str) -> String {
    let clean = sanitize_one_line(path);
    let trimmed = clean.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
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

pub(crate) fn inbox_source_badge(source: &str) -> String {
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

/// One color scheme for message kinds (badge in RECENT MESSAGES and `last msg` cell): ANSI code, table color, bold.
fn kind_style(english: &str) -> (&'static str, Color, bool) {
    match english {
        "ASSIGN" => (BRIGHT_CYAN, Color::Cyan, false),
        "REPORT" => (BRIGHT_GREEN, Color::Green, false),
        "PASS" => (BOLD_GREEN, Color::Green, true),
        "QUESTION" => (BRIGHT_YELLOW, Color::Yellow, false),
        "BLOCKED" => (BOLD_RED, Color::Red, true),
        "ERROR" | "FAIL" => (BRIGHT_RED, Color::Red, false),
        _ => (WHITE, Color::White, false),
    }
}

pub(crate) fn msg_kind_badge(kind: &str) -> String {
    let clean = sanitize_one_line(kind_to_english(kind));
    paint(kind_style(&clean).0, &format!("[{clean}]"))
}

/// `→ REPORT`: this terminal sent it; `← ASSIGN`: it received it.
fn last_msg_cell(last: Option<&(String, bool)>) -> Cell {
    let Some((kind, sent)) = last else {
        return Cell::new("-").fg(Color::DarkGrey);
    };
    let clean = sanitize_one_line(kind);
    let (_, color, bold) = kind_style(&clean);
    let c = Cell::new(format!("{} {clean}", if *sent { "→" } else { "←" })).fg(color);
    if bold {
        c.add_attribute(Attribute::Bold)
    } else {
        c
    }
}

fn wrap_styled_items(
    items: &[(usize, String)],
    sep: &str,
    sep_plain_len: usize,
    indent: usize,
    max_width: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut cur_len = indent;
    for (plain_len, styled) in items {
        let add_len = if cur.is_empty() {
            *plain_len
        } else {
            sep_plain_len + *plain_len
        };
        if !cur.is_empty() && cur_len + add_len > max_width {
            lines.push(cur.join(sep));
            cur.clear();
            cur_len = indent;
        }
        cur_len += if cur.is_empty() {
            *plain_len
        } else {
            sep_plain_len + *plain_len
        };
        cur.push(styled.as_str());
    }
    if !cur.is_empty() {
        lines.push(cur.join(sep));
    }
    lines
}

fn wrap_plain_words(
    text: &str,
    first_prefix: &str,
    cont_prefix: &str,
    max_width: usize,
) -> Vec<String> {
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
            cur = format!(
                "{cont_prefix}{}",
                one_line(word, max_width.saturating_sub(cont_prefix.width()))
            );
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

pub(crate) fn render_header(s: &Snapshot, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    let policy_plain = if s.policy_on {
        "● POLICY ON"
    } else {
        "○ POLICY OFF"
    };
    let policy_styled = if s.policy_on {
        paint(BOLD_GREEN, policy_plain)
    } else {
        paint(BOLD_RED, policy_plain)
    };
    let brand_and_policy = (
        7 + 2 + policy_plain.chars().count(), // "◆ agmag  ● POLICY ON"
        format!("{}  {policy_styled}", paint(BOLD_CYAN, "◆ agmag")),
    );

    let blocked_cnt = s
        .terminals
        .iter()
        .filter(|t| t.status.starts_with("BLOCKED"))
        .count();
    let term_noun = if s.terminals.len() == 1 {
        "terminal"
    } else {
        "terminals"
    };
    let term_item = if blocked_cnt > 0 {
        let blk_txt = format!("({blocked_cnt} blocked)");
        let plain = format!("{} {term_noun} {blk_txt}", s.terminals.len());
        (
            plain.chars().count(),
            format!(
                "{} {}",
                paint(WHITE, &format!("{} {term_noun}", s.terminals.len())),
                paint(BOLD_RED, &blk_txt)
            ),
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
    let mut items = vec![
        brand_and_policy,
        term_item,
        (inbox_plain.chars().count(), inbox_styled),
    ];
    if let Some(sys) = &s.system {
        items.push(system_summary(sys));
    }
    items.push((offset_plain.chars().count(), paint(GRAY, &offset_plain)));
    let mut out = String::new();
    for line in wrap_styled_items(&items, &sep, 3, 1, max_w) {
        out += &format!(" {line}\n");
    }
    out
}

/// `cpu 23% ram 57% C: 71%` for the header: plain length + styled text, each figure colored by load.
fn system_summary(sys: &SysStats) -> (usize, String) {
    let mut parts: Vec<(String, Option<f64>)> = vec![
        ("cpu".into(), sys.cpu.as_ref().map(|c| c.usage_percent)),
        (
            "ram".into(),
            sys.memory.as_ref().and_then(|m| m.used_percent),
        ),
    ];
    if let Some(d) = sys.fullest_disk() {
        parts.push((sanitize_one_line(&d.label()), d.used_percent));
    }
    let mut plain = Vec::new();
    let mut styled = Vec::new();
    for (name, p) in parts {
        let value = p.map_or_else(|| "n/a".to_string(), |v| format!("{v:.0}%"));
        let color = p.map_or(GRAY, pct_color);
        plain.push(format!("{name} {value}"));
        styled.push(format!("{} {}", paint(GRAY, &name), paint(color, &value)));
    }
    (plain.join(" ").chars().count(), styled.join(" "))
}

fn opt_pct_label(p: Option<f64>) -> String {
    p.map_or_else(|| " n/a".into(), |v| format!("{v:>3.0}%"))
}

fn opt_bar(p: Option<f64>) -> String {
    match p {
        Some(v) => colored_bar(v, pct_color(v)),
        None => paint(GRAY, "··········"),
    }
}

/// Same hues as the agent column of TERMINALS; desktop apps use the darker shade of their CLI's color.
fn agent_color(agent: &str) -> &'static str {
    match agent {
        "claude-code" => BRIGHT_MAGENTA,
        "claude-app" => MAGENTA,
        "codex" => BRIGHT_GREEN,
        "codex-app" => GREEN,
        "opencode" => BRIGHT_CYAN,
        "agy" => BRIGHT_YELLOW,
        "agy-app" => YELLOW,
        _ => WHITE,
    }
}

fn rate_label(r: Option<f64>) -> String {
    r.map_or_else(|| "-".into(), |v| format!("{}/s", human_bytes(v as u64)))
}

/// One-line load verdict from the busiest of CPU and RAM, plus a warning for a nearly full fixed disk.
fn system_verdict(sys: &SysStats) -> String {
    let cpu = sys.cpu.as_ref().map(|c| c.usage_percent);
    let ram = sys.memory.as_ref().and_then(|m| m.used_percent);
    let worst = cpu
        .into_iter()
        .chain(ram)
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
    let (style, badge, advice) = match worst {
        None => (GRAY, "○ UNKNOWN", "measuring…"),
        Some(v) if v >= 85.0 => (BOLD_RED, "● HIGH LOAD", "hold off spawning new workers"),
        Some(v) if v >= 60.0 => (BOLD_YELLOW, "● BUSY", "spawn new workers with care"),
        Some(_) => (BOLD_GREEN, "● OK", "room for more workers"),
    };
    let figure = |name: &str, p: Option<f64>| match p {
        Some(v) => format!(
            "{} {}",
            paint(GRAY, name),
            paint(pct_color(v), &format!("{v:.0}%"))
        ),
        None => format!("{} {}", paint(GRAY, name), paint(GRAY, "n/a")),
    };
    let mut line = format!(
        "  {}  {} {} {}  {} {}",
        paint(style, badge),
        figure("cpu", cpu),
        paint(GRAY, "·"),
        figure("ram", ram),
        paint(GRAY, "→"),
        paint(WHITE, advice)
    );
    if let Some(d) = sys
        .fullest_disk()
        .filter(|d| d.used_percent.is_some_and(|p| p >= 90.0))
    {
        line += &format!(
            "  {}",
            paint(
                BOLD_RED,
                &format!("⚠ {} nearly full", sanitize_one_line(&d.label()))
            )
        );
    }
    line
}

/// `  ▣ CPU    ████████░░  83%  detail`: icon, padded name, load-colored bar and figure, then the detail.
fn meter_line(icon: &str, name: &str, name_w: usize, p: Option<f64>, detail: &str) -> String {
    format!(
        "  {} {} {} {}  {detail}",
        paint(BRIGHT_CYAN, icon),
        paint(WHITE, &pad_cells(&one_line(name, name_w), name_w)),
        opt_bar(p),
        paint(p.map_or(GRAY, pct_color), &opt_pct_label(p)),
    )
}

fn used_of(used: u64, total: u64) -> String {
    format!(
        "{} {}",
        paint(WHITE, &human_bytes(used)),
        paint(GRAY, &format!("/ {}", human_bytes(total)))
    )
}

/// Column where meter bars start (`  ▣ CPU    `), so core rows line up under the CPU bar.
const METER_INDENT: usize = 11;
pub(crate) const SPARK: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// `12 cores · busiest #3 97% · 4 idle (<10%)`: the CPU line's detail.
fn cpu_detail(c: &CpuStats) -> String {
    let cores = paint(GRAY, &format!("{} cores", c.logical_cores));
    let Some((i, p)) = c
        .per_core_percent
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
    else {
        return cores;
    };
    let idle = c.per_core_percent.iter().filter(|&&p| p < 10.0).count();
    format!(
        "{cores} {} {} {}",
        paint(GRAY, &format!("· busiest #{i}")),
        paint(pct_color(p), &format!("{p:.0}%")),
        paint(GRAY, &format!("· {idle} idle")),
    )
}

/// TUI: one colored column per logical core, `▁` idle … `█` busy, under the CPU bar.
fn core_spark_line(c: &CpuStats) -> Option<String> {
    if c.per_core_percent.is_empty() {
        return None;
    }
    let spark: String = c
        .per_core_percent
        .iter()
        .map(|&p| {
            paint(
                pct_color(p),
                SPARK[((p.clamp(0.0, 100.0) / 100.0) * 7.0).round() as usize],
            )
        })
        .collect();
    Some(format!(
        "  {} {} {spark}",
        paint(BRIGHT_CYAN, "┊"),
        paint(GRAY, &pad_cells("cores", 6))
    ))
}

/// `agm sys`: `#0 ██████▌░  81%` for every logical core, as many per row as fit in `max_w`.
fn core_grid_lines(c: &CpuStats, max_w: usize) -> Vec<String> {
    let n = c.per_core_percent.len();
    if n == 0 {
        return Vec::new();
    }
    let label_w = format!("#{}", n - 1).len();
    let cell_w = label_w + 1 + 8 + 1 + 4;
    let gap = 3;
    let cols = (max_w.saturating_sub(METER_INDENT) + gap) / (cell_w + gap);
    let cols = cols.clamp(1, n);
    let cells: Vec<String> = c
        .per_core_percent
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let col = pct_color(p);
            format!(
                "{} {} {}",
                paint(GRAY, &format!("{:<label_w$}", format!("#{i}"))),
                fine_bar(p, col, 8),
                paint(col, &format!("{p:>3.0}%"))
            )
        })
        .collect();
    cells
        .chunks(cols)
        .map(|row| format!("{}{}", " ".repeat(METER_INDENT), row.join(&" ".repeat(gap))))
        .collect()
}

/// Machine meters. `core_grid_w`: `None` = one spark line for the cores (TUI), `Some(w)` = full grid (`agm sys`).
fn machine_lines(sys: &SysStats, core_grid_w: Option<usize>) -> Vec<String> {
    let mut out = Vec::new();
    let cpu = sys.cpu.as_ref();
    let detail = cpu.map_or_else(|| paint(GRAY, "measuring…"), cpu_detail);
    out.push(meter_line(
        "▣",
        "CPU",
        6,
        cpu.map(|c| c.usage_percent),
        &detail,
    ));
    if let Some(c) = cpu {
        match core_grid_w {
            None => out.extend(core_spark_line(c)),
            Some(w) => out.extend(core_grid_lines(c, w)),
        }
    }
    match &sys.memory {
        Some(m) => {
            out.push(meter_line(
                "▤",
                "RAM",
                6,
                m.used_percent,
                &used_of(m.used_bytes, m.total_bytes),
            ));
            if m.swap_total_bytes > 0 {
                let p = crate::sysmon::pct(m.swap_used_bytes, m.swap_total_bytes);
                out.push(meter_line(
                    "↺",
                    "swap",
                    6,
                    p,
                    &used_of(m.swap_used_bytes, m.swap_total_bytes),
                ));
            }
        }
        None => out.push(meter_line("▤", "RAM", 6, None, "")),
    }
    out
}

fn disk_lines(sys: &SysStats) -> Vec<String> {
    let name_w = sys
        .disks
        .iter()
        .map(|d| d.label().width())
        .max()
        .unwrap_or(0)
        .clamp(6, 14);
    sys.disks
        .iter()
        .map(|d| {
            let mut detail = format!(
                "{} {}   {} {}  {} {}",
                paint(WHITE, &format!("{} free", human_bytes(d.available_bytes))),
                paint(GRAY, &format!("of {}", human_bytes(d.total_bytes))),
                paint(BRIGHT_GREEN, "↓"),
                paint(GRAY, &rate_label(d.read_bytes_per_s)),
                paint(BRIGHT_YELLOW, "↑"),
                paint(GRAY, &rate_label(d.write_bytes_per_s)),
            );
            if d.removable {
                detail += &format!("  {}", paint(BRIGHT_MAGENTA, "removable"));
            }
            if !d.also_mounted_at.is_empty() {
                let labels: Vec<String> = d
                    .also_mounted_at
                    .iter()
                    .map(|m| sanitize_one_line(&mount_label(m)))
                    .collect();
                detail += &format!("  {}", paint(DIM, &format!("(also {})", labels.join(", "))));
            }
            meter_line(
                "◫",
                &sanitize_one_line(&d.label()),
                name_w,
                d.used_percent,
                &detail,
            )
        })
        .collect()
}

/// `cells`-wide bar with 1/8-cell steps (`▏▎▍▌▋▊▉█`), so shares of a few percent still show.
fn fine_bar(pct: f64, color: &str, cells: usize) -> String {
    const PARTS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let eighths = ((pct.clamp(0.0, 100.0) / 100.0) * (cells * 8) as f64).round() as usize;
    let (full, part) = (eighths / 8, eighths % 8);
    let used_cells = full + usize::from(part > 0);
    format!(
        "{color}{}{}{GRAY}{}{RESET}",
        "█".repeat(full),
        PARTS[part],
        "░".repeat(cells.saturating_sub(used_cells))
    )
}

fn agent_lines(sys: &SysStats) -> Vec<String> {
    if sys.agents.is_empty() {
        return vec![paint(GRAY, "  (no agent processes running)")];
    }
    let total_ram = sys.memory.as_ref().map_or(0, |m| m.total_bytes);
    let share = |bytes: u64| crate::sysmon::pct(bytes, total_ram);
    let mut out = vec![paint(
        GRAY,
        &format!(
            "    {:<12} {:<15} {:>8} {:>6} {:>7}",
            "agent", "share of RAM", "RAM", "procs", "CPU"
        ),
    )];
    for a in &sys.agents {
        let color = agent_color(&a.agent);
        let bar = match share(a.memory_bytes) {
            Some(p) => format!(
                "{} {}",
                fine_bar(p, color, 10),
                paint(color, &format!("{p:>3.0}%"))
            ),
            None => format!("{} {}", paint(GRAY, "··········"), paint(GRAY, " n/a")),
        };
        let cpu = match a.cpu_percent {
            Some(c) => paint(pct_color(c), &format!("{:>7}", format!("{c:.1}%"))),
            None => paint(GRAY, &format!("{:>7}", "n/a")),
        };
        out.push(format!(
            "  {} {} {bar} {} {} {cpu}",
            paint(color, "●"),
            paint(color, &pad_cells(&one_line(&a.agent, 12), 12)),
            paint(WHITE, &format!("{:>8}", human_bytes(a.memory_bytes))),
            paint(GRAY, &format!("{:>6}", a.processes)),
        ));
    }
    let mem: u64 = sys.agents.iter().map(|a| a.memory_bytes).sum();
    let procs: usize = sys.agents.iter().map(|a| a.processes).sum();
    let cpu: Option<f64> = sys.agents.iter().map(|a| a.cpu_percent).sum();
    let share_label = share(mem).map_or_else(|| "n/a".to_string(), |p| format!("{p:.0}% of RAM"));
    let cpu_label = cpu.map_or_else(|| "n/a".to_string(), |c| format!("{c:.1}%"));
    out.push(paint(
        BOLD_CYAN,
        &format!(
            "  Σ {} {} {:>8} {procs:>6} {cpu_label:>7}",
            pad_cells("SUM", 12),
            pad_cells(&share_label, 15),
            human_bytes(mem)
        ),
    ));
    out
}

fn bounded_block(lines: Vec<String>, max_w: usize) -> String {
    lines
        .into_iter()
        .map(|l| format!("{}\n", bounded_styled_line(&l, max_w)))
        .collect()
}

/// Body of the TUI SYSTEM section: verdict, machine meters, disks, agents (no sub-titles, to save lines).
fn render_system(sys: Option<&SysStats>, max_w: usize) -> String {
    let Some(sys) = sys else {
        return format!(
            "{}\n",
            paint(GRAY, "  (system metrics unavailable on this platform)")
        );
    };
    let mut lines = vec![system_verdict(sys)];
    lines.extend(machine_lines(sys, None));
    lines.extend(disk_lines(sys));
    lines.extend(agent_lines(sys));
    bounded_block(lines, max_w)
}

/// Full colored report for `agm sys`: MACHINE, DISKS and AGENTS sections, every line bounded to `width`.
pub fn render_sys_report(sys: &SysStats, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    let mut out = section("◉", "MACHINE", width);
    let mut machine = vec![system_verdict(sys)];
    machine.extend(machine_lines(sys, Some(max_w)));
    out += &bounded_block(machine, max_w);
    if !sys.disks.is_empty() {
        out += &section("◫", "DISKS", width);
        out += &bounded_block(disk_lines(sys), max_w);
    }
    out += &section("◆", "AGENTS (each with its whole process tree)", width);
    out += &bounded_block(agent_lines(sys), max_w);
    out
}

pub(crate) fn render_footer(width: usize) -> String {
    let cmd = |k: &str, args: &str| {
        if args.is_empty() {
            paint(BOLD_CYAN, k)
        } else {
            format!("{} {}", paint(BOLD_CYAN, k), paint(DIM, args))
        }
    };
    let sep = paint(GRAY, " · ");
    let items: Vec<(usize, String)> = TUI_COMMANDS
        .iter()
        .map(|(k, _, args)| {
            let plain_len = k.chars().count()
                + if args.is_empty() {
                    0
                } else {
                    1 + args.chars().count()
                };
            (plain_len, cmd(k, args))
        })
        .collect();
    let lines = wrap_styled_items(&items, &sep, 3, 5, width.saturating_sub(1));
    let rule = paint(GRAY, &"─".repeat(width.saturating_sub(1)));
    let mut out = format!("{rule}\n");
    for (idx, l) in lines.into_iter().enumerate() {
        let prefix = if idx == 0 {
            paint(GRAY, "cmd:")
        } else {
            "    ".into()
        };
        out += &format!("{prefix} {l}\n");
    }
    // Message types for `send`, one styled item each so they wrap instead of being clipped at narrow widths.
    let kinds: Vec<(usize, String)> = KINDS_EN
        .iter()
        .map(|(en, _)| (en.len(), paint(BOLD_CYAN, en)))
        .collect();
    for (idx, l) in wrap_styled_items(&kinds, " ", 1, 11, width.saturating_sub(1))
        .into_iter()
        .enumerate()
    {
        let prefix = if idx == 0 {
            paint(GRAY, "     TYPE:")
        } else {
            " ".repeat(10)
        };
        out += &format!("{prefix} {l}\n");
    }
    out
}

/// Dashboard sections, in display order. `agm tui` shows at most [`MAX_VIEW`] of them at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Quota,
    Usage,
    Terminals,
    Budgets,
    Inbox,
    System,
    Messages,
}

/// Most sections chosen together with `view` (RECENT MESSAGES is pinned on top of that and never counts).
pub const MAX_VIEW: usize = 3;

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Quota,
        Section::Usage,
        Section::Terminals,
        Section::Budgets,
        Section::Inbox,
        Section::System,
        Section::Messages,
    ];
    /// Sections `view` can pick; RECENT MESSAGES is always shown, just above the command bar.
    pub const SELECTABLE: [Section; 6] = [
        Section::Quota,
        Section::Usage,
        Section::Terminals,
        Section::Budgets,
        Section::Inbox,
        Section::System,
    ];
    pub const DEFAULT: [Section; 3] = [Section::Quota, Section::Terminals, Section::Inbox];

    pub fn name(self) -> &'static str {
        match self {
            Section::Quota => "quota",
            Section::Usage => "usage",
            Section::Terminals => "terminals",
            Section::Budgets => "budgets",
            Section::Inbox => "inbox",
            Section::System => "system",
            Section::Messages => "messages",
        }
    }

    pub fn from_name(name: &str) -> Option<Section> {
        Section::SELECTABLE
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    }

    pub fn names() -> String {
        Section::SELECTABLE.map(Section::name).join("|")
    }
}

/// Parses `view` arguments: 1..=MAX_VIEW distinct section names (any order; shown in display order), or `default`.
pub fn parse_view(args: &str) -> std::result::Result<Vec<Section>, String> {
    let words: Vec<&str> = args.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    if words.len() == 1 && words[0].eq_ignore_ascii_case("default") {
        return Ok(Section::DEFAULT.to_vec());
    }
    if words.is_empty() || words.len() > MAX_VIEW {
        return Err(format!(
            "usage: view <1-{MAX_VIEW} of {}> | view default",
            Section::names()
        ));
    }
    let mut picked = Vec::new();
    for w in words {
        if w.eq_ignore_ascii_case(Section::Messages.name()) {
            return Err(format!(
                "RECENT MESSAGES is always shown; choose up to {MAX_VIEW} of {}",
                Section::names()
            ));
        }
        let Some(sec) = Section::from_name(w) else {
            return Err(format!(
                "unknown section '{}'; choose from {}",
                sanitize_one_line(w),
                Section::names()
            ));
        };
        if !picked.contains(&sec) {
            picked.push(sec);
        }
    }
    Ok(Section::ALL
        .into_iter()
        .filter(|s| picked.contains(s))
        .collect())
}

/// Full-screen ANSI text render of every section (CLI/tests). The TUI uses [`render_view`].
pub fn render(s: &Snapshot, width: usize) -> String {
    render_view(s, width, &Section::ALL, None)
}

/// Header + the chosen sections + RECENT MESSAGES (always, last) + command bar. With `height`, section bodies are
/// shortened (never the header or command bar) so the frame fits and the pane never scrolls; [`FRAME_RESERVE`]
/// lines stay free for the note and prompt.
pub fn render_view(
    s: &Snapshot,
    width: usize,
    sections: &[Section],
    height: Option<usize>,
) -> String {
    let max_w = width.saturating_sub(1);
    let header = render_header(s, width);
    let footer = render_footer(width);
    let shown = sections
        .iter()
        .copied()
        .filter(|sec| *sec != Section::Messages)
        .chain([Section::Messages]);
    let mut blocks: Vec<Vec<String>> = shown
        .map(|sec| {
            render_section(s, sec, width)
                .lines()
                .map(String::from)
                .collect()
        })
        .collect();
    if let Some(h) = height {
        let fixed = header.lines().count() + footer.lines().count() + FRAME_RESERVE;
        fit_blocks(&mut blocks, h.saturating_sub(fixed));
    }
    let mut out = header;
    for line in blocks.iter().flatten() {
        out.push_str(line);
        out.push('\n');
    }
    out += &footer;
    out.lines()
        .map(|line| format!("{}\n", bounded_styled_line(line, max_w)))
        .collect()
}

/// Lines below the command bar that `agm tui` writes itself (note + `agm>` prompt).
const FRAME_RESERVE: usize = 2;

/// Header + a project body (AGENTS/INBOX/MESSAGES) + footer, every line bounded to the pane like
/// [`render_view`]. The body is already bounded by the project renderer; the header and footer are not.
pub fn render_scoped(s: &Snapshot, body: &str, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    let mut out = render_header(s, width);
    for line in body.lines() {
        out.push_str(line);
        out.push('\n');
    }
    out += &render_footer(width);
    out.lines()
        .map(|line| format!("{}\n", bounded_styled_line(line, max_w)))
        .collect()
}

/// Shrinks blocks to `budget` lines total. Small blocks keep every line; large ones share the rest evenly and end
/// with a "… N more lines" marker. Each block keeps at least its title line.
fn fit_blocks(blocks: &mut [Vec<String>], budget: usize) {
    let total: usize = blocks.iter().map(Vec::len).sum();
    if total <= budget || blocks.is_empty() {
        return;
    }
    let mut alloc = vec![0usize; blocks.len()];
    let mut left = budget;
    let mut open: Vec<usize> = (0..blocks.len()).collect();
    // Water-filling: blocks smaller than the fair share are kept whole; their unused share goes to the others.
    while !open.is_empty() {
        let share = left / open.len();
        let (small, big): (Vec<usize>, Vec<usize>) =
            open.iter().partition(|&&i| blocks[i].len() <= share);
        if small.is_empty() {
            for (k, &i) in big.iter().enumerate() {
                alloc[i] = share + usize::from(k < left % big.len());
            }
            break;
        }
        for &i in &small {
            alloc[i] = blocks[i].len();
            left -= blocks[i].len();
        }
        open = big;
    }
    for (block, keep) in blocks.iter_mut().zip(alloc) {
        if block.len() <= keep {
            continue;
        }
        let keep = keep.max(2);
        let hidden = block.len() - (keep - 1);
        block.truncate(keep - 1);
        block.push(paint(
            GRAY,
            &format!("  … {hidden} more lines (widen the pane or `view` fewer sections)"),
        ));
    }
}

/// `render_section` without its first (header) line: the body a titled panel shows.
pub fn render_section_body(s: &Snapshot, sec: Section, width: usize) -> String {
    let full = render_section(s, sec, width);
    match full.split_once('\n') {
        Some((_, body)) => body.to_string(),
        None => String::new(),
    }
}

fn render_section(s: &Snapshot, sec: Section, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    let mut out = String::new();
    match sec {
        Section::Quota => {
            // Hạn mức hiển thị theo phần CÒN LẠI (giảm dần từ 100%, như Codex `/status` và agy `/usage`).
            out += &section("◈", "PLAN QUOTA (left)", width);
            if s.quota.is_empty() {
                for l in wrap_plain_words(
                    "(no quota data — run `agm scan`; Claude needs `agm hook install`)",
                    "  ",
                    "  ",
                    max_w,
                ) {
                    out += &format!("{}\n", paint(GRAY, &l));
                }
            }
            for q in &s.quota {
                let win = if q.window_minutes == 43200 {
                    "mo".to_string()
                } else if q.window_minutes % 1440 == 0 {
                    format!("{}d", q.window_minutes / 1440)
                } else {
                    format!("{}h", q.window_minutes / 60)
                };
                let reset = if q.reset_passed {
                    paint(BRIGHT_GREEN, "reset ready")
                } else {
                    let left = q.resets_at_s - s.now_s;
                    paint(
                        CYAN,
                        &format!("resets in {}h{:02}m", left / 3600, left % 3600 / 60),
                    )
                };
                // 11 cột: đủ cho `opencode-go`; vẫn gọn ở 60 cột.
                let prov = one_line(&q.provider, 11);
                let empty_bar = paint(GRAY, "░░░░░░░░░░");
                let limited_tag = if q.limited {
                    format!(" {}", paint(BRIGHT_RED, "limited"))
                } else {
                    String::new()
                };
                if q.stale {
                    let age = format_age_s(q.observed_age_s);
                    out += &format!(
                        "  {} {}  {} {}{limited_tag}  {reset}\n",
                        paint(WHITE, &pad_cells(&prov, 11)),
                        paint(CYAN, &format!("{:>3}", win)),
                        empty_bar,
                        paint(GRAY, &format!("stale · seen {age} ago")),
                    );
                } else if q.left_percent.is_finite() {
                    // Màu vẫn theo mức đã dùng (còn ít → đỏ); thanh và con số theo phần còn lại.
                    let col = pct_color(100.0 - q.left_percent);
                    let pct_str = paint(col, &format!("{:>3.0}% left", q.left_percent));
                    out += &format!(
                        "  {} {}  {} {pct_str}{limited_tag}  {reset}\n",
                        paint(WHITE, &pad_cells(&prov, 11)),
                        paint(CYAN, &format!("{:>3}", win)),
                        colored_bar(q.left_percent, col),
                    );
                } else {
                    out += &format!(
                        "  {} {}  {} {}{limited_tag}  {reset}\n",
                        paint(WHITE, &pad_cells(&prov, 11)),
                        paint(CYAN, &format!("{:>3}", win)),
                        empty_bar,
                        paint(GRAY, "   ?"),
                    );
                }
            }
        }
        Section::Usage => {
            out += &section(
                "▪",
                &format!("USAGE ({})", s.usage_summary.offset_label),
                width,
            );
            let fmt_bucket = |label: &str, b: &UsageBucket| -> (usize, String) {
                let tok = tokens(b.usage.total());
                // Worth at list API prices (same figure as `agm cost`); hidden when nothing in the window has a price.
                let priced = b.usage.total() > b.unpriced_tokens;
                let usd_plain = if priced {
                    format!(
                        " ({})",
                        crate::report::value_label(Some(b.value_usd), b.unpriced_tokens)
                    )
                } else {
                    String::new()
                };
                let usd_styled = if usd_plain.is_empty() {
                    String::new()
                } else {
                    paint(BRIGHT_GREEN, &usd_plain)
                };
                let plain_len =
                    label.chars().count() + 1 + tok.chars().count() + usd_plain.chars().count();
                (
                    plain_len,
                    format!("{} {}{usd_styled}", paint(CYAN, label), paint(WHITE, &tok)),
                )
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

            let max_m = s
                .usage_summary
                .recent_months
                .iter()
                .map(|m| m.usage.total())
                .max()
                .unwrap_or(0);
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
                        [y, mo]
                            if cross_year
                                && y.len() == 4
                                && y.bytes().all(|b| b.is_ascii_digit()) =>
                        {
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
                        if is_latest {
                            paint(WHITE, &lbl)
                        } else {
                            paint(GRAY, &lbl)
                        },
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
        }
        Section::Terminals => {
            out += &section("▸", "TERMINALS", width);
            for (old, new, cwd) in &s.live_rotation_pairs {
                let msg = format!(
                    "! WARNING: {} (handed off) and {} are both running in {} — avoid conflicting edits; type 'cleanup' to clean up",
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
                    // Like `hc`: a colored dot for the state, then the terminal name (address when unnamed).
                    let name_clean = sanitize_one_line(&v.name);
                    let label = if name_clean.is_empty() {
                        sanitize_one_line(&v.address)
                    } else {
                        name_clean
                    };
                    let name_cell = Cell::new(format!("● {}", one_line(&label, NAME_CELLS)))
                        .fg(status_color(&v.status));
                    let name_cell = if v.status.starts_with("BLOCKED") {
                        name_cell.add_attribute(Attribute::Bold)
                    } else {
                        name_cell
                    };
                    let dash_or = |text: &str, color: Color| {
                        let clean = sanitize_one_line(text);
                        if clean.is_empty() {
                            Cell::new("-").fg(Color::DarkGrey)
                        } else {
                            Cell::new(clean).fg(color)
                        }
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
                        name_cell,
                        app_cell(&v.app),
                        dash_or(&v.model, Color::White),
                        effort_cell(&v.effort),
                        dash_or(
                            &one_line(&where_label(&v.place, &v.cwd), WHERE_CELLS),
                            Color::DarkGrey,
                        ),
                        task_cell,
                        tok_cell,
                        ctx_cell(v.context_pct),
                        last_msg_cell(v.last_msg.as_ref()),
                        status_cell(&v.status),
                    ]);
                }
                out += &format!("{t}\n");
            }
        }
        Section::Budgets => {
            out += &section("◆", "BUDGETS", width);
            if s.budgets.is_empty() {
                for l in wrap_plain_words(
                    "(none set — agm budget set task:<ID> --soft … --hard …)",
                    "  ",
                    "  ",
                    max_w,
                ) {
                    out += &format!("{}\n", paint(GRAY, &l));
                }
            }
            let scope_w = if width >= 76 { 24 } else { 14 };
            for b in &s.budgets {
                let pct = b
                    .value
                    .filter(|v| v.is_finite() && b.hard.is_finite() && b.hard > 0.0)
                    .map(|v| v / b.hard * 100.0);
                let (bar_col, flag) = match b
                    .value
                    .filter(|v| v.is_finite() && b.hard.is_finite() && b.hard > 0.0)
                {
                    Some(v) if v >= b.hard => (BRIGHT_RED, paint(BOLD_RED, " HARD")),
                    Some(v) if b.soft.is_finite() && v >= b.soft => {
                        (BRIGHT_YELLOW, paint(BRIGHT_YELLOW, " soft"))
                    }
                    _ => (BRIGHT_GREEN, String::new()),
                };
                // Missing or non-finite value → "?" (never render a fake 0%). Percent budgets keep decimals.
                let num = |v: f64| {
                    if b.unit == "tokens" {
                        tokens(v.max(0.0) as u64)
                    } else {
                        format!("{v:.1}")
                    }
                };
                let bar_str = pct
                    .map(|p| colored_bar(p, bar_col))
                    .unwrap_or_else(|| paint(GRAY, "··········"));
                let pct_str = match pct {
                    Some(p) if p > 999.0 => paint(bar_col, ">999%"),
                    Some(p) => paint(bar_col, &format!("{p:>3.0}%")),
                    None => paint(GRAY, "   ?"),
                };
                let scope_padded = pad_cells(&one_line(&b.scope, scope_w), scope_w);
                let val_str = b
                    .value
                    .filter(|v| v.is_finite())
                    .map(num)
                    .unwrap_or_else(|| "?".into());
                let hard_str = if b.hard.is_finite() {
                    num(b.hard)
                } else {
                    "?".into()
                };
                let unit = sanitize_one_line(&b.unit);
                out += &format!(
                    "  {} {bar_str} {pct_str}  {val_str} / {hard_str} {}{flag}\n",
                    paint(WHITE, &scope_padded),
                    paint(GRAY, &unit),
                );
            }
        }
        Section::Inbox => {
            let inbox_icon = if s.inbox.is_empty() { "○" } else { "●" };
            out += &section(inbox_icon, "INBOX", width);
            if s.inbox.is_empty() {
                out += &format!("  {}\n", paint(GRAY, "(empty)"));
            }
            for i in &s.inbox {
                let term = one_line(i.terminal.as_deref().unwrap_or("-"), max_w / 5);
                let src = one_line(&i.source, max_w / 5);
                let prefix_len =
                    2 + format!("#{}", i.id).width() + 1 + src.width() + 2 + 1 + term.width() + 3;
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
        }
        Section::System => {
            out += &section("◉", "SYSTEM (live · agent = its whole process tree)", width);
            out += &render_system(s.system.as_ref(), max_w);
        }
        Section::Messages => {
            out += &section("»", "RECENT MESSAGES", width);
            if s.messages.is_empty() {
                out += &format!("  {}\n", paint(GRAY, "(no messages)"));
            }
            for m in s.messages.iter().take(PLAIN_MESSAGES) {
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
        }
    }
    out
}

/// Strip ANSI CSI/OSC/escape sequences and replace newlines/control chars with spaces.
pub(crate) fn sanitize_one_line(s: &str) -> String {
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
            && params[..end]
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b';' || b == b':')
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiCmd {
    Refresh,
    Quit,
    Reply(i64, String),
    Done {
        ids: Vec<i64>,
        filters: crate::actions::DoneFilters,
        explicit_source_blocked: bool,
        legacy_single: bool,
    },
    Send(String, String, String),
    Cleanup(Option<String>),
    Interrupt(String),
    Kill(String),
    Policy(bool),
    /// `view` alone shows the current choice; with arguments, the sections to display.
    View(Option<Vec<Section>>),
    /// `cd` alone lists projects; `Some(q)` opens the board for project `q` (`".."` / `"/"` leave the scope).
    Cd(Option<String>),
    /// `cd -t <q>`: the text project view for `q` instead of the board.
    CdText(String),
    /// `show <#>` prints one inbox item in full.
    Show(i64),
}

/// TUI commands: full name shown in the `cmd:` bar, single-letter alias still accepted.
/// `cd`/`show` take no alias (`c` is cleanup, `s` is send): their alias field is empty.
pub const TUI_COMMANDS: [(&str, &str, &str); 11] = [
    ("reply", "r", "<#> <text>"),
    ("done", "d", "<#|a-b|kind:K|term:T>"),
    ("send", "s", "<terminal> <TYPE> <message>"),
    ("cleanup", "c", "[1,2|all]"),
    ("interrupt", "i", "<terminal>"),
    ("kill", "k", "<terminal>"),
    ("policy", "p", "on|off"),
    ("view", "v", "<up to 3 sections>"),
    ("cd", "", "[project|..] (text: -t <project>)"),
    ("show", "", "<#>"),
    ("quit", "q", ""),
];

fn english_kinds() -> String {
    KINDS_EN
        .iter()
        .map(|(en, _)| *en)
        .collect::<Vec<_>>()
        .join("|")
}

fn usage(name: &str) -> String {
    let args = TUI_COMMANDS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map_or("", |(_, _, a)| *a);
    let args = if name == "send" {
        format!("<terminal> <{}> <message>", english_kinds())
    } else {
        args.to_string()
    };
    format!("usage: {name} {args}").trim_end().to_string()
}

/// Splits `<TYPE> <message>` after `send <terminal>`. Returns the stored code and the untouched message text.
/// English types are one word, any case; the legacy Vietnamese codes (incl. `BÁO CÁO`, `CHƯA ĐẠT`) stay accepted.
fn split_kind(after: &str) -> Option<(&'static str, &str)> {
    let (word, text) = after.split_once(' ').unwrap_or((after, ""));
    if let Some(kind) = crate::actions::kind_from_english(word) {
        return Some((kind, text));
    }
    let kind = KINDS
        .iter()
        .filter(|k| {
            after
                .strip_prefix(**k)
                .is_some_and(|r| r.is_empty() || r.starts_with(' '))
        })
        .max_by_key(|k| k.len())?;
    Some((kind, &after[kind.len()..]))
}

fn split_tui_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    for c in s.chars() {
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_quote = Some(c);
                } else if c.is_whitespace() {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                } else {
                    cur.push(c);
                }
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_done_tui(rest: &str) -> std::result::Result<TuiCmd, String> {
    let tokens = split_tui_tokens(rest.trim());
    if tokens.is_empty() {
        return Err(usage("done"));
    }
    let mut id_args = Vec::new();
    let mut filters = crate::actions::DoneFilters::default();
    let mut explicit_source_blocked = false;
    for tok in tokens {
        if let Some(v) = tok
            .strip_prefix("kind:")
            .or_else(|| tok.strip_prefix("KIND:"))
        {
            if v.is_empty() {
                return Err(usage("done"));
            }
            let Some(k) = crate::actions::normalize_done_kind(v) else {
                return Err(format!("unknown kind: {v}"));
            };
            filters.kind = Some(k);
        } else if let Some(v) = tok
            .strip_prefix("term:")
            .or_else(|| tok.strip_prefix("TERM:"))
        {
            if v.is_empty() {
                return Err(usage("done"));
            }
            filters.terminal = Some(v.to_string());
        } else if let Some(v) = tok
            .strip_prefix("source:")
            .or_else(|| tok.strip_prefix("SOURCE:"))
            .or_else(|| tok.strip_prefix("src:"))
            .or_else(|| tok.strip_prefix("SRC:"))
        {
            if v.is_empty() {
                return Err(usage("done"));
            }
            let Some(s) = crate::actions::normalize_done_source(v) else {
                return Err(format!("unknown source: {v}"));
            };
            if s == "blocked" {
                explicit_source_blocked = true;
            }
            filters.source = Some(s);
        } else {
            id_args.push(tok);
        }
    }
    let has_filters = !filters.is_empty();
    if id_args.is_empty() && !has_filters {
        return Err(usage("done"));
    }
    let (ids, legacy_single) = if id_args.is_empty() {
        (Vec::new(), false)
    } else {
        let parsed =
            crate::actions::parse_done_selectors(&id_args).map_err(|e| format!("{e:#}"))?;
        let legacy = parsed.legacy_single && !has_filters;
        (parsed.ids, legacy)
    };
    Ok(TuiCmd::Done {
        ids,
        filters,
        explicit_source_blocked,
        legacy_single,
    })
}
pub fn parse_command(line: &str) -> std::result::Result<TuiCmd, String> {
    let line = line.trim();
    let (head, rest) = line
        .split_once(' ')
        .map_or((line, ""), |(h, r)| (h, r.trim()));
    let Some((name, _, _)) = TUI_COMMANDS
        .iter()
        .find(|(name, alias, _)| head == *name || (!alias.is_empty() && head == *alias))
    else {
        if head.is_empty() {
            return Ok(TuiCmd::Refresh);
        }
        return Err(format!(
            "unknown command '{line}' (commands: {})",
            TUI_COMMANDS.map(|c| c.0).join(", ")
        ));
    };
    Ok(match *name {
        "quit" => TuiCmd::Quit,
        "reply" => {
            let (id, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let Ok(id) = id.parse() else {
                return Err(usage("reply"));
            };
            if text.trim().is_empty() {
                return Err("missing reply text".into());
            }
            TuiCmd::Reply(id, text.trim().into())
        }
        "done" => parse_done_tui(rest)?,
        "send" => {
            let (target, after) = rest.split_once(' ').unwrap_or((rest, ""));
            if target.is_empty() {
                return Err(format!("missing target terminal; {}", usage("send")));
            }
            let Some((kind, text)) = split_kind(after.trim_start()) else {
                return Err(format!("unknown message type; {}", usage("send")));
            };
            let text = text.trim();
            if text.is_empty() {
                return Err(format!("missing message text; {}", usage("send")));
            }
            TuiCmd::Send(target.into(), kind.to_string(), text.into())
        }
        "cleanup" => TuiCmd::Cleanup((!rest.is_empty()).then(|| rest.to_string())),
        "interrupt" if !rest.is_empty() => TuiCmd::Interrupt(rest.into()),
        "kill" if !rest.is_empty() => TuiCmd::Kill(rest.into()),
        "policy" if rest == "on" || rest == "off" => TuiCmd::Policy(rest == "on"),
        "view" if rest.is_empty() => TuiCmd::View(None),
        "view" => TuiCmd::View(Some(parse_view(rest)?)),
        "cd" => {
            // `cd -t <project>` = the text project view; `cd <project>` opens the board.
            let (flag, arg) = rest
                .split_once(' ')
                .map_or((rest, ""), |(flag, arg)| (flag, arg.trim()));
            if flag == "-t" {
                if arg.is_empty() {
                    return Err("usage: cd <project|..> (text view: cd -t <project>)".to_string());
                }
                TuiCmd::CdText(arg.to_string())
            } else {
                TuiCmd::Cd((!rest.is_empty()).then(|| rest.to_string()))
            }
        }
        "show" => rest
            .strip_prefix('#')
            .unwrap_or(rest)
            .parse::<i64>()
            .map(TuiCmd::Show)
            .map_err(|_| usage("show"))?,
        other => return Err(usage(other)),
    })
}

#[cfg(test)]
mod section_body_tests {
    use super::*;

    fn empty() -> Snapshot {
        let store = crate::store::Store::open_in_memory().expect("in-memory store");
        snapshot(&store, &crate::config::Config::builtin(), 1_800_000_000_000).expect("snapshot")
    }

    #[test]
    fn section_body_drops_header() {
        let s = empty();
        for sec in Section::ALL {
            let full = render_section(&s, sec, 100);
            let body = render_section_body(&s, sec, 100);
            let (header, rest) = full.split_once('\n').expect("section has a header line");
            assert!(header.contains("── "), "{sec:?}: {header:?}");
            assert_eq!(body, rest, "{sec:?}");
            assert!(
                !body.contains("── "),
                "{sec:?} body keeps a header: {body:?}"
            );
        }
    }
}
