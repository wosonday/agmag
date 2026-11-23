//! Renders the TUI project view (AGENTS, INBOX, MESSAGES) with the dashboard's ANSI helpers.

use comfy_table::Color;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::actions::kind_to_english;
use crate::dashboard::{
    BOLD_CYAN, BOLD_GREEN, BOLD_RED, BOLD_YELLOW, BRIGHT_CYAN, BRIGHT_GREEN, BRIGHT_MAGENTA,
    BRIGHT_RED, BRIGHT_YELLOW, CYAN, GRAY, RESET, SPARK, WHITE, bounded_styled_line, format_age_s,
    inbox_source_badge, msg_kind_badge, one_line, paint, sanitize_one_line, section, status_color,
};
use crate::project::{AgentView, ProjectSummary, ProjectView};
use crate::store::{InboxRow, MessageRow};

/// Widths below this drop the token sparkline.
const NARROW_NO_SPARK: usize = 70;
/// Widths below this also drop the context bar.
const NARROW_NO_CTX: usize = 50;

/// AGENTS + INBOX + MESSAGES sections for one project (no header/footer).
pub fn render_project_body(pv: &ProjectView, width: usize, now_ms: i64) -> String {
    let name = sanitize_one_line(&pv.key.name);
    let mut lines: Vec<String> = Vec::new();

    lines.extend(section_lines(
        "▸",
        &format!("AGENTS · {name} ({})", pv.agents.len()),
        width,
    ));
    if pv.agents.is_empty() {
        lines.push(paint(GRAY, "  (empty)"));
    } else {
        lines.extend(agent_lines(&pv.agents, width, now_ms));
    }

    let inbox_icon = if pv.inbox.is_empty() { "○" } else { "●" };
    lines.extend(section_lines(
        inbox_icon,
        &format!("INBOX · {name} ({})", pv.inbox.len()),
        width,
    ));
    if pv.inbox.is_empty() {
        lines.push(paint(GRAY, "  (empty)"));
    }
    for item in &pv.inbox {
        lines.extend(inbox_lines(item, width, 4));
    }

    lines.extend(section_lines("»", &format!("MESSAGES · {name}"), width));
    if pv.messages.is_empty() {
        lines.push(paint(GRAY, "  (no messages)"));
    }
    for message in &pv.messages {
        lines.extend(message_lines(message, width, now_ms, 3));
    }

    bounded_block(lines, width)
}

/// The `cd` listing: one line per project plus `(no project)` when `no_project > 0`.
pub fn render_project_list(list: &[ProjectSummary], no_project: usize, width: usize) -> String {
    let mut name_w = list
        .iter()
        .map(|p| sanitize_one_line(&p.key.name).width())
        .max()
        .unwrap_or(0);
    if no_project > 0 {
        name_w = name_w.max("(no project)".width());
    }
    let name_w = name_w.clamp(6, 24);
    let row = |name: String, live: String, inbox: String| {
        format!(
            "  {}  {} {} {}",
            paint(WHITE, &pad_cells(&name, name_w)),
            live,
            paint(GRAY, "·"),
            inbox,
        )
    };
    let mut lines: Vec<String> = Vec::new();
    for p in list {
        let name = one_line(&p.key.name, name_w);
        lines.push(row(
            name,
            paint(BOLD_CYAN, &format!("{} live", p.live_agents)),
            paint(BRIGHT_YELLOW, &format!("{} inbox", p.open_inbox)),
        ));
    }
    if no_project > 0 {
        lines.push(row(
            paint(GRAY, &one_line("(no project)", name_w)),
            paint(GRAY, "-"),
            paint(GRAY, &format!("{no_project} inbox")),
        ));
    }
    bounded_block(lines, width)
}

/// One inbox item in full (wrapped, no line cap) for `show <#>`.
pub fn render_inbox_item(item: &InboxRow, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    let terminal = one_line(item.terminal.as_deref().unwrap_or("-"), (max_w / 3).max(3));
    let mut lines = vec![format!(
        "  {} {} {}",
        paint(BOLD_YELLOW, &format!("#{}", item.id)),
        inbox_source_badge(&item.source),
        paint(WHITE, &terminal),
    )];
    lines.extend(wrap_capped(&item.text, "  ", 2, "  ", max_w, usize::MAX));
    bounded_block(lines, width)
}

