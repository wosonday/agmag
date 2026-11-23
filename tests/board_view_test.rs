//! Buffer tests for the board rendering (`src/board/view.rs`).

use agmag::board::BoardData;
use agmag::board::flows::Edge;
use agmag::board::state::{BoardState, Mode};
use agmag::board::theme::{ThemeName, theme};
use agmag::board::view::{LayoutMode, draw, layout_mode};
use agmag::project::{AgentView, ProjectKey, ProjectSummary, ProjectView, TurnRow};
use agmag::store::{InboxRow, MessageRow};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

const NOW: i64 = 1_000_000;

fn key() -> ProjectKey {
    ProjectKey {
        root: "D:/p/agmag".into(),
        name: "agmag".into(),
    }
}

fn agent(address: &str, label: &str, depth: usize) -> AgentView {
    AgentView {
        address: address.into(),
        label: label.into(),
        app: "claude-code".into(),
        model: "opus".into(),
        effort: "high".into(),
        status: "working".into(),
        state: "live".into(),
        depth,
        last_sibling: false,
        doing: None,
        last_msg: None,
        context_pct: None,
        tokens: None,
        spark: None,
        stage: None,
    }
}

fn data(agents: Vec<AgentView>) -> BoardData {
    BoardData {
        projects: vec![],
        no_project_inbox: 0,
        view: Some(ProjectView {
            key: key(),
            agents,
            inbox: vec![],
            messages: vec![],
        }),
        edges: vec![],
        conversation: vec![],
        now_ms: NOW,
    }
}

fn render(size: (u16, u16), state: &BoardState, data: &BoardData, ascii: bool) -> Buffer {
    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).unwrap();
    let th = theme(state.theme);
    terminal.draw(|f| draw(f, state, data, &th, ascii)).unwrap();
    terminal.backend().buffer().clone()
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
}

fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| row_text(buf, y))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find(buf: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..buf.area.height {
        let line = row_text(buf, y);
        if let Some(byte) = line.find(needle) {
            let x = line[..byte].chars().count() as u16;
            return (x, y);
        }
    }
    panic!("{needle:?} not found in buffer:\n{}", text(buf));
}

fn has(buf: &Buffer, needle: &str) -> bool {
    (0..buf.area.height).any(|y| row_text(buf, y).contains(needle))
}

/// First occurrence of `needle` whose start column is in `x0..x1`.
fn find_in(buf: &Buffer, needle: &str, x0: u16, x1: u16) -> (u16, u16) {
    for y in 0..buf.area.height {
        let line = row_text(buf, y);
        for (byte, _) in line.match_indices(needle) {
            let x = line[..byte].chars().count() as u16;
            if (x0..x1).contains(&x) {
                return (x, y);
            }
        }
    }
    panic!(
        "{needle:?} not found in columns {x0}..{x1} of buffer:\n{}",
        text(buf)
    );
}

fn fg_of(buf: &Buffer, needle: &str) -> Color {
    let (x, y) = find(buf, needle);
    buf[(x, y)].fg
}

fn fg_in(buf: &Buffer, needle: &str, x0: u16, x1: u16) -> Color {
    let (x, y) = find_in(buf, needle, x0, x1);
    buf[(x, y)].fg
}

fn selected_state(sel_agent: &str) -> BoardState {
    let mut state = BoardState::new(Some(key()), ThemeName::CatppuccinMocha);
    state.sel_agent = Some(sel_agent.into());
    state
}

#[test]
fn layout_mode_thresholds() {
    assert_eq!(layout_mode(120, 20), LayoutMode::Wide);
    assert_eq!(layout_mode(119, 20), LayoutMode::Medium);
    assert_eq!(layout_mode(79, 40), LayoutMode::Narrow);
    assert_eq!(layout_mode(200, 19), LayoutMode::Narrow);
}

#[test]
fn wide_has_all_panel_titles() {
    let mut data = data(vec![agent("t9", "t9", 0)]);
    data.projects = vec![ProjectSummary {
        key: key(),
        live_agents: 1,
        open_inbox: 0,
    }];
    let state = selected_state("t9");
    let buf = render((140, 40), &state, &data, false);
    for title in ["Projects", "Status", "orchestration", "Messages", "Inbox"] {
        assert!(has(&buf, title), "missing title {title:?}:\n{}", text(&buf));
    }
}

