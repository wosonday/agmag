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
use super::global::{
    BorderId, CmdLine, Focus, GlobalData, GlobalState, Popup, RowTarget, is_plain_shell,
    ordered_terminals, visible_sections,
};
use super::theme::Theme;
use crate::actions::kind_to_english;
use crate::dashboard::{Section, Snapshot, TermView, render_section_body, sanitize_one_line};
use crate::project_render::age_label;
use crate::report::QuotaRow;

/// Messages is the flexible section: it absorbs the space no other section needs.
const MESSAGES_MIN: u16 = 4;
/// A panel is its border plus at least one usable row.
const MIN_SECTION_H: u16 = 3;
/// The top pair may never take more than this share of the sections area.
const PAIR_CAP_PERCENT: u32 = 60;
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
    state.clear_hit_map();
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
    let (sections, messages, layout) = split_messages(main, data);
    draw_sections(frame, sections, &layout, state, data, ctx);
    draw_messages(frame, messages, state, data, ctx);
    if messages.height > 0 {
        state.push_panel(messages, Section::Messages);
        state.push_border(
            Rect {
                x: messages.x,
                y: messages.y,
                width: messages.width,
                height: 1,
            },
            BorderId::Horizontal(visible_sections(&data.view).len().saturating_sub(1)),
        );
    }
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

/// Fitted heights for the section groups of one frame. The top pair is one group (two panels
/// side by side share a height); the remaining groups are full-width sections; Messages is pinned
/// last and is the flexible group.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LayoutHeights {
    pair: Option<u16>,
    stacked: Vec<u16>,
    messages: u16,
}

/// Rows a section needs for its content, borders included.
fn section_desired_rows(sec: Section, data: &GlobalData, width: u16) -> u16 {
    if sec == Section::Messages {
        return MESSAGES_MIN;
    }
    let content_rows = match sec {
        Section::Quota => data
            .snap
            .as_ref()
            .map(|snap| snap.quota.len().max(1))
            .unwrap_or(1),
        Section::Terminals => data
            .snap
            .as_ref()
            .map(|snap| snap.terminals.len().max(1) + 1) // header + rows (or one `-`)
            .unwrap_or(1),
        Section::Inbox => data
            .snap
            .as_ref()
            .map(|snap| {
                if snap.inbox.is_empty() {
                    1
                } else {
                    snap.inbox.len() + 1 // header + rows
                }
            })
            .unwrap_or(1),
        Section::Usage | Section::Budgets | Section::System => data
            .snap
            .as_ref()
            .map(|snap| {
                let inner = width.saturating_sub(2).max(1) as usize;
                render_section_body(snap, sec, inner).lines().count().max(1)
            })
            .unwrap_or(1),
        Section::Messages => unreachable!(),
    };
    (content_rows as u16)
        .saturating_add(2)
        .clamp(MIN_SECTION_H, u16::MAX - 2)
}

/// Fit every section group into `main` at once: Messages gets what nobody needs.
fn section_layout(main: Rect, data: &GlobalData) -> LayoutHeights {
    let mut chosen = visible_sections(&data.view);
    chosen.pop(); // Messages is drawn separately; it stays the last group below.
    let wide = main.width >= 120 && chosen.len() >= 2;
    let mut desired: Vec<u16> = Vec::with_capacity(chosen.len());
    let mut stacked: Vec<Section> = Vec::with_capacity(chosen.len());

    if wide {
        // The cap is a share of what is left for the fixed sections after Messages; it must
        // never be inflated to fit two panels. The floor is one panel's minimum, not two.
        let cap = (u32::from(main.height.saturating_sub(MESSAGES_MIN))
            .saturating_mul(PAIR_CAP_PERCENT)
            / 100) as u16;
        let pair = section_desired_rows(chosen[0], data, main.width)
            .max(section_desired_rows(chosen[1], data, main.width))
            .min(cap)
            .max(MIN_SECTION_H);
        desired.push(pair);
        stacked.extend_from_slice(&chosen[2..]);
    } else {
        stacked.extend_from_slice(&chosen);
    }
    for sec in &stacked {
        desired.push(section_desired_rows(*sec, data, main.width));
    }
    desired.push(MESSAGES_MIN.min(main.height.max(1)));

    let heights = fit_heights(&desired, desired.len() - 1, main.height, MIN_SECTION_H);
    let offset = usize::from(wide);
    LayoutHeights {
        pair: wide.then(|| heights[0]),
        stacked: heights[offset..heights.len() - 1].to_vec(),
        messages: *heights.last().unwrap_or(&0),
    }
}