fn section_lines(icon: &str, title: &str, width: usize) -> Vec<String> {
    section(icon, title, width)
        .trim_end_matches('\n')
        .lines()
        .map(String::from)
        .collect()
}

fn bounded_block(lines: Vec<String>, width: usize) -> String {
    let max_w = width.saturating_sub(1);
    lines
        .into_iter()
        .map(|line| format!("{}\n", bounded_styled_line(&line, max_w)))
        .collect()
}

fn pad_cells(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(text.width())))
}

/// ANSI color matching [`status_color`], so the symbol and word carry the terminal's state color.
fn status_ansi(status: &str) -> &'static str {
    match status_color(status) {
        Color::Red => BRIGHT_RED,
        Color::Yellow => BRIGHT_YELLOW,
        Color::Green => BRIGHT_GREEN,
        Color::Cyan => CYAN,
        Color::White => WHITE,
        _ => GRAY,
    }
}

fn kind_ansi(english: &str) -> &'static str {
    match english {
        "ASSIGN" => BRIGHT_CYAN,
        "REPORT" => BRIGHT_GREEN,
        "PASS" => BOLD_GREEN,
        "QUESTION" => BRIGHT_YELLOW,
        "BLOCKED" => BOLD_RED,
        "ERROR" | "FAIL" => BRIGHT_RED,
        _ => WHITE,
    }
}

fn status_parts(status: &str) -> (&'static str, String) {
    if status.starts_with("BLOCKED") {
        ("◐", "blocked".into())
    } else if status == "working" || status == "running" {
        ("●", status.into())
    } else if status == "idle" {
        ("○", "idle".into())
    } else if status == "done" {
        ("✓", "done".into())
    } else if status == "exited" {
        ("×", "exited".into())
    } else if status.is_empty() || status == "-" {
        ("?", "-".into())
    } else {
        ("?", status.into())
    }
}

fn status_segment(status: &str) -> String {
    let clean = sanitize_one_line(status);
    let (symbol, word) = status_parts(&clean);
    let color = status_ansi(&clean);
    format!("{} {}", paint(color, symbol), paint(color, &word))
}

/// 8-cell context bar; `ctx -` when the percentage is unknown.
fn ctx_segment(pct: Option<f64>) -> String {
    let Some(pct) = pct.filter(|p| p.is_finite()) else {
        return format!("{} {}", paint(GRAY, "ctx"), paint(GRAY, "-"));
    };
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * 8.0).round() as usize;
    let color = if pct >= 85.0 {
        BRIGHT_RED
    } else if pct >= 60.0 {
        BRIGHT_YELLOW
    } else {
        BRIGHT_GREEN
    };
    let bar = format!(
        "{color}{}{GRAY}{}{RESET}",
        "█".repeat(filled),
        "░".repeat(8 - filled)
    );
    format!(
        "{} {} {}",
        paint(GRAY, "ctx"),
        bar,
        paint(color, &format!("{pct:.0}%"))
    )
}

/// 12 glyphs scaled to the busiest bucket; all zero stays flat on `▁`.
fn spark_glyphs(buckets: &[u64]) -> String {
    let max = buckets.iter().copied().max().unwrap_or(0);
    buckets
        .iter()
        .map(|&value| {
            let index = if max == 0 {
                0
            } else {
                ((value.min(max) as f64 / max as f64) * 7.0).round() as usize
            };
            SPARK[index]
        })
        .collect()
}

fn tokens_label(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

fn age_label(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        format!("{secs}s")
    } else {
        format_age_s(secs)
    }
}