#[test]
fn medium_drops_projects_column() {
    let data = data(vec![agent("t9", "t9", 0)]);
    let state = selected_state("t9");
    let buf = render((100, 30), &state, &data, false);
    assert!(has(&buf, "orchestration"), "{}", text(&buf));
    assert!(has(&buf, "agmag"), "project name missing from title");
    assert!(!has(&buf, "Projects"));
    assert!(!has(&buf, "Status"));
}

#[test]
fn narrow_stacks() {
    let data = data(vec![agent("t1", "alpha", 0)]);
    let state = selected_state("t1");
    let buf = render((70, 20), &state, &data, false);
    assert!(!has(&buf, "Projects"));
    assert!(has(&buf, "alpha"), "{}", text(&buf));
    assert!(has(&buf, "Inbox"), "{}", text(&buf));
    let (_, agent_y) = find(&buf, "alpha");
    let (_, inbox_y) = find(&buf, "Inbox");
    assert!(
        agent_y < inbox_y,
        "expected orchestration above inbox ({agent_y} < {inbox_y})"
    );
}

#[test]
fn narrow_lists_every_agent() {
    let mut agents = vec![agent("t0", "ceo", 0)];
    for i in 1..=5 {
        let label = format!("worker-{i}");
        agents.push(agent(&format!("t{i}"), &label, 1));
    }
    let data = data(agents);
    let state = selected_state("t0");
    let buf = render((70, 20), &state, &data, false);
    for label in [
        "ceo", "worker-1", "worker-2", "worker-3", "worker-4", "worker-5",
    ] {
        assert!(
            has(&buf, label),
            "narrow list dropped {label:?}:\n{}",
            text(&buf)
        );
    }
}

#[test]
fn narrow_scrolls_to_selected() {
    let mut agents = vec![agent("t0", "ceo", 0)];
    for i in 1..15 {
        let label = format!("worker-{i}");
        agents.push(agent(&format!("t{i}"), &label, 1));
    }
    let data = data(agents);
    let state = selected_state("t14");
    let buf = render((70, 20), &state, &data, false);
    assert!(
        has(&buf, "worker-14 claude-code/opus"),
        "selected agent is not scrolled into view as a list row:\n{}",
        text(&buf)
    );
}

#[test]
fn narrow_nothing_lost_at_70x24() {
    let mut agents = vec![agent("t0", "ceo", 0)];
    for i in 1..=3 {
        let label = format!("worker-{i}");
        agents.push(agent(&format!("t{i}"), &label, 1));
    }
    let data = data(agents);
    let state = selected_state("t0");
    let buf = render((70, 24), &state, &data, false);
    for label in ["ceo", "worker-1", "worker-2", "worker-3"] {
        assert!(has(&buf, label), "missing {label:?}:\n{}", text(&buf));
    }
}

#[test]
fn inbox_scrolls_to_selected_item() {
    let mut data = data(vec![agent("t1", "alpha", 0)]);
    data.view.as_mut().unwrap().inbox = (1..=8)
        .map(|id| InboxRow {
            id,
            ts_ms: NOW,
            terminal: Some("t9".into()),
            source: "blocked".into(),
            kind: Some("question".into()),
            text: format!("item {id}"),
            resolved_ms: None,
            reply: None,
        })
        .collect();
    let mut state = selected_state("t1");
    state.panel = agmag::board::state::Panel::Inbox;
    state.sel_inbox = Some(8);
    let buf = render((140, 36), &state, &data, false);
    assert!(
        has(&buf, "#8"),
        "selected item is off screen:\n{}",
        text(&buf)
    );
    assert!(
        has(&buf, "▸ #8"),
        "selected marker is not on the visible row:\n{}",
        text(&buf)
    );
}

#[test]
fn projects_scroll_to_selected() {
    let mut data = data(vec![agent("t1", "alpha", 0)]);
    data.projects = (0..12)
        .map(|i| ProjectSummary {
            key: ProjectKey {
                root: format!("D:/p/p{i}"),
                name: format!("p{i}"),
            },
            live_agents: 0,
            open_inbox: 0,
        })
        .collect();
    let mut state = selected_state("t1");
    state.panel = agmag::board::state::Panel::Projects;
    state.sel_project = 11;
    let buf = render((140, 36), &state, &data, false);
    assert!(
        has(&buf, "p11"),
        "selected project is off screen:\n{}",
        text(&buf)
    );
    assert!(
        has(&buf, "▸ p11"),
        "selected marker is not on the visible row:\n{}",
        text(&buf)
    );
}

