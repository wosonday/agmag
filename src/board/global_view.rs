//! Rendering of the global screen of the full-screen `agm tui`. No I/O.

use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, HighlightSpacing, Paragraph, Row, Table, TableState,
    Wrap,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::ansi::ansi_to_text;
use super::cmd::NoteKind;
use super::global::{Focus, GlobalData, GlobalState, Popup, visible_sections};
use super::theme::Theme;
use crate::actions::kind_to_english;
use crate::dashboard::{
    Section, Snapshot, TermView, render_section_body, sanitize_one_line, where_label,
};
use crate::project_render::age_label;
use crate::report::QuotaRow;

/// Six message rows plus the panel's top and bottom borders, so wide screens show six messages.
const MESSAGES_H: u16 = 8;
const PROMPT: &str = "agm> ";
const HELP: &str =
    " tab panels · ↑↓ select · ⏎ open · r reply · d done · s send · t theme · ? help · q quit";
const HELP_ASCII: &str = " tab panels | up/down select | enter open | r reply | d done | s send | t theme | ? help | q quit";

pub fn draw_global(
    frame: &mut Frame,
    state: &GlobalState,
    data: &GlobalData,
    theme: &Theme,
    ascii: bool,
) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let ctx = Ctx {
        theme,
        ascii,
        now_ms: data.now_ms,
    };
    let (main, cmd, footer) = split_bottom(area);
    let (sections, messages) = split_messages(main);
    draw_sections(frame, sections, state, data, ctx);
    draw_messages(frame, messages, state, data, ctx);
    if let Some(output) = &data.output {
        draw_output(frame, sections, output, state.output_scroll, ctx);
    }
    let mut cursor = None;
    if state.popup.is_none() && matches!(state.focus, Focus::CmdLine) {
        cursor = draw_cmdline(frame, cmd, state, ctx);
    } else {
        draw_cmdline(frame, cmd, state, ctx);
    }
    draw_footer(frame, footer, state, ctx);
    if let Some(position) = draw_popup(frame, area, state, ctx) {
        cursor = Some(position);
    }
    if let Some(position) = cursor {
        let x = position.x.min(area.right().saturating_sub(1));
        let y = position.y.min(area.bottom().saturating_sub(1));
        frame.set_cursor_position(Position::new(x, y));
    }
}

/// Theme, ASCII flag and clock, passed down instead of three separate arguments.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    theme: &'a Theme,
    ascii: bool,
    now_ms: i64,
}

/// Main content, command bar and footer. With one row only the footer fits.
fn split_bottom(area: Rect) -> (Rect, Rect, Rect) {
    let footer_h = area.height.min(1);
    let cmd_h = area.height.saturating_sub(1).min(1);
    let main_h = area.height.saturating_sub(footer_h + cmd_h);
    let main = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: main_h,
    };
    let cmd = Rect {
        x: area.x,
        y: area.y + main_h,
        width: area.width,
        height: cmd_h,
    };
    let footer = Rect {
        x: area.x,
        y: area.y + main_h + cmd_h,
        width: area.width,
        height: footer_h,
    };
    (main, cmd, footer)
}

/// Sections get everything except the pinned Messages rows at the bottom.
fn split_messages(main: Rect) -> (Rect, Rect) {
    let messages_h = MESSAGES_H.min(main.height);
    let sections = Rect {
        x: main.x,
        y: main.y,
        width: main.width,
        height: main.height - messages_h,
    };
    let messages = Rect {
        x: main.x,
        y: main.y + sections.height,
        width: main.width,
        height: messages_h,
    };
    (sections, messages)
}

fn ascii_borders() -> symbols::border::Set<'static> {
    symbols::border::Set {
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
        vertical_left: "|",
        vertical_right: "|",
        horizontal_top: "-",
        horizontal_bottom: "-",
    }
}

fn panel(title: String, color: Color, ctx: Ctx) -> Block<'static> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::new().fg(color))
        .title_style(Style::new().fg(ctx.theme.title));
    if ctx.ascii {
        block.border_set(ascii_borders())
    } else {
        block.border_type(BorderType::Rounded)
    }
}

fn render_text(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(text.to_string(), style))),
        area,
    );
}

