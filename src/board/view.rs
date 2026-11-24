//! Board rendering with ratatui widgets. No I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Sparkline, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::BoardData;
use super::flows::{self, Edge};
use super::layout::{self, BlockKind};
use super::state::{BoardState, Mode, Panel};
use super::theme::Theme;
use crate::actions::{KINDS_EN, kind_to_english};
use crate::dashboard::{format_status, sanitize_one_line};
use crate::project::AgentView;
use crate::project_render::age_label;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// ≥ 120 columns and ≥ 20 rows: projects | orchestration | detail.
    Wide,
    /// 80–119 columns: orchestration | detail.
    Medium,
    /// Anything smaller: stacked single column.
    Narrow,
}

pub fn layout_mode(width: u16, height: u16) -> LayoutMode {
    if width >= 120 && height >= 20 {
        LayoutMode::Wide
    } else if width >= 80 && height >= 20 {
        LayoutMode::Medium
    } else {
        LayoutMode::Narrow
    }
}

/// Rects recorded during the last board draw, used for mouse hit testing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardHitMap {
    pub panels: Vec<(Rect, Panel)>,
    pub project_rows: Vec<(Rect, usize)>,
    pub agent_cards: Vec<(Rect, String)>,
    pub inbox_rows: Vec<(Rect, i64)>,
    pub picker_rows: Vec<(Rect, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitTarget {
    Panel(Panel),
    ProjectRow(usize),
    AgentCard(String),
    InboxRow(i64),
    PickerRow(usize),
}

/// Pure hit test: most specific target wins (rows/cards before panels).
pub fn hit(map: &BoardHitMap, x: u16, y: u16) -> Option<HitTarget> {
    for (rect, idx) in &map.project_rows {
        if contains(*rect, x, y) {
            return Some(HitTarget::ProjectRow(*idx));
        }
    }
    for (rect, addr) in &map.agent_cards {
        if contains(*rect, x, y) {
            return Some(HitTarget::AgentCard(addr.clone()));
        }
    }
    for (rect, id) in &map.inbox_rows {
        if contains(*rect, x, y) {
            return Some(HitTarget::InboxRow(*id));
        }
    }
    for (rect, idx) in &map.picker_rows {
        if contains(*rect, x, y) {
            return Some(HitTarget::PickerRow(*idx));
        }
    }
    for (rect, panel) in &map.panels {
        if contains(*rect, x, y) {
            return Some(HitTarget::Panel(*panel));
        }
    }
    None
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x.saturating_add(rect.width) && y >= rect.y && y < rect.y.saturating_add(rect.height)
}

pub fn draw(frame: &mut Frame, state: &BoardState, data: &BoardData, theme: &Theme, ascii: bool) -> BoardHitMap {
    draw_in(frame, frame.area(), state, data, theme, ascii)
}

pub fn draw_in(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    theme: &Theme,
    ascii: bool,
) -> BoardHitMap {
    let mut hit = BoardHitMap::default();
    let area = area.intersection(frame.area());
    if area.width == 0 || area.height == 0 {
        return hit;
    }
    let ctx = Ctx { theme, ascii };
    match layout_mode(area.width, area.height) {
        LayoutMode::Wide => draw_wide(frame, area, state, data, ctx, &mut hit),
        LayoutMode::Medium => draw_medium(frame, area, state, data, ctx, &mut hit),
        LayoutMode::Narrow => draw_narrow(frame, area, state, data, ctx, &mut hit),
    }
    hit
}

/// Theme + ASCII flag, passed down instead of two separate arguments.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    theme: &'a Theme,
    ascii: bool,
}

const STATUS_MIN_W: u16 = 14;
const RIGHT_W: u16 = 34;
const CARD_MIN_W: u16 = 16;
const CARD_MAX_W: u16 = 32;
const CARD_GAP: u16 = 1;
const CARD_H: u16 = 6;
/// Two connector rows sit between card rows: the bus and the drop/label line.
const CONNECTOR_ROWS: u16 = 2;
const INBOX_H: u16 = 5;
const STEPS: [&str; 4] = ["wip", "ready", "merging", "merged"];
const MAX_POPUP_H: u16 = 20;

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

fn split_frame(area: Rect) -> (Rect, Rect) {
    let main = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    let footer = Rect {
        x: area.x,
        y: area.y + main.height,
        width: area.width,
        height: area.height.min(1),
    };
    (main, footer)
}

fn split_inbox(main: Rect) -> (Rect, Rect) {
    let inbox_h = INBOX_H.min(main.height);
    let content = Rect {
        x: main.x,
        y: main.y,
        width: main.width,
        height: main.height - inbox_h,
    };
    let inbox = Rect {
        x: main.x,
        y: main.y + content.height,
        width: main.width,
        height: inbox_h,
    };
    (content, inbox)
}

fn draw_wide(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    let (main, footer) = split_frame(area);
    let (content, inbox) = split_inbox(main);
    let left_w = status_panel_width(data, ctx).min(content.width);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_w),
            Constraint::Min(0),
            Constraint::Length(RIGHT_W),
        ])
        .split(content);
    draw_status(frame, cols[0], data, ctx);
    draw_orchestration(frame, cols[1], state, data, ctx, hit);
    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).split(cols[2]);
    draw_detail(frame, right[0], state, data, ctx, hit);
    draw_messages(frame, right[1], state, data, ctx, hit);
    draw_inbox(frame, inbox, state, data, ctx, hit);
    draw_footer(frame, footer, state, ctx);
    draw_popup(frame, area, state, data, ctx, hit);
}

fn draw_medium(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    let (main, footer) = split_frame(area);
    let (content, inbox) = split_inbox(main);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(RIGHT_W)])
        .split(content);
    draw_orchestration(frame, cols[0], state, data, ctx, hit);
    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).split(cols[1]);
    draw_detail(frame, right[0], state, data, ctx, hit);
    draw_messages(frame, right[1], state, data, ctx, hit);
    draw_inbox(frame, inbox, state, data, ctx, hit);
    draw_footer(frame, footer, state, ctx);
    draw_popup(frame, area, state, data, ctx, hit);
}