#[test]
fn pick_project_scrolls_to_selected() {
    let mut data = data(vec![agent("t1", "alpha", 0)]);
    data.projects = (0..30)
        .map(|i| ProjectSummary {
            key: ProjectKey {
                root: format!("D:/p/p{i}"),
                name: format!("p{i}"),
            },
            live_agents: 0,
            open_inbox: 0,
        })
        .collect();
    let mut state = selected_state("t1");
    state.mode = Mode::PickProject { idx: 29 };
    let buf = render((140, 40), &state, &data, false);
    assert!(
        has(&buf, "p29"),
        "selected project is off screen:\n{}",
        text(&buf)
    );
    assert!(
        has(&buf, "▸ p29"),
        "selected marker is not on the visible row:\n{}",
        text(&buf)
    );
}

#[test]
fn pick_project_long_names_do_not_wrap() {
    let mut data = data(vec![agent("t1", "alpha", 0)]);
    data.projects = (0..30)
        .map(|i| ProjectSummary {
            key: ProjectKey {
                root: format!("D:/p/p{i}"),
                name: format!("p{i}-{}", "x".repeat(80)),
            },
            live_agents: 0,
            open_inbox: 0,
        })
        .collect();
    let mut state = selected_state("t1");
    state.mode = Mode::PickProject { idx: 29 };
    let buf = render((140, 40), &state, &data, false);
    assert!(
        has(&buf, "▸ p29-xxx"),
        "selected long project name is off screen:
{}",
        text(&buf)
    );
}

#[test]
fn selected_card_uses_selected_color() {
    let th = theme(ThemeName::CatppuccinMocha);
    let data = data(vec![agent("t1", "alpha", 0), agent("t2", "beta", 0)]);
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, false);
    let (label_x, label_y) = find_in(&buf, "alpha", 20, 106);
    let mut border_x = None;
    for x in (0..label_x).rev() {
        let symbol = buf[(x, label_y)].symbol();
        if symbol == "│" || symbol == "|" {
            border_x = Some(x);
            break;
        }
    }
    let border_x = border_x.expect("selected card border not found");
    assert_eq!(
        buf[(border_x, label_y)].fg,
        th.selected,
        "selected card border is not theme.selected:\n{}",
        text(&buf)
    );
}

#[test]
fn unknown_values_render_dash() {
    let mut a = agent("t1", "t1", 0);
    a.app.clear();
    a.model.clear();
    a.effort.clear();
    a.status = "-".into();
    a.state = "exited".into();
    let data = data(vec![a]);
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, false);
    for field in [
        "app: -",
        "model: -",
        "status: -",
        "task: -",
        "stage: -",
        "ctx: -",
        "tokens: -",
        "doing: -",
        "last: -",
    ] {
        assert!(has(&buf, field), "missing {field:?}:\n{}", text(&buf));
    }
}

#[test]
fn hostile_text_is_inert_and_clipped() {
    let a = agent("t1", "\x1b]0;pwn\x07evil\nname", 0);
    let mut data = data(vec![a]);
    data.conversation = vec![MessageRow {
        id: 1,
        ts_ms: NOW,
        from: "t1".into(),
        to: "t2".into(),
        kind: "GIAO".into(),
        text: "漢".repeat(500),
        ..MessageRow::default()
    }];
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, false);

    let all = text(&buf);
    assert!(!all.contains('\x1b'), "escape byte reached the buffer");
    assert!(
        !all.contains("pwn"),
        "OSC payload reached the buffer:\n{all}"
    );

    // The label lives in the middle column (wide: 20..106).
    let (evil_x, _) = find_in(&buf, "evil", 20, 106);
    assert!(
        (20..106).contains(&evil_x),
        "label leaked outside the orchestration column at x={evil_x}"
    );

    // Every CJK cell of the long message stays inside the right column (106..140).
    let mut seen = false;
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            if buf[(x, y)].symbol() == "漢" {
                seen = true;
                assert!(
                    x >= 106,
                    "message text leaked outside its panel at x={x}, y={y}"
                );
            }
        }
    }
    assert!(seen, "message text was not rendered at all");
}

