//! Board rendering with ratatui widgets. No I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Sparkline, Wrap};

use super::BoardData;
use super::flows::{self, Edge};
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

pub fn draw(frame: &mut Frame, state: &BoardState, data: &BoardData, theme: &Theme, ascii: bool) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let ctx = Ctx { theme, ascii };
    match layout_mode(area.width, area.height) {
        LayoutMode::Wide => draw_wide(frame, area, state, data, ctx),
        LayoutMode::Medium => draw_medium(frame, area, state, data, ctx),
        LayoutMode::Narrow => draw_narrow(frame, area, state, data, ctx),
    }
}

/// Theme + ASCII flag, passed down instead of two separate arguments.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    theme: &'a Theme,
    ascii: bool,
}

const LEFT_W: u16 = 20;
const RIGHT_W: u16 = 34;
const CARD_W: u16 = 18;
const CARD_H: u16 = 4;
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

fn draw_wide(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
    let (main, footer) = split_frame(area);
    let (content, inbox) = split_inbox(main);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LEFT_W),
            Constraint::Min(0),
            Constraint::Length(RIGHT_W),
        ])
        .split(content);
    let left = Layout::vertical([Constraint::Length(8), Constraint::Min(0)]).split(cols[0]);
    draw_projects(frame, left[0], state, data, ctx);
    draw_status(frame, left[1], data, ctx);
    draw_orchestration(frame, cols[1], state, data, ctx);
    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).split(cols[2]);
    draw_detail(frame, right[0], state, data, ctx);
    draw_messages(frame, right[1], state, data, ctx);
    draw_inbox(frame, inbox, state, data, ctx);
    draw_footer(frame, footer, state, ctx);
    draw_popup(frame, area, state, data, ctx);
}

fn draw_medium(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
    let (main, footer) = split_frame(area);
    let (content, inbox) = split_inbox(main);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(RIGHT_W)])
        .split(content);
    draw_orchestration(frame, cols[0], state, data, ctx);
    let right = Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).split(cols[1]);
    draw_detail(frame, right[0], state, data, ctx);
    draw_messages(frame, right[1], state, data, ctx);
    draw_inbox(frame, inbox, state, data, ctx);
    draw_footer(frame, footer, state, ctx);
    draw_popup(frame, area, state, data, ctx);
}