fn draw_narrow(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    let (main, footer) = split_frame(area);
    let (rest, inbox) = split_inbox(main);
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Percentage(40)]).split(rest);
    draw_narrow_orchestration(frame, rows[0], state, data, ctx, hit);
    draw_detail(frame, rows[1], state, data, ctx, hit);
    draw_inbox(frame, inbox, state, data, ctx, hit);
    draw_footer(frame, footer, state, ctx);
    draw_popup(frame, area, state, data, ctx, hit);
}

/// Keeps `selected` inside `height` visible rows, scrolling just enough to show it.
fn scroll_offset(selected: usize, height: u16, len: usize) -> u16 {
    let height = height as usize;
    if height == 0 || len <= height {
        return 0;
    }
    let selected = selected.min(len - 1);
    let max = len - height;
    selected.saturating_sub(height - 1).min(max) as u16
}

fn render_line(frame: &mut Frame, area: Rect, y: &mut u16, line: Line<'static>) {
    if *y < area.bottom() && area.width > 0 {
        frame.render_widget(
            Paragraph::new(line),
            Rect {
                x: area.x,
                y: *y,
                width: area.width,
                height: 1,
            },
        );
        *y = y.saturating_add(1);
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

fn status_lines(data: &BoardData, ctx: Ctx) -> Vec<Line<'static>> {
    let Some(view) = &data.view else {
        return Vec::new();
    };
    let mut counts = [0usize; 5];
    for agent in &view.agents {
        counts[status_kind(agent) as usize] += 1;
    }
    const KINDS: [(StatusKind, &str); 5] = [
        (StatusKind::Working, "working"),
        (StatusKind::Blocked, "blocked"),
        (StatusKind::Done, "done"),
        (StatusKind::Error, "exited"),
        (StatusKind::Unknown, "unknown"),
    ];
    let mut lines = Vec::new();
    for (kind, label) in KINDS {
        let count = counts[kind as usize];
        if count == 0 {
            continue;
        }
        lines.push(Line::from(Span::styled(
            format!("{} {label} {count}", status_symbol(kind, ctx.ascii)),
            Style::new().fg(status_color(kind, ctx.theme)),
        )));
    }
    lines
}

fn status_panel_width(data: &BoardData, ctx: Ctx) -> u16 {
    let max_line = status_lines(data, ctx)
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(1);
    max_line.saturating_add(2).max(STATUS_MIN_W)
}

fn draw_status(frame: &mut Frame, area: Rect, data: &BoardData, ctx: Ctx) {
    let block = panel(" Status ".into(), ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let lines = status_lines(data, ctx);
    if lines.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// One card placed on the orchestration panel.
struct CardSlot {
    index: usize,
    rect: Rect,
}

/// One visual row of cards: agents at the same depth that fit side by side.
struct CardRow {
    cards: Vec<CardSlot>,
}

/// Narrow layout: one line per agent, indented by depth, scrolled to the selection.
fn draw_narrow_orchestration(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    hit.panels.push((area, Panel::Orchestration));
    let name = data
        .view
        .as_ref()
        .map(|v| sanitize_one_line(&v.key.name))
        .unwrap_or_default();
    let title = if name.is_empty() {
        " orchestration ".to_string()
    } else {
        format!(" {name} · orchestration ")
    };
    let block = panel(title, ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(view) = &data.view else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    if view.agents.is_empty() {
        render_text(
            frame,
            inner,
            "(no agents)",
            Style::new().fg(ctx.theme.muted),
        );
        return;
    }

    let parents = flows::parents(&view.agents);
    let selected = state
        .sel_agent
        .as_deref()
        .and_then(|address| view.agents.iter().position(|a| a.address == address));
    let nodes = agent_nodes(&view.agents);
    let blocks = layout::ceo_blocks(&nodes);
    let mut is_orphan = vec![false; view.agents.len()];
    if let Some(no_ceo) = blocks.iter().find(|b| b.kind == BlockKind::NoCeo) {
        for lvl in &no_ceo.levels {
            for &idx in lvl {
                if let Some(slot) = is_orphan.get_mut(idx) {
                    *slot = true;
                }
            }
        }
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut line_agents: Vec<Option<String>> = Vec::new();
    let mut selected_line = None;
    let push_agent =
        |index: usize,
         lines: &mut Vec<Line<'static>>,
         line_agents: &mut Vec<Option<String>>,
         selected_line: &mut Option<usize>| {
            let agent = &view.agents[index];
            let is_selected = selected == Some(index);
            if is_selected {
                *selected_line = Some(lines.len());
            }
            let exited = agent.state == "exited";
            let kind = status_kind(agent);
            let base = if is_selected {
                Style::new()
                    .fg(ctx.theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else if exited {
                Style::new().fg(ctx.theme.muted)
            } else {
                Style::new()
            };
            let symbol_style = if is_selected {
                base
            } else {
                Style::new().fg(status_color(kind, ctx.theme))
            };
            let mut spans = vec![
                Span::styled(
                    format!(
                        "{}{} ",
                        "  ".repeat(agent.depth),
                        status_symbol(kind, ctx.ascii)
                    ),
                    symbol_style,
                ),
                Span::styled(sanitize_one_line(&agent.label), base),
            ];
            if let Some(edge) = data.edges.iter().find(|e| {
                e.child == index && parents.get(index).copied().flatten() == Some(e.parent)
            }) {
                let color = if flows::is_live(edge, data.now_ms) {
                    ctx.theme.accent
                } else {
                    ctx.theme.muted
                };
                let mut style = Style::new().fg(color);
                if is_selected {
                    style = style.add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(
                    format!(
                        " {} {}",
                        sanitize_one_line(&edge.kind),
                        age_since(data.now_ms, edge.ts_ms)
                    ),
                    style,
                ));
            }
            let app = sanitize_one_line(&agent.app);
            let model = sanitize_one_line(&agent.model);
            let ident = match (app.is_empty(), model.is_empty()) {
                (true, true) => String::new(),
                (true, false) => model,
                (false, true) => app,
                (false, false) => format!("{app}/{model}"),
            };
            if !ident.is_empty() {
                spans.push(Span::styled(format!(" {ident}"), base));
            }
            lines.push(Line::from(spans));
            line_agents.push(Some(agent.address.clone()));
        };

    for (index, &orphan) in is_orphan.iter().enumerate() {
        if !orphan {
            push_agent(index, &mut lines, &mut line_agents, &mut selected_line);
        }
    }
    if is_orphan.iter().any(|&o| o) {
        lines.push(muted_line(
            if ctx.ascii {
                "-- no CEO"
            } else {
                "── no CEO"
            },
            ctx,
        ));
        line_agents.push(None);
        for (index, &orphan) in is_orphan.iter().enumerate() {
            if orphan {
                push_agent(index, &mut lines, &mut line_agents, &mut selected_line);
            }
        }
    }
    let offset = scroll_offset(selected_line.unwrap_or(0), inner.height, lines.len());
    for (i, addr) in line_agents.iter().enumerate().skip(offset as usize) {
        let row = i.saturating_sub(offset as usize) as u16;
        if row >= inner.height {
            break;
        }
        if let Some(addr) = addr {
            hit.agent_cards.push((
                Rect {
                    x: inner.x,
                    y: inner.y + row,
                    width: inner.width,
                    height: 1,
                },
                addr.clone(),
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), inner);
}

/// Width of one card in a row of `count` cards, filling the panel up to 32 columns.
fn row_card_width(inner_width: u16, count: usize) -> u16 {
    let count = (count.max(1)) as u16;
    let gaps = count.saturating_sub(1).saturating_mul(CARD_GAP);
    let span = inner_width.saturating_sub(gaps);
    (span / count)
        .min(CARD_MAX_W)
        .max(CARD_MIN_W.min(inner_width))
}

fn agent_nodes(agents: &[AgentView]) -> Vec<layout::Node> {
    agents
        .iter()
        .map(|a| layout::Node {
            depth: a.depth,
            worker: a.role.as_deref() == Some("worker"),
        })
        .collect()
}

/// Place cards row by row for one block's levels, wrapping when a card would be narrower than CARD_MIN_W.
fn block_card_rows(inner: Rect, levels: &[Vec<usize>]) -> (Vec<CardRow>, u16) {
    let per_row = usize::from((inner.width + CARD_GAP) / (CARD_MIN_W + CARD_GAP)).max(1);
    let mut rows = Vec::new();
    let mut y = inner.y;
    let mut first = true;
    for indices in levels {
        if indices.is_empty() {
            continue;
        }
        for chunk in indices.chunks(per_row) {
            if !first {
                y = y.saturating_add(CONNECTOR_ROWS);
            }
            if y >= inner.bottom() {
                return (rows, y);
            }
            let width = row_card_width(inner.width, chunk.len());
            let mut cards = Vec::new();
            for (col, &index) in chunk.iter().enumerate() {
                let x = inner.x + (col as u16).saturating_mul(width + CARD_GAP);
                if x >= inner.right() {
                    break;
                }
                let rect = Rect {
                    x,
                    y,
                    width: width.min(inner.right() - x),
                    height: CARD_H.min(inner.bottom() - y),
                };
                if rect.width < 3 || rect.height < 2 {
                    continue;
                }
                cards.push(CardSlot { index, rect });
            }
            if cards.is_empty() {
                return (rows, y);
            }
            rows.push(CardRow { cards });
            y = y.saturating_add(CARD_H);
            first = false;
        }
    }
    (rows, y)
}

/// Truncate to `max` display columns, marking the cut with `…`.
fn fit_text(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if text.width() <= max {
        return text.to_string();
    }
    let target = max - 1;
    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > target {
            break;
        }
        out.push(ch);
        width += w;
    }
    out.push('…');
    out
}

fn card_centre(rect: Rect) -> u16 {
    rect.x + rect.width / 2
}

fn put_cell(frame: &mut Frame, x: u16, y: u16, symbol: &'static str, color: Color) {
    let area = frame.area();
    if x < area.x || x >= area.right() || y < area.y || y >= area.bottom() {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(symbol, Style::new().fg(color))),
        Rect {
            x,
            y,
            width: 1,
            height: 1,
        },
    );
}

fn draw_orchestration(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    hit.panels.push((area, Panel::Orchestration));
    let name = data
        .view
        .as_ref()
        .map(|v| sanitize_one_line(&v.key.name))
        .unwrap_or_default();
    let title = if name.is_empty() {
        " orchestration ".to_string()
    } else {
        format!(" {name} · orchestration ")
    };
    let block = panel(title, ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(view) = &data.view else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    if view.agents.is_empty() {
        render_text(
            frame,
            inner,
            "(no agents)",
            Style::new().fg(ctx.theme.muted),
        );
        return;
    }

    let nodes = agent_nodes(&view.agents);
    let blocks = layout::ceo_blocks(&nodes);
    let parents = flows::parents(&view.agents);
    let mut drawn = 0usize;
    match layout::block_widths(&blocks, inner.width, CARD_MIN_W, CARD_GAP) {
        Some(widths) => {
            let mut x = inner.x;
            for (block, width) in blocks.iter().zip(widths) {
                let rect = Rect {
                    x,
                    y: inner.y,
                    width: width.min(inner.right().saturating_sub(x)),
                    height: inner.height,
                };
                draw_block(frame, rect, block, &parents, state, data, ctx, &mut drawn, hit);
                x = x.saturating_add(width).saturating_add(CARD_GAP);
            }
        }
        None => {
            let mut y = inner.y;
            for (i, block) in blocks.iter().enumerate() {
                if i > 0 {
                    y = y.saturating_add(1);
                }
                if y >= inner.bottom() {
                    break;
                }
                let rect = Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: inner.bottom().saturating_sub(y),
                };
                y = draw_block(frame, rect, block, &parents, state, data, ctx, &mut drawn, hit);
            }
        }
    }
    // Cards that did not fit: say so on the bottom border instead of hiding them silently.
    let hidden = view.agents.len().saturating_sub(drawn);
    if hidden > 0 && area.height > 1 && area.width > 4 {
        let arrow = if ctx.ascii { "v" } else { "↓" };
        let note = format!(" {arrow} {hidden} more ");
        let width = (note.chars().count() as u16).min(area.width.saturating_sub(4));
        render_text(
            frame,
            Rect {
                x: area.x + 2,
                y: area.bottom() - 1,
                width,
                height: 1,
            },
            &note,
            Style::new().fg(ctx.theme.warn),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_block(
    frame: &mut Frame,
    area: Rect,
    block: &layout::Block,
    parents: &[Option<usize>],
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    drawn: &mut usize,
    hit: &mut BoardHitMap,
) -> u16 {
    let Some(view) = &data.view else {
        return area.y;
    };
    if area.width == 0 || area.height == 0 {
        return area.y;
    }
    let is_no_ceo = block.kind == BlockKind::NoCeo;
    // No room for a card under the caption: draw nothing (the "more" note counts the cards).
    if is_no_ceo && area.height < 2 {
        return area.y;
    }
    let cards_area = if is_no_ceo {
        render_text(
            frame,
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            },
            "no CEO",
            Style::new().fg(ctx.theme.muted),
        );
        Rect {
            x: area.x,
            y: area.y.saturating_add(1),
            width: area.width,
            height: area.height.saturating_sub(1),
        }
    } else {
        area
    };

    let (rows, end_y) = block_card_rows(cards_area, &block.levels);
    *drawn += rows.iter().map(|row| row.cards.len()).sum::<usize>();
    for row in &rows {
        for slot in &row.cards {
            draw_card(
                frame,
                slot.rect,
                &view.agents[slot.index],
                state,
                ctx,
                data.now_ms,
                hit,
            );
        }
    }
    if is_no_ceo {
        return end_y;
    }

    for (row_index, row) in rows.iter().enumerate() {
        if row_index == 0 {
            continue;
        }
        let Some(top) = row.cards.iter().map(|slot| slot.rect.y).min() else {
            continue;
        };
        let bus_y = top.saturating_sub(CONNECTOR_ROWS);
        let drop_y = top.saturating_sub(1);

        let mut groups: Vec<(usize, Vec<&CardSlot>)> = Vec::new();
        for slot in &row.cards {
            let Some(parent) = parents.get(slot.index).copied().flatten() else {
                continue;
            };
            match groups.iter_mut().find(|(known, _)| *known == parent) {
                Some((_, kids)) => kids.push(slot),
                None => groups.push((parent, vec![slot])),
            }
        }
        for (parent, kids) in groups {
            let Some((parent_row, parent_slot)) = slot_in_rows(&rows, parent) else {
                continue;
            };
            if parent_row >= row_index {
                continue;
            }
            draw_connectors(
                frame,
                cards_area,
                parent_slot,
                &kids,
                parent_row + 1 == row_index,
                bus_y,
                drop_y,
                &data.edges,
                data.now_ms,
                ctx,
            );
        }
    }
    end_y
}

/// Row index and slot holding agent `index`, when its card fits on screen.
fn slot_in_rows(rows: &[CardRow], index: usize) -> Option<(usize, &CardSlot)> {
    rows.iter().enumerate().find_map(|(row_index, row)| {
        row.cards
            .iter()
            .find(|slot| slot.index == index)
            .map(|slot| (row_index, slot))
    })
}

fn draw_card(
    frame: &mut Frame,
    area: Rect,
    agent: &AgentView,
    state: &BoardState,
    ctx: Ctx,
    now_ms: i64,
    hit: &mut BoardHitMap,
) {
    hit.agent_cards.push((area, agent.address.clone()));
    let selected = state.sel_agent.as_deref() == Some(agent.address.as_str());
    let exited = agent.state == "exited";
    let border = if selected {
        ctx.theme.selected
    } else if exited {
        ctx.theme.muted
    } else {
        ctx.theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border));
    let block = if ctx.ascii {
        block.border_set(ascii_borders())
    } else {
        block.border_type(BorderType::Rounded)
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = inner.width as usize;

    let kind = status_kind(agent);
    let muted = Style::new().fg(ctx.theme.muted);
    let symbol_color = if exited {
        ctx.theme.muted
    } else {
        status_color(kind, ctx.theme)
    };
    let is_ceo = layout::is_ceo(layout::Node {
        depth: agent.depth,
        worker: agent.role.as_deref() == Some("worker"),
    });
    let label_style = if exited {
        muted
    } else if is_ceo {
        Style::new()
            .fg(ctx.theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(ctx.theme.title)
    };
    let label = fit_text(&sanitize_one_line(&agent.label), width.saturating_sub(2));
    let line1 = Line::from(vec![
        Span::styled(
            format!("{} ", status_symbol(kind, ctx.ascii)),
            Style::new().fg(symbol_color),
        ),
        Span::styled(label, label_style),
    ])
    .centered();

    let model = sanitize_one_line(&agent.model);
    let app = sanitize_one_line(&agent.app);
    let ident = if !model.is_empty() {
        model
    } else if !app.is_empty() {
        app
    } else {
        "-".to_string()
    };
    let line2 = Line::from(Span::styled(fit_text(&ident, width), muted)).centered();

    let base = if exited {
        muted
    } else {
        Style::new().fg(Color::Reset)
    };
    let line3 = if let Some(doing) = &agent.doing {
        let summary = sanitize_one_line(&doing.summary);
        let age = age_since(now_ms, doing.ts_ms);
        let prefix = "doing: ";
        let summary_fits = prefix.width() + summary.width() <= width;
        let with_age = summary_fits && prefix.width() + summary.width() + 1 + age.width() <= width;
        let mut spans = vec![Span::styled(prefix, muted)];
        if with_age {
            spans.push(Span::styled(summary, base));
            spans.push(Span::styled(format!(" {age}"), muted));
        } else {
            let summary = fit_text(&summary, width.saturating_sub(prefix.width()));
            spans.push(Span::styled(summary, base));
        }
        Line::from(spans).centered()
    } else if let Some(stage) = &agent.stage {
        let prefix = "stage: ";
        let stage = fit_text(
            &sanitize_one_line(stage),
            width.saturating_sub(prefix.width()),
        );
        Line::from(vec![Span::styled(prefix, muted), Span::styled(stage, base)]).centered()
    } else {
        muted_line("-", ctx).centered()
    };

    let line4 = match agent
        .task
        .as_deref()
        .map(sanitize_one_line)
        .filter(|s| !s.is_empty())
    {
        Some(task) => {
            let style = if exited {
                muted
            } else {
                Style::new().fg(ctx.theme.selected)
            };
            Line::from(Span::styled(fit_text(&format!("‹{task}›"), width), style)).centered()
        }
        None => muted_line("-", ctx).centered(),
    };

    frame.render_widget(Paragraph::new(vec![line1, line2, line3, line4]), inner);
}

/// Box-drawing glyphs for the tree connector, or their ASCII fallbacks.
struct ConnGlyphs {
    vertical: &'static str,
    horizontal: &'static str,
    /// `┬`: a horizontal line with a stub going down.
    down: &'static str,
    /// `┴`: a horizontal line with a stub going up.
    up: &'static str,
    corner_left: &'static str,
    corner_right: &'static str,
    /// `┼`: stem and drop share one column.
    cross: &'static str,
}

impl ConnGlyphs {
    fn new(ascii: bool) -> Self {
        if ascii {
            Self {
                vertical: "|",
                horizontal: "-",
                down: "+",
                up: "+",
                corner_left: "+",
                corner_right: "+",
                cross: "+",
            }
        } else {
            Self {
                vertical: "│",
                horizontal: "─",
                down: "┬",
                up: "┴",
                corner_left: "┌",
                corner_right: "┐",
                cross: "┼",
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_connectors(
    frame: &mut Frame,
    area: Rect,
    parent: &CardSlot,
    kids: &[&CardSlot],
    adjacent: bool,
    bus_y: u16,
    drop_y: u16,
    edges: &[Edge],
    now_ms: i64,
    ctx: Ctx,
) {
    let glyphs = ConnGlyphs::new(ctx.ascii);
    let parent_cx = card_centre(parent.rect);
    let child_centres: Vec<u16> = kids.iter().map(|kid| card_centre(kid.rect)).collect();
    let (bus_start, bus_end) = if adjacent {
        let parent_bottom = parent.rect.y + parent.rect.height - 1;
        put_cell(
            frame,
            parent_cx,
            parent_bottom,
            glyphs.down,
            ctx.theme.border,
        );
        let start = child_centres
            .iter()
            .copied()
            .min()
            .unwrap_or(parent_cx)
            .min(parent_cx);
        let end = child_centres
            .iter()
            .copied()
            .max()
            .unwrap_or(parent_cx)
            .max(parent_cx);
        (start, end)
    } else {
        // A wrapped continuation row only has the two rows above it: mark the stem at the
        // left margin of its gap and feed the bus from there.
        put_cell(frame, area.x, bus_y, glyphs.vertical, ctx.theme.border);
        let end = child_centres
            .iter()
            .copied()
            .max()
            .unwrap_or(parent_cx)
            .max(parent_cx);
        (area.x.saturating_add(1), end)
    };

    let mut x = bus_start;
    loop {
        let is_parent = adjacent && x == parent_cx;
        let is_child = child_centres.contains(&x);
        let symbol = match (is_parent, is_child) {
            // Parent straight above its only child: a plain vertical, not a junction.
            (true, true) if bus_start == bus_end => glyphs.vertical,
            (true, true) => glyphs.cross,
            (true, false) => glyphs.up,
            (false, true) if x == bus_start => glyphs.corner_left,
            (false, true) if x == bus_end => glyphs.corner_right,
            (false, true) => glyphs.down,
            (false, false) if x == bus_start => glyphs.corner_left,
            (false, false) if x == bus_end => glyphs.corner_right,
            (false, false) => glyphs.horizontal,
        };
        put_cell(frame, x, bus_y, symbol, ctx.theme.border);
        if x >= bus_end {
            break;
        }
        x += 1;
    }

    for (i, kid) in kids.iter().enumerate() {
        let cx = card_centre(kid.rect);
        let edge = edges.iter().find(|edge| edge.child == kid.index);
        let (color, label) = match edge {
            Some(edge) => {
                let color = if flows::is_live(edge, now_ms) {
                    ctx.theme.accent
                } else {
                    ctx.theme.muted
                };
                let label = format!(
                    "{} {}",
                    sanitize_one_line(&edge.kind),
                    age_since(now_ms, edge.ts_ms)
                );
                (color, Some(label))
            }
            None => (ctx.theme.muted, None),
        };
        put_cell(frame, cx, drop_y, glyphs.vertical, color);
        put_cell(frame, cx, kid.rect.y, glyphs.up, color);
        let Some(label) = label else { continue };
        let next_x = kids
            .get(i + 1)
            .map(|next| card_centre(next.rect))
            .unwrap_or(area.right());
        let max_w = next_x
            .saturating_sub(cx + 1)
            .min(area.right().saturating_sub(cx + 1));
        let label = fit_text(&label, max_w as usize);
        if label.is_empty() {
            continue;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label, Style::new().fg(color)))),
            Rect {
                x: cx + 1,
                y: drop_y,
                width: max_w,
                height: 1,
            },
        );
    }
}

fn draw_detail(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    hit.panels.push((area, Panel::Detail));
    let agent = state.selected_agent(data);
    let title = match agent {
        Some(agent) => {
            let label = sanitize_one_line(&agent.label);
            let app = if agent.app.is_empty() {
                "-".to_string()
            } else {
                sanitize_one_line(&agent.app)
            };
            format!(" {label} · {app} ")
        }
        None => " detail ".to_string(),
    };
    let block = panel(title, ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(agent) = agent else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };

    let model = match (agent.model.is_empty(), agent.effort.is_empty()) {
        (true, true) => "-".to_string(),
        (false, true) => sanitize_one_line(&agent.model),
        (true, false) => sanitize_one_line(&agent.effort),
        (false, false) => format!(
            "{}/{}",
            sanitize_one_line(&agent.model),
            sanitize_one_line(&agent.effort)
        ),
    };
    let status = format_status(Some(agent.status.as_str()).filter(|s| !s.is_empty()), None);

    let task = agent
        .task
        .as_deref()
        .map(sanitize_one_line)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "-".to_string());

    let mut rows: Vec<Line<'static>> = vec![
        field_line("app", &dash_value(&agent.app), ctx),
        field_line("model", &model, ctx),
        field_line("status", &sanitize_one_line(&status), ctx),
        field_line("task", &task, ctx),
    ];
    rows.extend(stage_lines(agent.stage.as_deref(), ctx));
    rows.push(ctx_line(agent.context_pct, ctx));
    let tokens = agent
        .tokens
        .map(tokens_label)
        .unwrap_or_else(|| "-".to_string());
    rows.push(field_line("tokens", &tokens, ctx));

    let tail = vec![
        match &agent.doing {
            Some(doing) => field_line(
                "doing",
                &format!(
                    "{} {}",
                    sanitize_one_line(&doing.summary),
                    age_since(data.now_ms, doing.ts_ms)
                ),
                ctx,
            ),
            None => field_line("doing", "-", ctx),
        },
        match &agent.last_msg {
            Some((kind, sent)) => {
                let arrow = match (ctx.ascii, sent) {
                    (true, true) => "->",
                    (true, false) => "<-",
                    (false, true) => "→",
                    (false, false) => "←",
                };
                field_line("last", &format!("{arrow} {}", sanitize_one_line(kind)), ctx)
            }
            None => field_line("last", "-", ctx),
        },
    ];

    let mut y = inner.y;
    for row in rows {
        render_line(frame, inner, &mut y, row);
    }
    if !ctx.ascii
        && let Some(spark) = &agent.spark
        && y < inner.bottom()
    {
        frame.render_widget(
            Sparkline::default()
                .data(spark.clone())
                .style(Style::new().fg(ctx.theme.working)),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
        y = y.saturating_add(1);
    }
    for row in tail {
        render_line(frame, inner, &mut y, row);
    }
}

fn dash_value(value: &str) -> String {
    if value.is_empty() {
        "-".to_string()
    } else {
        sanitize_one_line(value)
    }
}

fn field_line(label: &str, value: &str, ctx: Ctx) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::new().fg(ctx.theme.muted)),
        Span::styled(value.to_string(), Style::new()),
    ])
}

fn stage_lines(stage: Option<&str>, ctx: Ctx) -> Vec<Line<'static>> {
    let Some(raw) = stage.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![field_line("stage", "-", ctx)];
    };
    let clean = sanitize_one_line(raw);
    let lower = clean.to_ascii_lowercase();
    let mut current: Option<usize> = None;
    let mut current_color = ctx.theme.selected;
    let mut badge: Option<(String, Color)> = None;
    match lower.as_str() {
        "wip" => current = Some(0),
        "ready" => current = Some(1),
        "merging" => current = Some(2),
        "merged" => current = Some(3),
        "deployed" => {
            current = Some(3);
            current_color = ctx.theme.ok;
            badge = Some((clean.clone(), ctx.theme.ok));
        }
        "blocked" | "needs-rebase" => {
            current = Some(0);
            current_color = ctx.theme.warn;
            badge = Some((clean.clone(), ctx.theme.warn));
        }
        "failed" => {
            current_color = ctx.theme.error;
            badge = Some((clean.clone(), ctx.theme.error));
        }
        _ => {
            return vec![Line::from(vec![
                Span::styled("stage: ", Style::new().fg(ctx.theme.muted)),
                Span::styled(clean, Style::new().fg(ctx.theme.accent)),
            ])];
        }
    }

    let separator = if ctx.ascii { ">" } else { "→" };
    let mut spans = vec![Span::styled("stage: ", Style::new().fg(ctx.theme.muted))];
    for (i, step) in STEPS.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(separator, Style::new().fg(ctx.theme.muted)));
        }
        let style = if current == Some(i) {
            Style::new().fg(current_color).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(ctx.theme.muted)
        };
        spans.push(Span::styled(*step, style));
    }
    let mut lines = vec![Line::from(spans)];
    if let Some((text, color)) = badge {
        lines.push(Line::from(Span::styled(
            format!("[{text}]"),
            Style::new().fg(color),
        )));
    }
    lines
}

fn ctx_line(pct: Option<f64>, ctx: Ctx) -> Line<'static> {
    let Some(pct) = pct.filter(|p| p.is_finite()) else {
        return field_line("ctx", "-", ctx);
    };
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * 8.0).round() as usize;
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
    Line::from(vec![
        Span::styled("ctx: ", Style::new().fg(ctx.theme.muted)),
        Span::styled(full.to_string().repeat(filled), Style::new().fg(color)),
        Span::styled(
            empty.to_string().repeat(8 - filled),
            Style::new().fg(ctx.theme.muted),
        ),
        Span::styled(format!(" {pct:.0}%"), Style::new().fg(color)),
    ])
}