#[test]
fn live_edge_accent_stale_edge_muted() {
    let th = theme(ThemeName::CatppuccinMocha);
    let mut data = data(vec![
        agent("t1", "alpha", 0),
        agent("t2", "beta", 1),
        agent("t3", "gamma", 1),
    ]);
    data.edges = vec![
        Edge {
            parent: 0,
            child: 1,
            kind: "ASSIGN".into(),
            ts_ms: NOW - 1_000,
        },
        Edge {
            parent: 0,
            child: 2,
            kind: "QUESTION".into(),
            ts_ms: NOW - 7_200_000,
        },
    ];
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, false);
    assert_eq!(
        fg_of(&buf, "ASSIGN"),
        th.accent,
        "live edge is not accent:\n{}",
        text(&buf)
    );
    assert_eq!(
        fg_of(&buf, "QUESTION"),
        th.muted,
        "stale edge is not muted:\n{}",
        text(&buf)
    );
}

#[test]
fn ascii_mode_has_no_box_drawing() {
    let data = data(vec![agent("t1", "alpha", 0)]);
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, true);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let symbol = buf[(x, y)].symbol();
            for c in symbol.chars() {
                assert!(
                    !('\u{2500}'..='\u{257f}').contains(&c),
                    "box drawing {c:?} at ({x},{y}) in ASCII mode"
                );
            }
        }
    }
    assert!(has(&buf, "+"), "ASCII borders missing:\n{}", text(&buf));
}

#[test]
fn empty_states_no_panic() {
    let state = BoardState::new(None, ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &BoardData::default(), false);
    assert!(has(&buf, "Projects"), "{}", text(&buf));

    let mut data = data(vec![]);
    data.view = Some(ProjectView {
        key: key(),
        agents: vec![],
        inbox: vec![],
        messages: vec![],
    });
    let state = BoardState::new(Some(key()), ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &data, false);
    assert!(has(&buf, "orchestration"), "{}", text(&buf));
    assert!(has(&buf, "(no agents)"), "{}", text(&buf));
}

#[test]
fn tiny_terminal_no_panic() {
    let data = data(vec![agent("t1", "alpha", 0)]);
    let state = selected_state("t1");
    for size in [(20, 6), (1, 1)] {
        let _ = render(size, &state, &data, false);
        let _ = render(size, &state, &data, true);
    }
}

#[test]
fn popup_reply_visible() {
    let data = data(vec![agent("t1", "alpha", 0)]);
    let mut state = selected_state("t1");
    state.mode = Mode::Reply {
        id: 42,
        input: "hello there".into(),
    };
    let buf = render((140, 40), &state, &data, false);
    assert!(has(&buf, "Reply"), "{}", text(&buf));
    assert!(has(&buf, "#42"), "{}", text(&buf));
    assert!(has(&buf, "hello there"), "{}", text(&buf));
}

fn stage_buffer(stage: Option<&str>) -> Buffer {
    let mut a = agent("t1", "alpha", 0);
    a.stage = stage.map(str::to_string);
    let data = data(vec![a]);
    let state = selected_state("t1");
    render((140, 40), &state, &data, false)
}

#[test]
fn stage_pipeline_mapping() {
    let th = theme(ThemeName::CatppuccinMocha);

    let buf = stage_buffer(Some("wip"));
    assert_eq!(fg_in(&buf, "wip", 106, 140), th.selected);
    assert_eq!(fg_in(&buf, "ready", 106, 140), th.muted);

    let buf = stage_buffer(Some("ready"));
    assert_eq!(fg_in(&buf, "ready", 106, 140), th.selected);

    let buf = stage_buffer(Some("merging"));
    assert_eq!(fg_in(&buf, "merging", 106, 140), th.selected);

    let buf = stage_buffer(Some("merged"));
    assert_eq!(fg_in(&buf, "merged", 106, 140), th.selected);

    let buf = stage_buffer(Some("deployed"));
    assert_eq!(fg_in(&buf, "merged", 106, 140), th.ok);
    assert_eq!(fg_in(&buf, "deployed", 106, 140), th.ok);

    let buf = stage_buffer(Some("blocked"));
    assert_eq!(fg_in(&buf, "wip", 106, 140), th.warn);
    assert_eq!(fg_in(&buf, "blocked", 106, 140), th.warn);

    let buf = stage_buffer(Some("failed"));
    assert_eq!(fg_in(&buf, "failed", 106, 140), th.error);
    assert_eq!(fg_in(&buf, "wip", 106, 140), th.muted);

    let buf = stage_buffer(Some("custom-lane"));
    assert!(has(&buf, "custom-lane"), "{}", text(&buf));
    assert!(!has(&buf, "merging"));

    let buf = stage_buffer(None);
    assert!(has(&buf, "stage: -"), "{}", text(&buf));
}