/// Sections get everything except the height Messages needs, which is fitted with the rest.
fn split_messages(main: Rect, data: &GlobalData) -> (Rect, Rect, LayoutHeights) {
    let mut layout = section_layout(main, data);
    // `fit_heights` keeps its one-panel floor even for a window shorter than the floor. The
    // floor may win over the requested total, but no rect may spill into the command bar/footer.
    layout.messages = layout.messages.min(main.height);
    let sections_h = main.height.saturating_sub(layout.messages);
    let sections = Rect {
        x: main.x,
        y: main.y,
        width: main.width,
        height: sections_h,
    };
    let messages = Rect {
        x: main.x,
        y: main.y + sections_h,
        width: main.width,
        height: layout.messages,
    };
    (sections, messages, layout)
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

/// Fit the desired row counts into `total`.
///
/// Surplus goes to the single flexible section (`flex`); a deficit is taken from the largest
/// section first so small panels keep their content, and no section is ever shrunk below `min`.
/// The result sums to `total` whenever `total >= desired.len() * min` (or is empty for no input).
pub fn fit_heights(desired: &[u16], flex: usize, total: u16, min: u16) -> Vec<u16> {
    if desired.is_empty() {
        return Vec::new();
    }
    let mut heights: Vec<u16> = desired.iter().map(|&d| d.max(min)).collect();
    let sum: u32 = heights.iter().map(|&h| h as u32).sum();
    let total = total as u32;

    if sum <= total {
        // Everyone got what they wanted: the leftover belongs to the flexible section.
        if let Some(slot) = heights.get_mut(flex) {
            let room = u32::from(u16::MAX) - u32::from(*slot);
            *slot += (total - sum).min(room) as u16;
        }
        return heights;
    }

    let mut deficit = sum - total;
    while deficit > 0 {
        // Lowest index wins ties, so the result is deterministic.
        let mut largest = None;
        for (i, &h) in heights.iter().enumerate() {
            if h > min && largest.is_none_or(|(j, _)| h > heights[j]) {
                largest = Some((i, h));
            }
        }
        let Some((i, _)) = largest else {
            break; // everything is at `min`; that floor wins over the requested total
        };
        heights[i] -= 1;
        deficit -= 1;
    }
    heights
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

fn draw_sections(
    frame: &mut Frame,
    area: Rect,
    layout: &LayoutHeights,
    state: &GlobalState,
    data: &GlobalData,
    ctx: Ctx,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut chosen = visible_sections(&data.view);
    // `Messages` is pinned and drawn separately below the section area.
    chosen.pop();
    if let Some(pair_h) = layout.pair {
        // ≥ 120 columns: first two sections side by side, the rest stacked under them.
        let pair = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: pair_h.min(area.height),
        };
        let constraints = if chosen[0] == Section::Quota {
            [
                Constraint::Length(quota_panel_width(data, ctx, pair.width)),
                Constraint::Min(0),
            ]
        } else if chosen[1] == Section::Quota {
            [
                Constraint::Min(0),
                Constraint::Length(quota_panel_width(data, ctx, pair.width)),
            ]
        } else {
            [Constraint::Percentage(50), Constraint::Percentage(50)]
        };
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(pair);
        for (col, sec) in cols.iter().zip(chosen.iter().take(2)) {
            state.push_panel(*col, *sec);
        }
        // Both panels' border columns are draggable, not just the gap between them. Clamp the
        // two-cell strip so it never reaches past the right edge of the content area.
        let divider_x = cols[0]
            .right()
            .saturating_sub(1)
            .min(pair.right().saturating_sub(1));
        let divider_w = 2.min(pair.right().saturating_sub(divider_x));
        state.push_border(
            Rect {
                x: divider_x,
                y: pair.y,
                width: divider_w,
                height: pair.height,
            },
            BorderId::TopPairVertical,
        );
        draw_section(frame, cols[0], chosen[0], state, data, ctx);
        draw_section(frame, cols[1], chosen[1], state, data, ctx);
        if chosen.len() > 2 {
            let rest = Rect {
                x: area.x,
                y: area.y + pair.height,
                width: area.width,
                height: area.height.saturating_sub(pair.height),
            };
            draw_stacked(
                frame,
                rest,
                &chosen[2..],
                2,
                &layout.stacked,
                state,
                data,
                ctx,
            );
        }
    } else {
        draw_stacked(frame, area, &chosen, 0, &layout.stacked, state, data, ctx);
    }
}

/// One section per fitted height. The heights were computed for the whole frame, so rounding is
/// absorbed by the last panel instead of leaving gaps between sections.
#[allow(clippy::too_many_arguments)]
fn draw_stacked(
    frame: &mut Frame,
    area: Rect,
    sections: &[Section],
    base_index: usize,
    heights: &[u16],
    state: &GlobalState,
    data: &GlobalData,
    ctx: Ctx,
) {
    if sections.is_empty() || area.height == 0 {
        return;
    }
    let mut heights = heights.to_vec();
    if heights.len() != sections.len() {
        heights = vec![0; sections.len()];
    }
    let sum: u16 = heights.iter().copied().fold(0, u16::saturating_add);
    if sum < area.height {
        if let Some(last) = heights.last_mut() {
            *last = last.saturating_add(area.height - sum);
        }
    } else if sum > area.height {
        // A section that cannot get its minimum is dropped entirely, never squeezed to a
        // one-row sliver that overlaps its neighbour or the Messages panel below.
        let mut excess = sum - area.height;
        for height in heights.iter_mut().rev() {
            let take = (*height).min(excess);
            *height -= take;
            excess -= take;
            if excess == 0 {
                break;
            }
        }
    }
    let mut y = area.y;
    for (i, (sec, height)) in sections.iter().zip(&heights).enumerate() {
        if *height == 0 {
            continue; // dropped: neither drawn nor recorded in the hit map
        }
        let rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height: *height,
        };
        state.push_panel(rect, *sec);
        if i > 0 || base_index > 0 {
            state.push_border(
                Rect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: 1,
                },
                BorderId::Horizontal(base_index + i),
            );
        }
        draw_section(frame, rect, *sec, state, data, ctx);
        y = y.saturating_add(*height);
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

fn quota_panel_width(data: &GlobalData, ctx: Ctx, max_width: u16) -> u16 {
    let title_w = " Quota ".width() + 2;
    let row_w = data
        .snap
        .as_ref()
        .map(|snap| {
            snap.quota
                .iter()
                .map(|q| quota_line(q, snap.now_s, ctx).width() + 2)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    (row_w.max(title_w) as u16).min(max_width)
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

fn terminals_title(agents: usize, shells: usize) -> String {
    let noun = |n: usize, one: &'static str, many: &'static str| if n == 1 { one } else { many };
    match (agents, shells) {
        (0, 0) => " Terminals ".to_string(),
        (a, 0) => format!(" Terminals ({a} {}) ", noun(a, "agent", "agents")),
        (0, s) => format!(" Terminals ({s} {}) ", noun(s, "shell", "shells")),
        (a, s) => format!(
            " Terminals ({a} {} · {s} {}) ",
            noun(a, "agent", "agents"),
            noun(s, "shell", "shells")
        ),
    }
}

/// Rect of each row a `Table` actually shows, following ratatui's offset for the selected row.
/// Index 0 is the data row at the top of the inner area (the header occupies the first line).
fn visible_table_rows(
    inner: Rect,
    row_count: usize,
    selected: Option<usize>,
) -> Vec<(usize, Rect)> {
    let view = inner.height.saturating_sub(1) as usize;
    if view == 0 || row_count == 0 {
        return Vec::new();
    }
    let offset = selected
        .map(|sel| sel.saturating_add(1).saturating_sub(view))
        .unwrap_or(0)
        .min(row_count.saturating_sub(view));
    (offset..row_count.min(offset.saturating_add(view)))
        .map(|i| {
            (
                i,
                Rect {
                    x: inner.x,
                    y: inner.y + 1 + (i - offset) as u16,
                    width: inner.width,
                    height: 1,
                },
            )
        })
        .collect()
}

fn draw_terminals(frame: &mut Frame, area: Rect, state: &GlobalState, data: &GlobalData, ctx: Ctx) {
    // One ordering for the rows, the title counts and the ↑↓ navigation (in `global.rs`).
    let ordered = data
        .snap
        .as_ref()
        .map(ordered_terminals)
        .unwrap_or_default();
    let shells = ordered.iter().filter(|t| is_plain_shell(t)).count();
    let agents = ordered.len() - shells;
    let block = panel(terminals_title(agents, shells), ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if ordered.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }

    let width = inner.width;
    let terms = &ordered;
    let values = |header: &'static str| -> Vec<String> {
        match header {
            "app" => terms.iter().map(|t| dash_value(&t.app)).collect(),
            "model" => terms.iter().map(|t| model_label(t)).collect(),
            "st" => terms
                .iter()
                .map(|t| status_label(&t.status, ctx).0)
                .collect(),
            "ctx" => terms
                .iter()
                .map(|t| {
                    if t.context_pct.is_some_and(|p| p.is_finite()) {
                        "ctx".to_string()
                    } else {
                        "-".to_string()
                    }
                })
                .collect(),
            "tok" => terms.iter().map(|t| tokens_label(t.tokens)).collect(),
            "last" => terms.iter().map(|t| last_msg_label(t, ctx)).collect(),
            "task" => terms.iter().map(|t| dash_value(&t.task)).collect(),
            _ => Vec::new(),
        }
    };
    let has_data = |header: &'static str| values(header).iter().any(|v| v != "-");

    // Size name/model from their content first (with generous caps); the `task` column, when it
    // exists, shows all dashes and is dropped, otherwise it takes the leftover width.
    let name_w = content_width(terms.iter().map(|t| display_name(t).width()), 4, 32);
    let app_w = content_width(terms.iter().map(|t| dash_value(&t.app).width()), 3, 12);
    let model_w = content_width(terms.iter().map(|t| model_label(t).width()), 5, 32);
    let status_w = content_width(
        terms.iter().map(|t| status_label(&t.status, ctx).0.width()),
        2,
        14,
    );
    let tokens_w = content_width(terms.iter().map(|t| tokens_label(t.tokens).width()), 3, 6);
    let last_w = content_width(terms.iter().map(|t| last_msg_label(t, ctx).width()), 4, 10);

    const HIGHLIGHT_W: usize = 2; // `▸ ` / `> `
    const TASK_MIN: usize = 6;
    let show_task = has_data("task");
    let reserve = if show_task { 1 + TASK_MIN } else { 0 };
    let mut columns: Vec<(&'static str, u16)> = vec![("", 1), ("name", name_w)];
    let optional = [
        ("app", app_w),
        ("model", model_w),
        ("st", status_w),
        ("ctx", 9),
        ("tok", tokens_w),
        ("last", last_w),
    ];
    for (header, col_width) in optional {
        if !has_data(header) {
            continue; // a column that is `-` for every row only wastes cells
        }
        let mut trial: Vec<u16> = columns.iter().map(|(_, w)| *w).collect();
        trial.push(col_width);
        if columns_width(&trial, HIGHLIGHT_W) + reserve <= width as usize {
            columns.push((header, col_width));
        }
    }
    if show_task {
        columns.push(("task", 0));
    }

    let show_app = columns.iter().any(|(h, _)| *h == "app");
    let show_model = columns.iter().any(|(h, _)| *h == "model");
    let show_status = columns.iter().any(|(h, _)| *h == "st");
    let show_ctx = columns.iter().any(|(h, _)| *h == "ctx");
    let show_tokens = columns.iter().any(|(h, _)| *h == "tok");
    let show_last = columns.iter().any(|(h, _)| *h == "last");

    let mut rows: Vec<Row<'static>> = Vec::new();
    for t in &ordered {
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
        if show_task {
            cells.push(Cell::from(dash_value(&t.task)));
        }
        // Plain shells sit last and are dimmed; the highlight style still wins for the selection.
        let row_style = if is_plain_shell(t) {
            Style::new().fg(ctx.theme.muted)
        } else {
            Style::new()
        };
        rows.push(Row::new(cells).style(row_style));
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
        .and_then(|address| ordered.iter().position(|t| t.address == address));
    for (i, rect) in visible_table_rows(inner, ordered.len(), selected) {
        state.push_row(rect, RowTarget::Terminal(ordered[i].address.clone()));
    }
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
    for (i, rect) in visible_table_rows(inner, snap.inbox.len(), selected) {
        state.push_row(rect, RowTarget::Inbox(snap.inbox[i].id));
    }
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
    // One row per message when wide, two in the narrow header/text form. Wrapped lines can add
    // more, so this is a best-effort map of where each message starts.
    let per_message = if one_line { 1usize } else { 2 };
    for (i, _) in snap.messages.iter().enumerate() {
        let Some(y_offset) = (i * per_message).checked_sub(scroll as usize) else {
            continue;
        };
        if y_offset >= inner.height as usize {
            break;
        }
        state.push_row(
            Rect {
                x: inner.x,
                y: inner.y + y_offset as u16,
                width: inner.width,
                height: (per_message as u16).min(inner.height - y_offset as u16),
            },
            RowTarget::Message(i),
        );
    }
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

/// `Outcome::Shown` text over the lower half of the content area.
pub(crate) fn draw_output_overlay(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    scroll: u16,
    theme: &Theme,
    ascii: bool,
) {
    draw_output(
        frame,
        area,
        text,
        scroll,
        Ctx {
            theme,
            ascii,
            now_ms: 0,
        },
    );
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

/// Command bar; returns the cursor position when focused.
pub(crate) fn draw_cmdline_bar(
    frame: &mut Frame,
    area: Rect,
    cmd: &CmdLine,
    focused: bool,
    theme: &Theme,
) -> Option<Position> {
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
    let text = sanitize_one_line(&cmd.text);
    let cursor_chars = cmd.cursor.min(text.chars().count());
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
            Style::new().fg(theme.title).add_modifier(Modifier::BOLD),
        ),
        Span::styled(visible, Style::new()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
    if !focused {
        return None;
    }
    let x = area
        .x
        .saturating_add(prompt_w as u16)
        .saturating_add((cursor_cells.saturating_sub(scroll)).min(u16::MAX as usize) as u16)
        .min(area.right().saturating_sub(1));
    Some(Position::new(x, area.y))
}

/// Command bar; returns the cursor position when the command line has focus.
fn draw_cmdline(frame: &mut Frame, area: Rect, state: &GlobalState, ctx: Ctx) -> Option<Position> {
    draw_cmdline_bar(
        frame,
        area,
        &state.cmd,
        matches!(state.focus, Focus::CmdLine),
        ctx.theme,
    )
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

pub(crate) fn draw_footer_bar(
    frame: &mut Frame,
    area: Rect,
    note: Option<&(NoteKind, String)>,
    help: &str,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (text, style) = match note {
        Some((kind, text)) => (
            sanitize_one_line(text),
            Style::new().fg(match kind {
                NoteKind::Ok => theme.ok,
                NoteKind::Warn => theme.warn,
                NoteKind::Err => theme.error,
            }),
        ),
        None => (help.to_string(), Style::new().fg(theme.muted)),
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &GlobalState, ctx: Ctx) {
    draw_footer_bar(
        frame,
        area,
        state.note.as_ref(),
        if ctx.ascii { HELP_ASCII } else { HELP },
        ctx.theme,
    );
}

pub(crate) fn draw_confirm_popup(
    frame: &mut Frame,
    area: Rect,
    state: &GlobalState,
    theme: &Theme,
    ascii: bool,
) -> Option<Position> {
    draw_popup(
        frame,
        area,
        state,
        Ctx {
            theme,
            ascii,
            now_ms: 0,
        },
    )
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

#[cfg(test)]
mod tests {
    use super::fit_heights;

    #[test]
    fn fit_heights_empty_input_is_empty() {
        assert!(fit_heights(&[], 0, 20, 3).is_empty());
        assert!(fit_heights(&[], 9, 0, 3).is_empty());
    }

    #[test]
    fn fit_heights_surplus_goes_to_flex() {
        assert_eq!(fit_heights(&[3, 5, 4], 1, 20, 3), vec![3, 13, 4]);
        assert_eq!(fit_heights(&[4, 4], 0, 10, 3), vec![6, 4]);
    }

    #[test]
    fn fit_heights_deficit_shrinks_largest_first() {
        let got = fit_heights(&[20, 4, 4], 0, 15, 3);
        assert_eq!(got, vec![7, 4, 4], "only the largest section shrinks");
        assert_eq!(got.iter().sum::<u16>(), 15);

        let got = fit_heights(&[10, 10, 3], 0, 15, 3);
        assert_eq!(got, vec![6, 6, 3], "ties share the deficit evenly");
        assert_eq!(got.iter().sum::<u16>(), 15);
    }

    #[test]
    fn fit_heights_never_below_min() {
        let got = fit_heights(&[20, 2, 2], 0, 4, 3);
        assert!(got.iter().all(|&h| h >= 3), "below min: {got:?}");
        assert_eq!(got, vec![3, 3, 3], "min wins when total is too small");

        let got = fit_heights(&[20, 2, 2], 0, 12, 3);
        assert_eq!(got, vec![6, 3, 3]);
    }

    #[test]
    fn fit_heights_sums_to_total() {
        for total in [12u16, 20, 38, 50] {
            let got = fit_heights(&[3, 14, 3, 4], 3, total, 3);
            assert_eq!(got.iter().sum::<u16>(), total, "total {total}: {got:?}");
            assert!(got.iter().all(|&h| h >= 3), "below min: {got:?}");
        }
        assert_eq!(fit_heights(&[3, 14, 3, 4], 3, 38, 3), vec![3, 14, 3, 18]);
    }
}