/// `agent` line pair: identity/status/instruments, then `doing: …`.
fn agent_lines(agents: &[AgentView], width: usize, now_ms: i64) -> Vec<String> {
    let mut out = Vec::new();
    // stack[d] = the node at `depth` d has a later sibling, so its subtree continues below.
    let mut stack: Vec<bool> = Vec::new();
    for agent in agents {
        let depth = agent.depth;
        stack.truncate(depth);
        while stack.len() < depth {
            stack.push(false);
        }
        let mut prefix = String::from(" ");
        for more in stack.iter().take(depth) {
            prefix.push_str(if *more { "│ " } else { "  " });
        }
        if depth > 0 {
            prefix.push_str(if agent.last_sibling { "└ " } else { "├ " });
        }
        if stack.len() > depth {
            stack[depth] = !agent.last_sibling;
        } else {
            stack.push(!agent.last_sibling);
        }

        let mut cont = String::from(" ");
        for more in stack.iter().take(depth + 1) {
            cont.push_str(if *more { "│ " } else { "  " });
        }
        cont.push_str("  ");

        out.push(agent_identity_line(agent, &prefix, width));
        out.push(agent_doing_line(agent, &cont, width, now_ms));
    }
    out
}

fn agent_identity_line(agent: &AgentView, prefix: &str, width: usize) -> String {
    let label = sanitize_one_line(&agent.label);
    let mut segments = vec![format!("{prefix}{}", paint(WHITE, &label))];

    let app = sanitize_one_line(&agent.app);
    let model = sanitize_one_line(&agent.model);
    let effort = sanitize_one_line(&agent.effort);
    let model_effort = if model.is_empty() {
        String::new()
    } else if effort.is_empty() {
        model
    } else {
        format!("{model}/{effort}")
    };
    segments.push(match (app.is_empty(), model_effort.is_empty()) {
        (true, true) => paint(GRAY, "-"),
        (false, true) => paint(BRIGHT_MAGENTA, &app),
        (true, false) => paint(WHITE, &model_effort),
        (false, false) => format!(
            "{} {}",
            paint(BRIGHT_MAGENTA, &app),
            paint(WHITE, &model_effort)
        ),
    });

    segments.push(status_segment(&agent.status));
    if width >= NARROW_NO_CTX {
        segments.push(ctx_segment(agent.context_pct));
    }
    if width >= NARROW_NO_SPARK {
        segments.push(match &agent.spark {
            Some(buckets) => paint(GRAY, &spark_glyphs(buckets)),
            None => paint(GRAY, "-"),
        });
    }
    segments.push(match agent.tokens {
        Some(n) => paint(WHITE, &tokens_label(n)),
        None => paint(GRAY, "-"),
    });
    if let Some(stage) = &agent.stage {
        let stage = sanitize_one_line(stage);
        segments.push(format!(
            "{} {}",
            paint(GRAY, "stage:"),
            paint(BRIGHT_MAGENTA, &stage)
        ));
    }
    segments.join("  ")
}

fn agent_doing_line(agent: &AgentView, cont: &str, width: usize, now_ms: i64) -> String {
    if let Some(turn) = &agent.doing {
        let age = format!("{} ago", age_label((now_ms - turn.ts_ms) / 1000));
        let summary = sanitize_one_line(&turn.summary);
        let avail = (width - 1)
            .saturating_sub(cont.width() + "doing:".len() + " · ".len() + age.width())
            .max(1);
        return format!(
            "{cont}{} {} {} {}",
            paint(GRAY, "doing:"),
            paint(WHITE, &one_line(&summary, avail)),
            paint(GRAY, "·"),
            paint(GRAY, &age),
        );
    }
    match &agent.last_msg {
        Some((kind, sent)) => {
            let kind = sanitize_one_line(kind);
            format!(
                "{cont}{} {}  {} {} {}",
                paint(GRAY, "doing:"),
                paint(GRAY, "-"),
                paint(GRAY, "·"),
                paint(GRAY, "last msg:"),
                paint(
                    kind_ansi(&kind),
                    &format!("{kind} {}", if *sent { "sent" } else { "received" })
                ),
            )
        }
        None => format!("{cont}{} {}", paint(GRAY, "doing:"), paint(GRAY, "-")),
    }
}

fn inbox_lines(item: &InboxRow, width: usize, max_lines: usize) -> Vec<String> {
    let max_w = width.saturating_sub(1);
    let terminal = one_line(item.terminal.as_deref().unwrap_or("-"), (max_w / 5).max(3));
    let source = sanitize_one_line(&item.source);
    let id = format!("#{}", item.id);
    let first_prefix = format!(
        "  {} {} {} {} ",
        paint(BOLD_YELLOW, &id),
        inbox_source_badge(&source),
        paint(WHITE, &terminal),
        paint(GRAY, "—"),
    );
    let first_plain = 2 + id.width() + 1 + (source.width() + 2) + 1 + terminal.width() + 1 + 1 + 1;
    wrap_capped(
        &item.text,
        &first_prefix,
        first_plain,
        "      ",
        max_w,
        max_lines,
    )
}