/// (left border x, right border x, label row) of the card whose line 1 holds `needle`.
fn card_centre(left: u16, right: u16) -> u16 {
    left + (right - left).div_ceil(2)
}

fn card_bounds(buf: &Buffer, needle: &str) -> (u16, u16, u16) {
    // Wide layout: the orchestration column is 20..106. Restricting the search skips
    // the detail panel title, which can hold the same label.
    let (x, y) = find_in(buf, needle, 20, 106);
    let mut left = x;
    while left > 0 {
        let symbol = buf[(left - 1, y)].symbol();
        if symbol == "│" || symbol == "|" {
            break;
        }
        left -= 1;
    }
    let mut right = x;
    while right + 1 < buf.area.width {
        let symbol = buf[(right + 1, y)].symbol();
        if symbol == "│" || symbol == "|" {
            break;
        }
        right += 1;
    }
    (left - 1, right + 1, y)
}

#[test]
fn cards_grow_to_fill_width() {
    let mut parent = agent("t7p", "agm-ceo", 0);
    parent.app = "claude".into();
    parent.model = "opus".into();
    let mut children = Vec::new();
    for address in ["t9a", "t9b", "t9c", "t9d"] {
        let mut child = agent(address, address, 1);
        child.app = "opencode".into();
        child.model = "m".into();
        child.doing = Some(TurnRow {
            ts_ms: NOW - 60_000,
            role: "tool".into(),
            summary: "cargo test".into(),
        });
        children.push(child);
    }
    let mut agents = vec![parent];
    agents.extend(children);
    let data = data(agents);
    let state = selected_state("t7p");
    let buf = render((140, 36), &state, &data, false);
    assert!(
        has(&buf, "agm-ceo claude"),
        "card line 1 is truncated:\n{}",
        text(&buf)
    );
    assert!(
        has(&buf, "doing: cargo test"),
        "card line 2 is truncated:\n{}",
        text(&buf)
    );
}

#[test]
fn wrapped_children_keep_edge_labels() {
    let mut agents = vec![agent("t7p", "agm-ceo", 0)];
    let mut edges = Vec::new();
    for i in 1..=6 {
        agents.push(agent(&format!("t9{i}"), &format!("worker-{i}"), 1));
        edges.push(Edge {
            parent: 0,
            child: i,
            kind: format!("KIND-{i}"),
            ts_ms: NOW - 60_000,
        });
    }
    let mut data = data(agents);
    data.edges = edges;
    let state = selected_state("t7p");
    let buf = render((100, 30), &state, &data, false);
    for i in 1..=6 {
        assert!(
            has(&buf, &format!("KIND-{i}")),
            "edge label KIND-{i} lost on a wrapped child row:\n{}",
            text(&buf)
        );
    }
}

#[test]
fn connector_joins_parent_and_children() {
    let mut data = data(vec![
        agent("t7p", "agm-ceo", 0),
        agent("t9a", "worker-a", 1),
        agent("t9b", "worker-b", 1),
    ]);
    data.edges = vec![Edge {
        parent: 0,
        child: 1,
        kind: "REPORT".into(),
        ts_ms: NOW - 60_000,
    }];
    let state = selected_state("t7p");
    let buf = render((140, 36), &state, &data, false);

    let (pl, pr, py) = card_bounds(&buf, "agm-ceo");
    let parent_cx = card_centre(pl, pr);
    let parent_bottom = py + 2;
    assert_eq!(
        buf[(parent_cx, parent_bottom)].symbol(),
        "┬",
        "no connector stem under the parent card:\n{}",
        text(&buf)
    );

    let bus_row = parent_bottom + 1;
    assert!(
        matches!(buf[(parent_cx, bus_row)].symbol(), "┴" | "┼"),
        "stem does not meet the bus:\n{}",
        text(&buf)
    );
    assert!(
        row_text(&buf, bus_row).contains('─'),
        "no horizontal bus:\n{}",
        text(&buf)
    );
    assert!(
        row_text(&buf, bus_row + 1).contains("REPORT"),
        "edge label missing from the drop line:\n{}",
        text(&buf)
    );

    for label in ["worker-a", "worker-b"] {
        let (cl, cr, cy) = card_bounds(&buf, label);
        let cx = card_centre(cl, cr);
        assert_eq!(
            buf[(cx, cy - 1)].symbol(),
            "┴",
            "no drop mark on {label}'s top border:\n{}",
            text(&buf)
        );
        assert_eq!(
            buf[(cx, cy - 2)].symbol(),
            "│",
            "no drop line above {label}:\n{}",
            text(&buf)
        );
    }
}