fn tokens_label(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.0}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

fn age_since(now_ms: i64, ts_ms: i64) -> String {
    age_label(now_ms.saturating_sub(ts_ms) / 1000)
}

/// Terminal label for an address known to this project, else the address itself.
fn agent_label(agents: &[AgentView], address: &str) -> String {
    agents
        .iter()
        .find(|agent| agent.address == address)
        .map(|agent| sanitize_one_line(&agent.label))
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| address.to_string())
}

fn draw_messages(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    hit.panels.push((area, Panel::Detail));
    let title = match state.selected_agent(data) {
        Some(agent) => format!(" Messages · {} ", sanitize_one_line(&agent.label)),
        None => " Messages ".to_string(),
    };
    let block = panel(title, ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if data.conversation.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }

    let agents = data
        .view
        .as_ref()
        .map(|view| view.agents.as_slice())
        .unwrap_or(&[]);
    let needle = state.filter.trim().to_lowercase();
    let arrow = if ctx.ascii { "->" } else { "→" };
    let mut lines: Vec<Line<'static>> = Vec::new();
    for message in &data.conversation {
        let raw_from = sanitize_one_line(&message.from);
        let raw_to = sanitize_one_line(&message.to);
        let from = agent_label(agents, &raw_from);
        let to = agent_label(agents, &raw_to);
        let body = sanitize_one_line(&message.text);
        if !needle.is_empty()
            && !body.to_lowercase().contains(&needle)
            && !from.to_lowercase().contains(&needle)
            && !to.to_lowercase().contains(&needle)
            && !raw_from.to_lowercase().contains(&needle)
            && !raw_to.to_lowercase().contains(&needle)
        {
            continue;
        }
        let kind = sanitize_one_line(kind_to_english(&message.kind));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{from} {arrow} {to} "),
                Style::new().fg(ctx.theme.title),
            ),
            Span::styled(
                format!("{kind} {}", age_since(data.now_ms, message.ts_ms)),
                Style::new().fg(ctx.theme.accent),
            ),
        ]));
        lines.push(Line::from(Span::styled(body, Style::new())));
    }
    if lines.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }
    let scroll = state.msg_scroll.min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        inner,
    );
}