fn muted_line(text: impl Into<String>, ctx: Ctx) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::new().fg(ctx.theme.muted)))
}

fn section_title(sec: Section) -> &'static str {
    match sec {
        Section::Quota => "Quota",
        Section::Usage => "Usage",
        Section::Terminals => "Terminals",
        Section::Budgets => "Budgets",
        Section::Inbox => "Inbox",
        Section::System => "System",
        Section::Messages => "Messages",
    }
}

fn draw_sections(frame: &mut Frame, area: Rect, state: &GlobalState, data: &GlobalData, ctx: Ctx) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut chosen = visible_sections(&data.view);
    // `Messages` is pinned and drawn separately below the section area.
    chosen.pop();
    if chosen.len() == 1 {
        draw_section(frame, area, chosen[0], state, data, ctx);
        return;
    }
    if area.width >= 120 {
        // ≥ 120 columns: first two sections side by side, the rest stacked under them.
        let pair_h = if chosen.len() >= 3 {
            let max_h = area.height.saturating_sub(1).max(1);
            (area.height.saturating_mul(3) / 5)
                .max(1)
                .min(max_h)
                .min(area.height)
        } else {
            area.height
        };
        let pair = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: pair_h,
        };
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(pair);
        draw_section(frame, cols[0], chosen[0], state, data, ctx);
        draw_section(frame, cols[1], chosen[1], state, data, ctx);
        if chosen.len() > 2 {
            let rest = Rect {
                x: area.x,
                y: area.y + pair_h,
                width: area.width,
                height: area.height.saturating_sub(pair_h),
            };
            draw_stacked(frame, rest, &chosen[2..], state, data, ctx);
        }
    } else {
        draw_stacked(frame, area, &chosen, state, data, ctx);
    }
}

/// One section per chunk of the available height (small panels get one row each).
fn draw_stacked(
    frame: &mut Frame,
    area: Rect,
    sections: &[Section],
    state: &GlobalState,
    data: &GlobalData,
    ctx: Ctx,
) {
    if sections.is_empty() || area.height == 0 {
        return;
    }
    let count = sections.len() as u16;
    let base = area.height / count;
    let extra = area.height % count;
    let mut y = area.y;
    for (i, sec) in sections.iter().enumerate() {
        let height = base + u16::from((i as u16) < extra);
        let rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height,
        };
        draw_section(frame, rect, *sec, state, data, ctx);
        y = y.saturating_add(height);
    }
}

fn draw_section(
    frame: &mut Frame,
    area: Rect,
    sec: Section,
    state: &GlobalState,
    data: &GlobalData,
    ctx: Ctx,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match sec {
        Section::Quota => draw_quota(frame, area, data, ctx),
        Section::Terminals => draw_terminals(frame, area, state, data, ctx),
        Section::Inbox => draw_inbox(frame, area, state, data, ctx),
        Section::Messages => draw_messages(frame, area, state, data, ctx),
        Section::Usage | Section::Budgets | Section::System => {
            draw_bridged(frame, area, sec, state, data, ctx)
        }
    }
}

fn pad_cells(text: &str, width: usize) -> String {
    let mut out = text.to_string();
    let used = text.width();
    if used < width {
        out.push_str(&" ".repeat(width - used));
    }
    out
}

fn window_label(minutes: u32) -> String {
    if minutes == 43_200 {
        "mo".to_string()
    } else if minutes.is_multiple_of(1_440) {
        format!("{}d", minutes / 1_440)
    } else {
        format!("{}h", minutes / 60)
    }
}

/// ≤ 50 % left is warn, < 20 % is error; more than half left is healthy.
fn left_color(left: f64, theme: &Theme) -> Color {
    if !left.is_finite() || left < 20.0 {
        theme.error
    } else if left <= 50.0 {
        theme.warn
    } else {
        theme.ok
    }
}