fn message_lines(message: &MessageRow, width: usize, now_ms: i64, max_lines: usize) -> Vec<String> {
    let max_w = width.saturating_sub(1);
    let age = age_label((now_ms - message.ts_ms) / 1000);
    let cap = (max_w / 6).max(3);
    let from = one_line(&message.from, cap);
    let to = one_line(&message.to, cap);
    let kind = sanitize_one_line(kind_to_english(&message.kind));
    let status = sanitize_one_line(&message.status);
    let age_color = match status.as_str() {
        "delivered" | "sent" => BRIGHT_GREEN,
        "failed" => BRIGHT_RED,
        "queued" => BRIGHT_YELLOW,
        _ => GRAY,
    };
    let first_prefix = format!(
        "  {} {} {} {}  {}  ",
        paint(age_color, &age),
        paint(CYAN, &from),
        paint(GRAY, "→"),
        paint(WHITE, &to),
        msg_kind_badge(&message.kind),
    );
    let first_plain =
        2 + age.width() + 1 + from.width() + 1 + 1 + 1 + to.width() + 2 + (kind.width() + 2) + 2;
    wrap_capped(
        &message.text,
        &first_prefix,
        first_plain,
        "      ",
        max_w,
        max_lines,
    )
}

/// Greedy wrap of sanitized text, breaking on spaces when possible; caps at `max_lines` with `…`.
fn wrap_capped(
    text: &str,
    first_prefix: &str,
    first_plain_width: usize,
    cont_prefix: &str,
    max_width: usize,
    max_lines: usize,
) -> Vec<String> {
    let clean = sanitize_one_line(text);
    let graphemes: Vec<&str> = clean.graphemes(true).collect();
    let mut lines: Vec<String> = Vec::new();
    let mut index = 0;
    let mut prefix = first_prefix;
    let mut prefix_width = first_plain_width;
    while index < graphemes.len() && lines.len() < max_lines {
        let avail = max_width.saturating_sub(prefix_width).max(1);
        let mut line = String::new();
        let mut line_width = 0;
        let mut last_space: Option<usize> = None;
        while index < graphemes.len() {
            let grapheme = graphemes[index];
            let grapheme_width = grapheme.width();
            if line_width + grapheme_width > avail {
                break;
            }
            if grapheme == " " {
                last_space = Some(line.len());
            }
            line.push_str(grapheme);
            line_width += grapheme_width;
            index += 1;
        }
        if line.is_empty() && index < graphemes.len() {
            // A single grapheme wider than the pane: keep it and let `bounded_styled_line` cut it.
            line.push_str(graphemes[index]);
            index += 1;
        } else if index < graphemes.len()
            && let Some(space) = last_space
            && space > 0
        {
            let tail = &line[space + 1..];
            index -= tail.graphemes(true).count();
            line.truncate(space);
        }
        lines.push(format!("{prefix}{}", line.trim_end()));
        prefix = cont_prefix;
        prefix_width = cont_prefix.width();
        while index < graphemes.len() && graphemes[index] == " " {
            index += 1;
        }
    }
    if lines.len() >= max_lines
        && index < graphemes.len()
        && let Some(last) = lines.pop()
    {
        lines.push(ellipsize(&last, max_width));
    }
    if lines.is_empty() {
        lines.push(prefix.to_string());
    }
    lines
}

/// Cuts `line` to `max_width` cells and marks it with `…`.
fn ellipsize(line: &str, max_width: usize) -> String {
    let budget = max_width.saturating_sub(1);
    let mut out = String::with_capacity(line.len());
    let mut cells = 0;
    for grapheme in line.graphemes(true) {
        let width = grapheme.width();
        if cells + width > budget {
            break;
        }
        out.push_str(grapheme);
        cells += width;
    }
    out.push('…');
    out
}