fn draw_narrow(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
    let (main, footer) = split_frame(area);
    let rows = Layout::vertical([
        Constraint::Percentage(45),
        Constraint::Percentage(30),
        Constraint::Percentage(25),
    ])
    .split(main);
    draw_orchestration(frame, rows[0], state, data, ctx);
    draw_detail(frame, rows[1], state, data, ctx);
    draw_inbox(frame, rows[2], state, data, ctx);
    draw_footer(frame, footer, state, ctx);
    draw_popup(frame, area, state, data, ctx);
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

fn draw_projects(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
    let block = panel(" Projects ".into(), ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if data.projects.is_empty() {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    }
    let marker = if ctx.ascii { ">" } else { "▸" };
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, project) in data.projects.iter().enumerate() {
        let selected = state.panel == Panel::Projects && i == state.sel_project;
        let style = if selected {
            Style::new()
                .fg(ctx.theme.selected)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        let name = sanitize_one_line(&project.key.name);
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", if selected { marker } else { " " }), style),
            Span::styled(name, style),
            Span::styled(
                format!(" {}", project.live_agents),
                if selected {
                    style
                } else {
                    Style::new().fg(ctx.theme.working)
                },
            ),
            Span::styled(
                format!("●{}", project.open_inbox),
                if selected {
                    style
                } else {
                    Style::new().fg(ctx.theme.warn)
                },
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_status(frame: &mut Frame, area: Rect, data: &BoardData, ctx: Ctx) {
    let block = panel(" Status ".into(), ctx.theme.border, ctx);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let Some(view) = &data.view else {
        render_text(frame, inner, "-", Style::new().fg(ctx.theme.muted));
        return;
    };
    let mut counts = [0usize; 5];
    for agent in &view.agents {
        counts[status_kind(agent) as usize] += 1;
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    for kind in [
        StatusKind::Done,
        StatusKind::Working,
        StatusKind::Blocked,
        StatusKind::Error,
        StatusKind::Unknown,
    ] {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!(
                "{}{}",
                status_symbol(kind, ctx.ascii),
                counts[kind as usize]
            ),
            Style::new().fg(status_color(kind, ctx.theme)),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_orchestration(
    frame: &mut Frame,
    area: Rect,
    state: &BoardState,
    data: &BoardData,
    ctx: Ctx,
) {
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

    let per_row = usize::from(((inner.width + 1) / (CARD_W + 1)).max(1));
    let max_depth = view.agents.iter().map(|a| a.depth).max().unwrap_or(0);
    let mut depth_rows: Vec<(usize, Vec<usize>)> = Vec::new();
    for depth in 0..=max_depth {
        let indices: Vec<usize> = view
            .agents
            .iter()
            .enumerate()
            .filter(|(_, a)| a.depth == depth)
            .map(|(i, _)| i)
            .collect();
        for chunk in indices.chunks(per_row) {
            depth_rows.push((depth, chunk.to_vec()));
        }
    }

    let mut y = inner.y;
    for (row, (depth, chunk)) in depth_rows.iter().enumerate() {
        if y >= inner.bottom() {
            break;
        }
        for (col, &index) in chunk.iter().enumerate() {
            let x = inner.x + (col as u16) * (CARD_W + 1);
            if x >= inner.right() {
                break;
            }
            let card = Rect {
                x,
                y,
                width: CARD_W.min(inner.right() - x),
                height: CARD_H.min(inner.bottom() - y),
            };
            if card.width < 3 || card.height < 2 {
                continue;
            }
            draw_card(frame, card, &view.agents[index], state, ctx, data.now_ms);
        }
        y = y.saturating_add(CARD_H);
        if let Some((next_depth, next_chunk)) = depth_rows.get(row + 1)
            && *next_depth > *depth
            && y < inner.bottom()
        {
            draw_connectors(
                frame,
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
                next_chunk,
                &data.edges,
                data.now_ms,
                ctx,
            );
            y = y.saturating_add(1);
        }
    }
}

fn draw_card(
    frame: &mut Frame,
    area: Rect,
    agent: &AgentView,
    state: &BoardState,
    ctx: Ctx,
    now_ms: i64,
) {
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

    let kind = status_kind(agent);
    let symbol_color = if exited {
        ctx.theme.muted
    } else {
        status_color(kind, ctx.theme)
    };
    let label = sanitize_one_line(&agent.label);
    let app = sanitize_one_line(&agent.app);
    let model = sanitize_one_line(&agent.model);
    let ident = if app.is_empty() {
        format!("{label} {model}")
    } else if model.is_empty() {
        format!("{label} {app}")
    } else {
        format!("{label} {app}/{model}")
    };
    let base = Style::new().fg(if exited {
        ctx.theme.muted
    } else {
        Color::Reset
    });
    let ident_style = if selected {
        base.add_modifier(Modifier::BOLD)
    } else {
        base
    };
    let line1 = Line::from(vec![
        Span::styled(
            format!("{} ", status_symbol(kind, ctx.ascii)),
            Style::new().fg(symbol_color),
        ),
        Span::styled(ident, ident_style),
    ]);

    let line2 = if let Some(doing) = &agent.doing {
        Line::from(vec![
            Span::styled("doing: ", Style::new().fg(ctx.theme.muted)),
            Span::styled(sanitize_one_line(&doing.summary), base),
            Span::styled(
                format!(" {}", age_since(now_ms, doing.ts_ms)),
                Style::new().fg(ctx.theme.muted),
            ),
        ])
    } else if let Some(stage) = &agent.stage {
        Line::from(vec![
            Span::styled("stage: ", Style::new().fg(ctx.theme.muted)),
            Span::styled(sanitize_one_line(stage), base),
        ])
    } else {
        muted_line("-", ctx)
    };

    frame.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

fn draw_connectors(
    frame: &mut Frame,
    area: Rect,
    chunk: &[usize],
    edges: &[Edge],
    now_ms: i64,
    ctx: Ctx,
) {
    let dash = if ctx.ascii { "-" } else { "─" };
    for (col, &child) in chunk.iter().enumerate() {
        let Some(edge) = edges.iter().find(|e| e.child == child) else {
            continue;
        };
        let x = area.x + (col as u16) * (CARD_W + 1);
        if x >= area.right() {
            continue;
        }
        let width = CARD_W.min(area.right() - x);
        let live = flows::is_live(edge, now_ms);
        let color = if live {
            ctx.theme.accent
        } else {
            ctx.theme.muted
        };
        let label = format!(
            "{dash} {} {} {dash}",
            sanitize_one_line(&edge.kind),
            age_since(now_ms, edge.ts_ms)
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label, Style::new().fg(color)))),
            Rect {
                x,
                y: area.y,
                width,
                height: 1,
            },
        );
    }
}

fn draw_detail(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
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

    let mut rows: Vec<Line<'static>> = vec![
        field_line("app", &dash_value(&agent.app), ctx),
        field_line("model", &model, ctx),
        field_line("status", &sanitize_one_line(&status), ctx),
        field_line("task", "-", ctx),
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

fn draw_messages(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
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

    let needle = state.filter.trim().to_lowercase();
    let arrow = if ctx.ascii { "->" } else { "→" };
    let mut lines: Vec<Line<'static>> = Vec::new();
    for message in &data.conversation {
        let from = sanitize_one_line(&message.from);
        let to = sanitize_one_line(&message.to);
        let body = sanitize_one_line(&message.text);
        if !needle.is_empty()
            && !body.to_lowercase().contains(&needle)
            && !from.to_lowercase().contains(&needle)
            && !to.to_lowercase().contains(&needle)
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

fn draw_inbox(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
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
            .map(sanitize_one_line)
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
    frame.render_widget(Paragraph::new(lines), inner);
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

fn draw_popup(frame: &mut Frame, area: Rect, state: &BoardState, data: &BoardData, ctx: Ctx) {
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
                        .map(sanitize_one_line)
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
                        sanitize_one_line(&project.key.name)
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
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
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