fn draw_inbox(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    hit.panels.push((area, Panel::Inbox));
    let items = data
        .view
        .as_ref()
        .map(|view| view.inbox.as_slice())
        .unwrap_or(&[]);
    let title = format!(" Inbox ({}) ", items.len());
    let block = panel(title, ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if items.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }
    let marker = if ctx.ascii { ">" } else { "▸" };
    let mut lines: Vec<Line<'static>> = Vec::new();
    for item in items {
        let selected = state.sel_inbox == Some(item.id);
        let style = if selected {
            Style::new()
                .fg(ctx.theme.selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        let kind = item
            .kind
            .as_deref()
            .map(|kind| sanitize_one_line(kind_to_english(kind)))
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| "-".to_string());
        let terminal = item
            .terminal
            .as_deref()
            .map(sanitize_one_line)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "-".to_string());
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", if selected { marker } else { " " }), style),
            Span::styled(format!("#{} ", item.id), style),
            Span::styled(format!("{kind} {terminal} "), style),
            Span::styled(sanitize_one_line(&item.text), style),
        ]));
    }
    let selected = items
        .iter()
        .position(|item| Some(item.id) == state.sel_inbox)
        .unwrap_or(0);
    let offset = scroll_offset(selected, inner.height, items.len());
    for (i, item) in items.iter().enumerate().skip(offset as usize) {
        let row = i.saturating_sub(offset as usize) as u16;
        if row >= inner.height {
            break;
        }
        hit.inbox_rows.push((
            Rect {
                x: inner.x,
                y: inner.y + row,
                width: inner.width,
                height: 1,
            },
            item.id,
        ));
    }
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), inner);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &BoardState, ctx: Ctx) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (text, style) = match &state.note {
        Some(note) => (
            sanitize_one_line(&note.text),
            Style::new().fg(if note.ok {
                ctx.theme.ok
            } else {
                ctx.theme.error
            }),
        ),
        None => (
            if ctx.ascii {
                " up/down select  tab panel  enter open  r reply  d done  s send  / filter  t theme  q back"
                    .to_string()
            } else {
                " ↑↓ select  tab panel  ⏎ open  r reply  d done  s send  / filter  t theme  q back"
                    .to_string()
            },
            Style::new().fg(ctx.theme.muted),
        ),
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}

fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn draw_popup(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
    hit: &mut BoardHitMap,
) {
    let (title, lines, width, height) = match &state.mode {
        Mode::Normal => return,
        Mode::Reply { id, input } => (
            format!(" Reply to #{id} "),
            vec![
                Line::from(Span::styled(sanitize_one_line(input), Style::new())),
                muted_line("enter sends · esc cancels", ctx),
            ],
            60,
            5,
        ),
        Mode::Send { to, kind, input } => {
            let kind = KINDS_EN
                .get(*kind)
                .map(|(english, _)| *english)
                .unwrap_or("-");
            (
                format!(" Send to {} · {kind} ", sanitize_one_line(to)),
                vec![
                    Line::from(Span::styled(sanitize_one_line(input), Style::new())),
                    muted_line("tab cycles type · enter sends · esc cancels", ctx),
                ],
                60,
                6,
            )
        }
        Mode::ConfirmDone { id } => (
            format!(" Close #{id} "),
            vec![
                Line::from(format!("Close inbox item #{id}?")),
                Line::from(Span::styled(
                    "y confirms, any other key cancels",
                    Style::new().fg(ctx.theme.warn),
                )),
            ],
            44,
            5,
        ),
        Mode::ShowItem { id } => {
            let item = data
                .view
                .as_ref()
                .and_then(|view| view.inbox.iter().find(|item| item.id == *id));
            let mut lines = Vec::new();
            match item {
                Some(item) => {
                    let kind = item
                        .kind
                        .as_deref()
                        .map(|kind| sanitize_one_line(kind_to_english(kind)))
                        .unwrap_or_else(|| "-".to_string());
                    let terminal = item
                        .terminal
                        .as_deref()
                        .map(sanitize_one_line)
                        .unwrap_or_else(|| "-".to_string());
                    lines.push(Line::from(Span::styled(
                        format!("#{} {kind} {terminal}", item.id),
                        Style::new().fg(ctx.theme.title),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        sanitize_one_line(&item.text),
                        Style::new(),
                    )));
                    lines.push(Line::from(""));
                    lines.push(muted_line("any key closes", ctx));
                }
                None => lines.push(muted_line(format!("#{id} is gone"), ctx)),
            }
            (format!(" Item #{id} "), lines, 70, 14)
        }
        Mode::Filter { input } => (
            " Filter ".to_string(),
            vec![
                Line::from(Span::styled(sanitize_one_line(input), Style::new())),
                muted_line("enter applies · esc clears", ctx),
            ],
            50,
            5,
        ),
        Mode::PickProject { idx } => {
            let mut lines = Vec::new();
            if data.projects.is_empty() {
                lines.push(muted_line("-", ctx));
            }
            for (i, project) in data.projects.iter().enumerate() {
                let selected = i == *idx;
                let style = if selected {
                    Style::new()
                        .fg(ctx.theme.selected)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "{} {}",
                        if selected {
                            if ctx.ascii { ">" } else { "▸" }
                        } else {
                            " "
                        },
                        // One row per project: the scroll offset counts items, so never wrap.
                        fit_text(&sanitize_one_line(&project.key.name), 45)
                    ),
                    style,
                )));
            }
            let height = (lines.len() as u16 + 2).min(MAX_POPUP_H);
            (" Pick project ".to_string(), lines, 50, height)
        }
        Mode::Help => (
            " Help ".to_string(),
            vec![
                Line::from("↑↓ hjkl    move          tab   next panel"),
                Line::from("enter      open / details"),
                Line::from("r          reply          d     done"),
                Line::from("s          send           /     filter"),
                Line::from("t          theme          p     pick project"),
                Line::from("?          help           q/esc leave board"),
                Line::from(""),
                muted_line(
                    "every refresh is live; failed actions show in the footer",
                    ctx,
                ),
            ],
            64,
            12,
        ),
    };

    let rect = popup_area(area, width, height);
    frame.render_widget(Clear, rect);
    let block = panel(title, ctx.theme.title, ctx);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let scroll = match &state.mode {
        Mode::PickProject { idx } => scroll_offset(*idx, inner.height, data.projects.len()),
        _ => 0,
    };
    if let Mode::PickProject { .. } = &state.mode {
        hit.panels.push((rect, Panel::Projects));
        for (i, _project) in data.projects.iter().enumerate().skip(scroll as usize) {
            let row = i.saturating_sub(scroll as usize) as u16;
            if row >= inner.height {
                break;
            }
            hit.picker_rows.push((
                Rect {
                    x: inner.x,
                    y: inner.y + row,
                    width: inner.width,
                    height: 1,
                },
                i,
            ));
        }
    }
    let paragraph = Paragraph::new(lines).scroll((scroll, 0));
    let paragraph = if matches!(state.mode, Mode::PickProject { .. }) {
        paragraph
    } else {
        paragraph.wrap(Wrap { trim: true })
    };
    frame.render_widget(paragraph, inner);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Working = 0,
    Blocked = 1,
    Done = 2,
    Error = 3,
    Unknown = 4,
}