fn draw_quota(frame: &mut Frame, area: Rect, data: &GlobalData, ctx: Ctx) {
    let block = panel(
        format!(" {} ", section_title(Section::Quota)),
        ctx.theme.border,
        ctx,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(snap) = &data.snap else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    if snap.quota.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    for q in &snap.quota {
        lines.push(quota_line(q, snap.now_s, ctx));
        if lines.len() as u16 >= inner.height {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn quota_line(q: &QuotaRow, now_s: i64, ctx: Ctx) -> Line<'static> {
    let provider = pad_cells(&sanitize_one_line(&q.provider), 11);
    let color = left_color(q.left_percent, ctx.theme);
    let (full, empty) = if ctx.ascii {
        ('#', '-')
    } else {
        ('█', '░')
    };
    let filled = if q.left_percent.is_finite() {
        (q.left_percent.clamp(0.0, 100.0) / 10.0).round() as usize
    } else {
        0
    };
    let pct = if q.left_percent.is_finite() {
        format!("{:>3.0}% left", q.left_percent)
    } else {
        "   ?".to_string()
    };
    let mut spans = vec![
        Span::styled(format!("{provider} "), Style::new().fg(ctx.theme.title)),
        Span::styled(
            format!("{:>3} ", window_label(q.window_minutes)),
            Style::new().fg(ctx.theme.working),
        ),
        Span::styled(full.to_string().repeat(filled), Style::new().fg(color)),
        Span::styled(
            empty.to_string().repeat(10 - filled),
            Style::new().fg(ctx.theme.muted),
        ),
        Span::styled(format!(" {pct}"), Style::new().fg(color)),
    ];
    if q.limited {
        spans.push(Span::styled(
            " limited".to_string(),
            Style::new().fg(ctx.theme.error),
        ));
    }
    if q.stale {
        spans.push(Span::styled(
            format!(
                " stale · seen {} ago",
                crate::dashboard::format_age_s(q.observed_age_s)
            ),
            Style::new().fg(ctx.theme.warn),
        ));
    } else if q.reset_passed {
        spans.push(Span::styled(
            " reset ready".to_string(),
            Style::new().fg(ctx.theme.ok),
        ));
    } else {
        let left = q.resets_at_s.saturating_sub(now_s);
        spans.push(Span::styled(
            format!(" resets in {}h{:02}m", left / 3_600, left % 3_600 / 60),
            Style::new().fg(ctx.theme.working),
        ));
    }
    Line::from(spans)
}

/// Widest content of a column, floored and capped so one long value cannot squeeze the others.
fn content_width(values: impl Iterator<Item = usize>, min: usize, cap: usize) -> u16 {
    values.max().unwrap_or(0).clamp(min, cap) as u16
}

/// Cells a table needs: its columns, the highlight symbol column and one space between columns.
fn columns_width(widths: &[u16], highlight: usize) -> usize {
    widths.iter().map(|w| *w as usize).sum::<usize>() + highlight + widths.len()
}

fn draw_terminals(frame: &mut Frame, area: Rect, state: &GlobalState, data: &GlobalData, ctx: Ctx) {
    let block = panel(
        format!(" {} ", section_title(Section::Terminals)),
        ctx.theme.border,
        ctx,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(snap) = &data.snap else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    if snap.terminals.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }

    let width = inner.width;
    let terms = &snap.terminals;
    // Size columns from content: a long name must not squeeze app/model out, and each column has
    // a cap so one hostile value cannot eat the row.
    let name_w = content_width(terms.iter().map(|t| display_name(t).width()), 6, 20);
    let app_w = content_width(terms.iter().map(|t| dash_value(&t.app).width()), 3, 12);
    let model_w = content_width(terms.iter().map(|t| model_label(t).width()), 5, 22);
    let status_w = content_width(
        terms.iter().map(|t| status_label(&t.status, ctx).0.width()),
        6,
        14,
    );
    let tokens_w = content_width(terms.iter().map(|t| tokens_label(t.tokens).width()), 6, 6);
    let last_w = content_width(terms.iter().map(|t| last_msg_label(t, ctx).width()), 5, 10);
    let place_w = content_width(
        terms
            .iter()
            .map(|t| dash_value(&where_label(&t.place, &t.cwd)).width()),
        6,
        14,
    );

    const HIGHLIGHT_W: usize = 2; // `▸ ` / `> `
    const TASK_MIN: usize = 6;
    let mut columns: Vec<(&'static str, u16)> = vec![("", 1), ("name", name_w)];
    let optional = [
        ("app", app_w),
        ("model", model_w),
        ("status", status_w),
        ("ctx", 9),
        ("tokens", tokens_w),
        ("last", last_w),
        ("place", place_w),
    ];
    for (header, col_width) in optional {
        let mut trial: Vec<u16> = columns.iter().map(|(_, w)| *w).collect();
        trial.push(col_width);
        // Keep the flexible task column at least `TASK_MIN` cells wide.
        if columns_width(&trial, HIGHLIGHT_W) + 1 + TASK_MIN <= width as usize {
            columns.push((header, col_width));
        }
    }
    columns.push(("task", 0));

    let show_app = columns.iter().any(|(h, _)| *h == "app");
    let show_model = columns.iter().any(|(h, _)| *h == "model");
    let show_status = columns.iter().any(|(h, _)| *h == "status");
    let show_ctx = columns.iter().any(|(h, _)| *h == "ctx");
    let show_tokens = columns.iter().any(|(h, _)| *h == "tokens");
    let show_last = columns.iter().any(|(h, _)| *h == "last");
    let show_place = columns.iter().any(|(h, _)| *h == "place");

    let mut rows: Vec<Row<'static>> = Vec::new();
    for t in &snap.terminals {
        let mut cells = vec![status_cell(t, ctx), Cell::from(display_name(t))];
        if show_app {
            cells.push(Cell::from(dash_value(&t.app)));
        }
        if show_model {
            cells.push(Cell::from(model_label(t)));
        }
        if show_status {
            let (text, color) = status_label(&t.status, ctx);
            cells.push(Cell::from(Span::styled(text, Style::new().fg(color))));
        }
        if show_ctx {
            cells.push(context_cell(t.context_pct, ctx));
        }
        if show_tokens {
            cells.push(Cell::from(align_right(&tokens_label(t.tokens))));
        }
        if show_last {
            cells.push(Cell::from(last_msg_label(t, ctx)));
        }
        if show_place {
            cells.push(Cell::from(dash_value(&where_label(&t.place, &t.cwd))));
        }
        // `task` is the last column (it takes the remaining width), same order as `columns`.
        cells.push(Cell::from(dash_value(&t.task)));
        rows.push(Row::new(cells));
    }

    let header = Row::new(columns.iter().map(|(h, _)| Cell::from(*h)))
        .style(Style::new().fg(ctx.theme.muted));
    let constraints: Vec<Constraint> = columns
        .iter()
        .map(|(header, col_width)| {
            if *header == "task" {
                Constraint::Min(TASK_MIN as u16)
            } else {
                Constraint::Length(*col_width)
            }
        })
        .collect();
    let selected = state
        .sel_terminal
        .as_deref()
        .and_then(|address| snap.terminals.iter().position(|t| t.address == address));
    let mut table_state = TableState::default().with_selected(selected);
    let table = Table::new(rows, constraints)
        .header(header)
        .row_highlight_style(
            Style::new()
                .fg(ctx.theme.selected)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(if ctx.ascii { "> " } else { "▸ " })
        .highlight_spacing(HighlightSpacing::Always)
        .column_spacing(1);
    frame.render_stateful_widget(table, inner, &mut table_state);
}

fn display_name(t: &TermView) -> String {
    let name = sanitize_one_line(&t.name);
    if name.is_empty() {
        sanitize_one_line(&t.address)
    } else {
        name
    }
}

fn dash_value(value: &str) -> String {
    let clean = sanitize_one_line(value);
    if clean.trim().is_empty() {
        "-".to_string()
    } else {
        clean
    }
}

fn model_label(t: &TermView) -> String {
    let model = sanitize_one_line(&t.model);
    let effort = sanitize_one_line(&t.effort);
    match (model.is_empty(), effort.is_empty()) {
        (true, true) => "-".to_string(),
        (true, false) => effort,
        (false, true) => model,
        (false, false) => format!("{model}/{effort}"),
    }
}

fn tokens_label(tokens: Option<u64>) -> String {
    match tokens {
        Some(n) if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        Some(n) if n >= 1_000 => format!("{:.0}k", n as f64 / 1e3),
        Some(n) => n.to_string(),
        None => "-".to_string(),
    }
}

fn align_right(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::new()))
        .alignment(ratatui::layout::Alignment::Right)
}

fn last_msg_label(t: &TermView, ctx: Ctx) -> String {
    match &t.last_msg {
        Some((kind, sent)) => {
            let arrow = match (ctx.ascii, sent) {
                (true, true) => "->",
                (true, false) => "<-",
                (false, true) => "→",
                (false, false) => "←",
            };
            format!("{arrow} {}", sanitize_one_line(kind))
        }
        None => "-".to_string(),
    }
}

fn context_cell(pct: Option<f64>, ctx: Ctx) -> Cell<'static> {
    let Some(pct) = pct.filter(|p| p.is_finite()) else {
        return Cell::from("-").style(Style::new().fg(ctx.theme.muted));
    };
    let pct = pct.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * 4.0).round() as usize;
    let (full, empty) = if ctx.ascii {
        ('#', '-')
    } else {
        ('█', '░')
    };
    let color = if pct >= 85.0 {
        ctx.theme.error
    } else if pct >= 60.0 {
        ctx.theme.warn
    } else {
        ctx.theme.ok
    };
    Cell::from(Line::from(vec![
        Span::styled(full.to_string().repeat(filled), Style::new().fg(color)),
        Span::styled(
            empty.to_string().repeat(4 - filled),
            Style::new().fg(ctx.theme.muted),
        ),
        Span::styled(format!(" {:>3.0}%", pct), Style::new().fg(color)),
    ]))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Working,
    Blocked,
    Done,
    Error,
    Unknown,
}

fn status_kind(status: &str) -> StatusKind {
    let status = status.to_ascii_lowercase();
    if status.starts_with("blocked") {
        StatusKind::Blocked
    } else if status.contains("working") || status.contains("running") {
        StatusKind::Working
    } else if status.contains("done") || status.contains("idle") || status.contains("active") {
        StatusKind::Done
    } else if status.contains("error") || status.contains("fail") {
        StatusKind::Error
    } else {
        StatusKind::Unknown
    }
}

fn status_symbol(kind: StatusKind, ascii: bool) -> &'static str {
    match (kind, ascii) {
        (StatusKind::Working, false) => "●",
        (StatusKind::Blocked, false) => "◐",
        (StatusKind::Done, false) => "✓",
        (StatusKind::Error, false) => "✗",
        (StatusKind::Unknown, false) => "-",
        (StatusKind::Working, true) => "*",
        (StatusKind::Blocked, true) => "~",
        (StatusKind::Done, true) => "v",
        (StatusKind::Error, true) => "x",
        (StatusKind::Unknown, true) => "-",
    }
}