#[test]
fn connector_ascii_uses_plus_pipe_dash() {
    let mut data = data(vec![
        agent("t7p", "agm-ceo", 0),
        agent("t9a", "worker-a", 1),
        agent("t9b", "worker-b", 1),
    ]);
    data.edges = vec![Edge {
        parent: 0,
        child: 1,
        kind: "REPORT".into(),
        ts_ms: NOW - 60_000,
    }];
    let state = selected_state("t7p");
    let buf = render((140, 36), &state, &data, true);

    let (pl, pr, py) = card_bounds(&buf, "agm-ceo");
    let parent_cx = card_centre(pl, pr);
    let parent_bottom = py + 2;
    assert_eq!(buf[(parent_cx, parent_bottom)].symbol(), "+");
    let bus_row = parent_bottom + 1;
    assert!(
        row_text(&buf, bus_row).contains('-'),
        "no ASCII bus:\n{}",
        text(&buf)
    );
    for label in ["worker-a", "worker-b"] {
        let (cl, cr, cy) = card_bounds(&buf, label);
        let cx = card_centre(cl, cr);
        assert_eq!(buf[(cx, cy - 1)].symbol(), "+");
        assert_eq!(buf[(cx, cy - 2)].symbol(), "|");
    }
    assert!(!has(&buf, "─") && !has(&buf, "┬") && !has(&buf, "┴"));
}

#[test]
fn inbox_kind_is_english() {
    let mut data = data(vec![agent("t1", "alpha", 0)]);
    data.view.as_mut().unwrap().inbox = vec![InboxRow {
        id: 43,
        ts_ms: NOW,
        terminal: Some("t9".into()),
        source: "blocked".into(),
        kind: Some("HỎI".into()),
        text: "needs a decision".into(),
        resolved_ms: None,
        reply: None,
    }];
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, false);
    assert!(has(&buf, "QUESTION"), "{}", text(&buf));
    assert!(!has(&buf, "HỎI"), "stored code leaked:\n{}", text(&buf));
}

#[test]
fn messages_use_agent_labels() {
    let mut data = data(vec![agent("t1", "alpha", 0), agent("t2", "beta", 0)]);
    data.conversation = vec![
        MessageRow {
            id: 1,
            ts_ms: NOW,
            from: "t1".into(),
            to: "t2".into(),
            kind: "GIAO".into(),
            text: "hello".into(),
            ..MessageRow::default()
        },
        MessageRow {
            id: 2,
            ts_ms: NOW,
            from: "zz".into(),
            to: "t1".into(),
            kind: "HỎI".into(),
            text: "who?".into(),
            ..MessageRow::default()
        },
    ];
    let state = selected_state("t1");
    let buf = render((140, 40), &state, &data, false);
    assert!(has(&buf, "alpha → beta"), "{}", text(&buf));
    assert!(
        !has(&buf, "t1 → t2"),
        "raw addresses shown:\n{}",
        text(&buf)
    );
    assert!(
        has(&buf, "zz → alpha"),
        "unknown address changed:\n{}",
        text(&buf)
    );
}

#[test]
fn inbox_rows_and_selected_row() {
    let th = theme(ThemeName::CatppuccinMocha);
    let mut data = data(vec![agent("t1", "alpha", 0)]);
    data.view.as_mut().unwrap().inbox = vec![InboxRow {
        id: 43,
        ts_ms: NOW,
        terminal: Some("t9".into()),
        source: "blocked".into(),
        kind: Some("question".into()),
        text: "needs a decision".into(),
        resolved_ms: None,
        reply: None,
    }];
    let mut state = selected_state("t1");
    state.sel_inbox = Some(43);
    let buf = render((140, 40), &state, &data, false);
    assert!(has(&buf, "#43"), "{}", text(&buf));
    assert!(has(&buf, "needs a decision"));
    let (x, y) = find(&buf, "#43");
    assert_eq!(buf[(x, y)].fg, th.selected);
}