fn status_kind(agent: &AgentView) -> StatusKind {
    if agent.state == "exited" {
        return StatusKind::Error;
    }
    let status = agent.status.to_ascii_lowercase();
    if status.starts_with("blocked") {
        StatusKind::Blocked
    } else if status.contains("working") || status.contains("running") {
        StatusKind::Working
    } else if status.contains("done") || status.contains("idle") {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect { x, y, width, height }
    }

    #[test]
    fn hit_prefers_specific_targets_over_panels() {
        let map = BoardHitMap {
            panels: vec![(rect(0, 0, 20, 10), Panel::Orchestration)],
            agent_cards: vec![(rect(2, 2, 5, 3), "t1".into())],
            inbox_rows: vec![(rect(0, 8, 20, 1), 42)],
            ..BoardHitMap::default()
        };
        assert_eq!(hit(&map, 3, 3), Some(HitTarget::AgentCard("t1".into())));
        assert_eq!(hit(&map, 1, 8), Some(HitTarget::InboxRow(42)));
        assert_eq!(hit(&map, 1, 1), Some(HitTarget::Panel(Panel::Orchestration)));
        assert_eq!(hit(&map, 99, 99), None);
    }

    #[test]
    fn hit_finds_project_and_picker_rows() {
        let map = BoardHitMap {
            project_rows: vec![(rect(0, 1, 10, 1), 0), (rect(0, 2, 10, 1), 1)],
            picker_rows: vec![(rect(5, 5, 10, 1), 3)],
            ..BoardHitMap::default()
        };
        assert_eq!(hit(&map, 2, 2), Some(HitTarget::ProjectRow(1)));
        assert_eq!(hit(&map, 6, 5), Some(HitTarget::PickerRow(3)));
    }
}