fn status_color(kind: StatusKind, theme: &Theme) -> Color {
    match kind {
        StatusKind::Working => theme.working,
        StatusKind::Blocked => theme.warn,
        StatusKind::Done => theme.ok,
        StatusKind::Error => theme.error,
        StatusKind::Unknown => theme.muted,
    }
}

fn status_cell(t: &TermView, ctx: Ctx) -> Cell<'static> {
    let kind = status_kind(&t.status);
    Cell::from(Span::styled(
        status_symbol(kind, ctx.ascii),
        Style::new().fg(status_color(kind, ctx.theme)),
    ))
}

fn status_label(status: &str, ctx: Ctx) -> (String, Color) {
    let kind = status_kind(status);
    let clean = sanitize_one_line(status);
    if clean.trim().is_empty() {
        ("-".to_string(), ctx.theme.muted)
    } else {
        (clean, status_color(kind, ctx.theme))
    }
}

fn draw_inbox(frame: &mut Frame, area: Rect, state: &GlobalState, data: &GlobalData, ctx: Ctx) {
    let count = data.snap.as_ref().map(|snap| snap.inbox.len()).unwrap_or(0);
    let block = panel(
        format!(" {} ({count}) ", section_title(Section::Inbox)),
        ctx.theme.border,
        ctx,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(snap) = &data.snap else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    if snap.inbox.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }

    let width = inner.width;
    let arrow = if ctx.ascii { "->" } else { "→" };
    let mut constraints = vec![Constraint::Length(6), Constraint::Length(12)];
    let mut headers = vec!["#", "kind"];
    let show_terminal = width >= 52;
    let show_age = width >= 64;
    if show_terminal {
        constraints.push(Constraint::Length(12));
        headers.push("terminal");
    }
    if show_age {
        constraints.push(Constraint::Length(6));
        headers.push("age");
    }
    constraints.push(Constraint::Min(10));
    headers.push("text");

    let mut rows: Vec<Row<'static>> = Vec::new();
    for item in &snap.inbox {
        let mut cells = vec![
            Cell::from(format!("#{}", item.id)),
            Cell::from(
                item.kind
                    .as_deref()
                    .map(kind_to_english)
                    .map(sanitize_one_line)
                    .filter(|k| !k.is_empty())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ];
        if show_terminal {
            cells.push(Cell::from(dash_value(
                item.terminal.as_deref().unwrap_or("-"),
            )));
        }
        if show_age {
            cells.push(Cell::from(age_label(
                ctx.now_ms.saturating_sub(item.ts_ms) / 1_000,
            )));
        }
        cells.push(Cell::from(format!(
            "{arrow} {}",
            sanitize_one_line(&item.text)
        )));
        rows.push(Row::new(cells));
    }

    let header =
        Row::new(headers.into_iter().map(Cell::from)).style(Style::new().fg(ctx.theme.muted));
    let selected = state
        .sel_inbox
        .and_then(|id| snap.inbox.iter().position(|item| item.id == id));
    let mut table_state = TableState::default().with_selected(selected);
    let table = Table::new(rows, constraints)
        .header(header)
        .row_highlight_style(
            Style::new()
                .fg(ctx.theme.selected)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(if ctx.ascii { "> " } else { "▸ " })
        .highlight_spacing(HighlightSpacing::Always)
        .column_spacing(1);
    frame.render_stateful_widget(table, inner, &mut table_state);
}

fn party_labels(snap: &Snapshot) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    for t in &snap.terminals {
        let display = display_name(t);
        labels.insert(t.address.clone(), display.clone());
        let name = sanitize_one_line(&t.name);
        if !name.is_empty() {
            labels.insert(name, display);
        }
    }
    labels
}

fn draw_messages(frame: &mut Frame, area: Rect, state: &GlobalState, data: &GlobalData, ctx: Ctx) {
    let block = panel(
        format!(" {} ", section_title(Section::Messages)),
        ctx.theme.border,
        ctx,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(snap) = &data.snap else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    if snap.messages.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }

    let labels = party_labels(snap);
    let arrow = if ctx.ascii { "->" } else { "→" };
    let party = |address: &str| {
        labels
            .get(address)
            .cloned()
            .unwrap_or_else(|| sanitize_one_line(address))
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    // Wide panels have room for one line per message (`from → to KIND age  text`), so the pinned
    // six rows show more of the conversation; narrow ones keep the header/text split.
    let one_line = inner.width >= 100;
    for message in &snap.messages {
        let age = age_label(ctx.now_ms.saturating_sub(message.ts_ms) / 1_000);
        let header = Line::from(vec![
            Span::styled(
                format!("{} {arrow} {} ", party(&message.from), party(&message.to)),
                Style::new().fg(ctx.theme.title),
            ),
            Span::styled(
                format!(
                    "{} {age}",
                    sanitize_one_line(kind_to_english(&message.kind))
                ),
                Style::new().fg(ctx.theme.accent),
            ),
        ]);
        if one_line {
            let mut line = header;
            line.spans.push(Span::styled(
                format!("  {}", sanitize_one_line(&message.text)),
                Style::new(),
            ));
            lines.push(line);
        } else {
            lines.push(header);
            lines.push(Line::from(Span::styled(
                sanitize_one_line(&message.text),
                Style::new(),
            )));
        }
    }
    let scroll = state.scroll.get(&Section::Messages).copied().unwrap_or(0);
    let paragraph = Paragraph::new(lines).scroll((scroll, 0));
    let paragraph = if one_line {
        paragraph
    } else {
        paragraph.wrap(Wrap { trim: true })
    };
    frame.render_widget(paragraph, inner);
}

fn draw_bridged(
    frame: &mut Frame,
    area: Rect,
    sec: Section,
    state: &GlobalState,
    data: &GlobalData,
    ctx: Ctx,
) {
    let block = panel(format!(" {} ", section_title(sec)), ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(snap) = &data.snap else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    let body = render_section_body(snap, sec, inner.width as usize);
    let text: Text<'static> = ansi_to_text(&body);
    let scroll = state.scroll.get(&sec).copied().unwrap_or(0);
    frame.render_widget(Paragraph::new(text).scroll((scroll, 0)), inner);
}

/// `Outcome::Shown` text over the lower half of the section area, above Messages.
fn draw_output(frame: &mut Frame, area: Rect, text: &str, scroll: u16, ctx: Ctx) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let height = (area.height / 2).max(3).min(area.height);
    let rect = Rect {
        x: area.x,
        y: area.y + area.height - height,
        width: area.width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = panel(" Output ".to_string(), ctx.theme.accent, ctx);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let text: Text<'static> = ansi_to_text(text);
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inner,
    );
}

/// Command bar; returns the cursor position when the command line has focus.
fn draw_cmdline(frame: &mut Frame, area: Rect, state: &GlobalState, ctx: Ctx) -> Option<Position> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let prompt_w = PROMPT.width();
    let available = (area.width as usize).saturating_sub(prompt_w);
    // No room for even one cell of input: draw nothing there instead of computing a negative
    // cursor column (`cursor_cells - scroll` used to underflow at 5 columns and fewer).
    if available == 0 {
        return None;
    }
    let text = sanitize_one_line(&state.cmd.text);
    let cursor_chars = state.cmd.cursor.min(text.chars().count());
    let cursor_cells = cells_before(&text, cursor_chars);
    // Scroll only when the cursor would leave the field; `available > 0` is guaranteed above.
    let scroll = if cursor_cells >= available {
        cursor_cells - available + 1
    } else {
        0
    };
    let visible = visible_slice(&text, scroll, available);
    let line = Line::from(vec![
        Span::styled(
            PROMPT,
            Style::new()
                .fg(ctx.theme.title)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(visible, Style::new()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
    if !matches!(state.focus, Focus::CmdLine) {
        return None;
    }
    let x = area
        .x
        .saturating_add(prompt_w as u16)
        .saturating_add((cursor_cells.saturating_sub(scroll)).min(u16::MAX as usize) as u16)
        .min(area.right().saturating_sub(1));
    Some(Position::new(x, area.y))
}

fn cells_before(text: &str, chars: usize) -> usize {
    text.chars()
        .take(chars)
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

/// Slice by display cells so a horizontally scrolled command line never splits a wide char.
fn visible_slice(text: &str, skip: usize, max: usize) -> String {
    let mut out = String::new();
    let mut position = 0usize;
    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if position < skip {
            position += width;
            continue;
        }
        if position - skip + width > max {
            break;
        }
        out.push(ch);
        position += width;
    }
    out
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &GlobalState, ctx: Ctx) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (text, style) = match &state.note {
        Some((kind, text)) => (
            sanitize_one_line(text),
            Style::new().fg(match kind {
                NoteKind::Ok => ctx.theme.ok,
                NoteKind::Warn => ctx.theme.warn,
                NoteKind::Err => ctx.theme.error,
            }),
        ),
        None => (
            if ctx.ascii { HELP_ASCII } else { HELP }.to_string(),
            Style::new().fg(ctx.theme.muted),
        ),
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}

fn draw_popup(frame: &mut Frame, area: Rect, state: &GlobalState, ctx: Ctx) -> Option<Position> {
    let Popup::Confirm { prompt, input } = state.popup.as_ref()?;
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let width = 60.min(area.width);
    let height = 8.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = panel(" Confirm ".to_string(), ctx.theme.warn, ctx);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    let hint = if ctx.ascii {
        "y then enter confirms · anything else cancels"
    } else {
        "y then ⏎ confirms · anything else cancels"
    };
    let lines = vec![
        Line::from(Span::styled(
            sanitize_one_line(prompt),
            Style::new().fg(ctx.theme.warn),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::new().fg(ctx.theme.muted)),
            Span::styled(sanitize_one_line(input), Style::new()),
        ]),
        Line::from(""),
        muted_line(hint, ctx),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    let input_width = sanitize_one_line(input).width() as u16;
    let x = inner
        .x
        .saturating_add(2)
        .saturating_add(input_width)
        .min(inner.right().saturating_sub(1));
    Some(Position::new(x, inner.y.saturating_add(2)))
}
