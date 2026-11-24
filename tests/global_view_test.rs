//! Buffer tests for the global screen rendering (`src/board/global_view.rs`).

use std::collections::HashMap;

use agmag::board::global::{
    BorderId, Focus, GlobalData, GlobalState, Hit, HitMap, HitPanel, Popup, RowTarget, hit,
};
use agmag::board::global_view::draw_global;
use agmag::board::theme::{ThemeName, theme};
use agmag::config::Config;
use agmag::dashboard::{Section, Snapshot, TermView, snapshot};
use agmag::report::QuotaRow;
use agmag::store::{InboxRow, MessageRow, Store};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

const NOW: i64 = 1_000_000;

fn empty_snap() -> Snapshot {
    let store = Store::open_in_memory().unwrap();
    let mut snap = snapshot(&store, &Config::builtin(), NOW).unwrap();
    snap.quota.clear();
    snap.terminals.clear();
    snap.inbox.clear();
    snap.messages.clear();
    snap
}

fn data(view: &[Section]) -> GlobalData {
    GlobalData {
        snap: Some(empty_snap()),
        view: view.to_vec(),
        project_of: HashMap::new(),
        output: None,
        now_ms: NOW,
    }
}

fn render(size: (u16, u16), state: &GlobalState, data: &GlobalData, ascii: bool) -> Buffer {
    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).unwrap();
    let th = theme(state.theme);
    terminal
        .draw(|f| draw_global(f, state, data, &th, ascii))
        .unwrap();
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

fn has(buf: &Buffer, needle: &str) -> bool {
    (0..buf.area.height).any(|y| row_text(buf, y).contains(needle))
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

fn fg_of(buf: &Buffer, needle: &str) -> Color {
    let (x, y) = find(buf, needle);
    buf[(x, y)].fg
}

/// Left edge, top edge, width and height of the panel whose title contains `title`.
fn panel_rect(buf: &Buffer, title: &str) -> (u16, u16, u16, u16) {
    let (tx, top) = find(buf, title);
    let is_corner = |symbol: &str, upper: bool| {
        if upper {
            matches!(symbol, "╭" | "╮" | "+")
        } else {
            matches!(symbol, "╰" | "╯" | "+")
        }
    };
    let mut left = tx;
    while left > 0 && !is_corner(buf[(left, top)].symbol(), true) {
        left -= 1;
    }
    assert!(is_corner(buf[(left, top)].symbol(), true), "no left corner");
    let mut right = tx;
    while right + 1 < buf.area.width && !is_corner(buf[(right, top)].symbol(), true) {
        right += 1;
    }
    assert!(
        is_corner(buf[(right, top)].symbol(), true),
        "no right corner for {title:?}:\n{}",
        text(buf)
    );
    let mut bottom = top + 1;
    while bottom < buf.area.height && !is_corner(buf[(left, bottom)].symbol(), false) {
        bottom += 1;
    }
    assert!(
        is_corner(buf[(left, bottom)].symbol(), false),
        "no bottom corner for {title:?}:\n{}",
        text(buf)
    );
    (left, top, right - left + 1, bottom - top + 1)
}

/// Rows of a panel's inner area that contain nothing but spaces.
fn blank_inner_rows(buf: &Buffer, title: &str) -> u16 {
    let (left, top, width, height) = panel_rect(buf, title);
    let mut blanks = 0;
    for y in top + 1..top + height - 1 {
        let inner: String = row_text(buf, y)
            .chars()
            .skip(left as usize + 1)
            .take(width.saturating_sub(2) as usize)
            .collect();
        if inner.trim().is_empty() {
            blanks += 1;
        }
    }
    blanks
}

fn quota(provider: &str, window_minutes: u32, left: f64) -> QuotaRow {
    QuotaRow {
        provider: provider.into(),
        window_minutes,
        used_percent: 100.0 - left,
        left_percent: left,
        resets_at_s: NOW / 1000 + 3_600,
        observed_ms: NOW,
        reset_passed: false,
        stale: false,
        observed_age_s: 0,
        limited: false,
    }
}

fn empty_terminal(address: &str, name: &str) -> TermView {
    TermView {
        address: address.into(),
        name: name.into(),
        app: String::new(),
        status: String::new(),
        task: String::new(),
        model: String::new(),
        effort: String::new(),
        cwd: String::new(),
        place: String::new(),
        last_msg: None,
        tokens: None,
        context_pct: None,
        handoff_state: String::new(),
    }
}

fn terminal(address: &str, name: &str) -> TermView {
    TermView {
        address: address.into(),
        name: name.into(),
        app: "claude".into(),
        status: "working".into(),
        task: "build the app".into(),
        model: "opus".into(),
        effort: "high".into(),
        cwd: "D:/p/agmag".into(),
        place: "agmag / agm-ceo".into(),
        last_msg: Some(("REPORT".into(), true)),
        tokens: Some(12_400),
        context_pct: Some(42.0),
        handoff_state: "active".into(),
    }
}

#[test]
fn default_view_titles() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.terminals.push(terminal("t7p", "agm-ceo"));
    snap.inbox.push(InboxRow {
        id: 43,
        ts_ms: NOW - 60_000,
        terminal: Some("t7p".into()),
        ..InboxRow::default()
    });
    snap.messages.push(MessageRow {
        id: 1,
        ts_ms: NOW - 60_000,
        from: "t7p".into(),
        to: "t9b".into(),
        kind: "BÁO CÁO".into(),
        text: "tests red→green".into(),
        ..MessageRow::default()
    });
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    for title in ["Quota", "Terminals", "Inbox", "Messages"] {
        assert!(has(&buf, title), "missing title {title:?}:\n{}", text(&buf));
    }
}

#[test]
fn quota_gauge_colours() {
    let mut d = data(&[Section::Quota]);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.quota.push(quota("codex", 10_080, 30.0));
    snap.quota.push(quota("opencode-go", 43_200, 10.0));
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    let th = theme(ThemeName::CatppuccinMocha);
    assert_eq!(fg_of(&buf, "80% left"), th.ok);
    assert_eq!(fg_of(&buf, "30% left"), th.warn);
    assert_eq!(fg_of(&buf, "10% left"), th.error);
}

#[test]
fn terminals_selected_row_visible() {
    let mut d = data(&[Section::Terminals]);
    let snap = d.snap.as_mut().unwrap();
    for i in 0..45 {
        snap.terminals
            .push(terminal(&format!("t{i}"), &format!("worker-{i}")));
    }
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t44".into());
    let buf = render((140, 40), &state, &d, false);
    assert!(
        has(&buf, "worker-44"),
        "selected terminal scrolled out:\n{}",
        text(&buf)
    );
}

#[test]
fn inbox_kind_english_and_selected() {
    let mut d = data(&[Section::Inbox]);
    let snap = d.snap.as_mut().unwrap();
    snap.inbox.push(InboxRow {
        id: 43,
        ts_ms: NOW - 300_000,
        terminal: Some("t9c".into()),
        source: "worker".into(),
        kind: Some("HỎI".into()),
        text: "needs a decision: keep API v1?".into(),
        ..InboxRow::default()
    });
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_inbox = Some(43);
    let buf = render((140, 40), &state, &d, false);
    assert!(
        has(&buf, "QUESTION"),
        "kind not translated:\n{}",
        text(&buf)
    );
    assert!(
        !has(&buf, "HỎI"),
        "stored kind shown instead of English:\n{}",
        text(&buf)
    );
    assert!(has(&buf, "needs a decision"), "{}", text(&buf));
    assert!(has(&buf, "▸"), "selected marker missing:\n{}", text(&buf));
}

#[test]
fn unknown_values_dash() {
    let mut d = data(&[Section::Terminals]);
    let snap = d.snap.as_mut().unwrap();
    snap.terminals.push(empty_terminal("t1", "mystery"));
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t1".into());
    let buf = render((140, 40), &state, &d, false);
    let (_, y) = find(&buf, "mystery");
    let line = row_text(&buf, y);
    assert!(line.contains('-'), "unknown values not dashed: {line:?}");
    assert!(
        !line.contains("0%") && !line.contains('?'),
        "unknown values invented a number: {line:?}"
    );
}

#[test]
fn hostile_text_inert() {
    let hostile = format!("\x1b[31mEVIL\x07\x1b]0;pwn\x07\nline2{}", "毒".repeat(500));
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.terminals.push(TermView {
        name: hostile.clone(),
        ..terminal("t1", "")
    });
    snap.messages.push(MessageRow {
        id: 1,
        ts_ms: NOW,
        from: "t1".into(),
        to: "t2".into(),
        kind: "BÁO CÁO".into(),
        text: hostile.clone(),
        ..MessageRow::default()
    });
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((100, 30), &state, &d, false);
    let all = text(&buf);
    assert!(!all.contains('\x1b'), "escape sequence reached the screen");
    assert!(!all.contains('\x07'), "bell reached the screen");
    assert!(!all.contains("pwn"), "OSC payload reached the screen");
    let cjk = all.chars().filter(|c| *c == '毒').count();
    assert!(
        cjk > 0 && cjk < 1_000,
        "hostile text not clipped to its panels: {cjk} cells"
    );
    for y in 0..buf.area.height {
        let per_row = row_text(&buf, y).chars().filter(|c| *c == '毒').count();
        assert!(
            per_row <= buf.area.width as usize,
            "row {y} overflowed the screen with {per_row} cells"
        );
    }
    for y in buf.area.height.saturating_sub(2)..buf.area.height {
        let line = row_text(&buf, y);
        assert!(!line.contains('毒'), "hostile text leaked into the bars");
    }
}

#[test]
fn output_panel_overlays_and_scrolls() {
    let mut d = data(&[Section::Quota]);
    d.output = Some(
        (0..40)
            .map(|i| format!("out-{i:02}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.output_scroll = 5;
    let buf = render((140, 40), &state, &d, false);
    assert!(has(&buf, "Output"), "missing Output title:\n{}", text(&buf));
    assert!(has(&buf, "out-05"), "output not scrolled:\n{}", text(&buf));
    assert!(
        !has(&buf, "out-00"),
        "output scroll ignored:\n{}",
        text(&buf)
    );
}

#[test]
fn confirm_popup_visible() {
    let d = data(&Section::DEFAULT);
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.popup = Some(Popup::Confirm {
        prompt: "kill t1? (y confirms)".into(),
        input: "y".into(),
    });
    let buf = render((140, 40), &state, &d, false);
    assert!(has(&buf, "Confirm"), "missing popup title:\n{}", text(&buf));
    assert!(has(&buf, "kill t1?"), "{}", text(&buf));
    assert!(has(&buf, "y then"), "missing confirm hint:\n{}", text(&buf));
}

#[test]
fn cmdline_long_input_scrolls_cursor_visible() {
    let d = data(&Section::DEFAULT);
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.cmd.text = format!("{}Z", "a".repeat(199));
    state.cmd.cursor = state.cmd.text.chars().count();
    let buf = render((60, 20), &state, &d, false);
    let cmd_row = row_text(&buf, buf.area.height.saturating_sub(2));
    assert!(
        cmd_row.contains('Z'),
        "cursor char scrolled out of the command bar: {cmd_row:?}"
    );
    assert!(
        cmd_row.contains("agm>"),
        "prompt missing from the command bar: {cmd_row:?}"
    );
}

#[test]
fn cmdline_tiny_widths_no_panic() {
    let d = data(&Section::DEFAULT);
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.focus = Focus::CmdLine;
    state.cmd.text = "abcdefghij".into();
    state.cmd.cursor = state.cmd.text.chars().count();
    for width in 1..=8 {
        for height in 2..=4 {
            let _ = render((width, height), &state, &d, false);
        }
    }
}

#[test]
fn cmdline_draw_hostile_text_inert() {
    let d = data(&Section::DEFAULT);
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.cmd.text = "a\x1b[31mEVIL\x1b]0;pwn\x07b\nc".into();
    state.cmd.cursor = state.cmd.text.chars().count();
    let buf = render((80, 20), &state, &d, false);
    let all = text(&buf);
    assert!(!all.contains('\x1b'), "escape sequence reached the screen");
    assert!(!all.contains('\x07'), "bell reached the screen");
    assert!(!all.contains("pwn"), "OSC payload reached the screen");
}

#[test]
fn output_keeps_sgr_colour() {
    let mut d = data(&[Section::Quota]);
    d.output = Some("\x1b[1;32mok\x1b[0m plain".into());
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    assert!(has(&buf, "ok"), "output text missing:\n{}", text(&buf));
    assert_eq!(
        fg_of(&buf, "ok"),
        Color::Green,
        "the ANSI bridge's colour was dropped:\n{}",
        text(&buf)
    );
    assert_eq!(
        fg_of(&buf, "plain"),
        Color::Reset,
        "text after the reset must be default-coloured"
    );
}

#[test]
fn terminals_columns_fit_app_and_model() {
    let mut d = data(&[Section::Terminals]);
    let snap = d.snap.as_mut().unwrap();
    snap.terminals.push(TermView {
        app: "antigravity".into(),
        model: "claude-opus-5-5".into(),
        effort: String::new(),
        ..terminal("t1", "worker-1")
    });
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t1".into());
    let buf = render((150, 40), &state, &d, false);
    assert!(
        has(&buf, "antigravity"),
        "app column cut the app name:\n{}",
        text(&buf)
    );
    assert!(
        has(&buf, "claude-opus-5-5"),
        "model column cut the model name:\n{}",
        text(&buf)
    );
}

#[test]
fn messages_one_line_when_wide() {
    let mut d = data(&[Section::Inbox]);
    let snap = d.snap.as_mut().unwrap();
    for i in 0..6 {
        snap.messages.push(MessageRow {
            id: i + 1,
            ts_ms: NOW - 60_000,
            from: "t1".into(),
            to: "t2".into(),
            kind: "BÁO CÁO".into(),
            text: format!("distinct-message-text-{i}"),
            ..MessageRow::default()
        });
    }
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    let (_, header_y) = find(&buf, "REPORT");
    let (_, text_y) = find(&buf, "distinct-message-text-0");
    assert_eq!(
        header_y,
        text_y,
        "wide Messages must show one line per message:\n{}",
        text(&buf)
    );
    for i in 0..6 {
        assert!(
            has(&buf, &format!("distinct-message-text-{i}")),
            "six message rows must show six messages:\n{}",
            text(&buf)
        );
    }
}

#[test]
fn narrow_and_tiny_no_panic() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.terminals.push(terminal("t1", "worker-1"));
    snap.inbox.push(InboxRow {
        id: 1,
        ts_ms: NOW,
        terminal: Some("t1".into()),
        kind: Some("HỎI".into()),
        text: "hello".into(),
        ..InboxRow::default()
    });
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    for size in [(70, 20), (30, 10), (1, 1)] {
        let _ = render(size, &state, &d, false);
    }
}

#[test]
fn ascii_mode_no_box_drawing() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.terminals.push(terminal("t1", "worker-1"));
    snap.inbox.push(InboxRow {
        id: 1,
        ts_ms: NOW,
        terminal: Some("t1".into()),
        kind: Some("HỎI".into()),
        text: "hello".into(),
        ..InboxRow::default()
    });
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t1".into());
    let buf = render((140, 40), &state, &d, true);
    let all = text(&buf);
    for ch in "╭╮╰╯│─┌┐└┘├┤┬┴┼▸●✓✗→⏎█░◐".chars() {
        assert!(
            !all.contains(ch),
            "unicode glyph {ch:?} in ascii mode:\n{all}"
        );
    }
}

#[test]
fn bridged_sections_render_titles_and_body() {
    let d = data(&[Section::Usage, Section::Budgets, Section::System]);
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    for title in ["Usage", "Budgets", "System"] {
        assert!(has(&buf, title), "missing title {title:?}:\n{}", text(&buf));
    }
}

#[test]
fn cmdline_short_input_shows_whole_text() {
    let d = data(&Section::DEFAULT);
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.cmd.text = "ls".into();
    state.cmd.cursor = 2;
    let buf = render((80, 24), &state, &d, false);
    let cmd_row = row_text(&buf, buf.area.height.saturating_sub(2));
    assert!(
        cmd_row.contains("agm> ls"),
        "short command cut: {cmd_row:?}"
    );
}

#[test]
fn terminals_task_sits_under_task_header() {
    let mut d = data(&[Section::Terminals]);
    let snap = d.snap.as_mut().unwrap();
    snap.terminals.push(TermView {
        task: "TASK-XYZ".into(),
        tokens: Some(1234),
        ..terminal("t1", "worker-1")
    });
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t1".into());
    let buf = render((220, 30), &state, &d, false);
    let char_pos = |row: &str, needle: &str| row.find(needle).map(|b| row[..b].chars().count());
    let rows: Vec<String> = (0..buf.area.height).map(|y| row_text(&buf, y)).collect();
    let header = rows
        .iter()
        .find(|r| r.contains(" task"))
        .expect("task header");
    let data_row = rows
        .iter()
        .find(|r| r.contains("TASK-XYZ"))
        .expect("task cell");
    assert_eq!(
        char_pos(header, " task").map(|p| p + 1),
        char_pos(data_row, "TASK-XYZ"),
        "task value not under its header:\n{header}\n{data_row}"
    );
}

#[test]
fn quota_panel_fits_content_not_half() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    let mut q = quota("opencode-go", 43_200, 49.0);
    q.resets_at_s = NOW / 1000 + 626 * 3_600 + 24 * 60;
    snap.quota.push(q);
    snap.terminals.push(terminal("t1", "worker-1"));
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((150, 40), &state, &d, false);
    let top = row_text(&buf, 0);
    let (term_x, _) = find(&buf, "Terminals");
    assert!(
        term_x < 75,
        "Quota should be narrower than 75 cols (Terminals started at {term_x}):\n{top}"
    );
    assert!(
        has(&buf, "opencode-go  mo ") && has(&buf, "49% left resets in 626h24m"),
        "quota row was cut:\n{}",
        text(&buf)
    );
}

#[test]
fn terminals_gets_remaining_width() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.terminals.push(terminal("t1", "worker-1"));
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t1".into());
    let buf = render((150, 40), &state, &d, false);
    let top = row_text(&buf, 0);
    let quota_end = top.find("╮").map(|b| top[..b].chars().count()).unwrap();
    let term_start = top
        .match_indices("╭")
        .nth(1)
        .map(|(b, _)| top[..b].chars().count())
        .unwrap();
    assert_eq!(
        term_start,
        quota_end + 1,
        "Terminals must start right after Quota:\n{top}"
    );
    assert!(
        150 - term_start > 75,
        "Terminals should get more than half the width:\n{top}"
    );
    // Whole header tokens of the Terminals table (a substring check would let `last` satisfy `st`).
    let header = (0..buf.area.height)
        .map(|y| row_text(&buf, y))
        .find(|row| row.contains(" name ") && row.contains(" model "))
        .expect("terminals header row");
    let tokens: Vec<&str> = header
        .split(|c: char| c.is_whitespace() || c == '│')
        .filter(|t| !t.is_empty())
        .collect();
    for hdr in ["name", "app", "model", "st", "ctx", "tok", "task"] {
        assert!(
            tokens.contains(&hdr),
            "missing short header {hdr:?}: {header:?}"
        );
    }
    assert!(!tokens.contains(&"status"), "long header left: {header:?}");
}

/// Eleven agents, no inbox: the panels follow their content and Messages absorbs the rest.
fn many_terminals_no_inbox() -> GlobalData {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    for i in 0..12 {
        snap.quota.push(quota(&format!("provider-{i}"), 300, 80.0));
    }
    for i in 0..11 {
        snap.terminals
            .push(terminal(&format!("t{i}"), &format!("worker-{i}")));
    }
    d
}

#[test]
fn wide_overview_fits_content_and_inbox_is_three_rows() {
    let d = many_terminals_no_inbox();
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    for size in [(140, 40), (200, 50)] {
        let buf = render(size, &state, &d, false);
        let (_, _, _, inbox_h) = panel_rect(&buf, "Inbox (0)");
        assert_eq!(inbox_h, 3, "empty Inbox at {size:?}:\n{}", text(&buf));
        let (_, _, _, terms_h) = panel_rect(&buf, "Terminals");
        assert_eq!(terms_h, 14, "header + 11 rows + 2 borders at {size:?}");
        let (_, _, _, messages_h) = panel_rect(&buf, "Messages");
        assert!(
            messages_h > 8,
            "Messages must take the surplus instead of a fixed 8 rows: {messages_h}"
        );
        for title in ["Quota", "Terminals", "Inbox"] {
            let blanks = blank_inner_rows(&buf, title);
            assert!(
                blanks <= 1,
                "{title} has {blanks} blank inner rows at {size:?}:\n{}",
                text(&buf)
            );
        }
    }
}

fn shell(address: &str, name: &str, app: &str) -> TermView {
    TermView {
        app: app.into(),
        model: String::new(),
        effort: String::new(),
        ..terminal(address, name)
    }
}

#[test]
fn terminals_title_counts_agents_and_shells() {
    let mut d = data(&[Section::Terminals]);
    for i in 0..8 {
        d.snap
            .as_mut()
            .unwrap()
            .terminals
            .push(terminal(&format!("a{i}"), &format!("agent-{i}")));
    }
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    assert!(has(&buf, "Terminals (8 agents)"), "{}", text(&buf));

    d.snap
        .as_mut()
        .unwrap()
        .terminals
        .push(shell("s1", "shell-1", "bash"));
    d.snap
        .as_mut()
        .unwrap()
        .terminals
        .push(shell("s2", "shell-2", "pwsh"));
    let buf = render((140, 40), &state, &d, false);
    assert!(
        has(&buf, "Terminals (8 agents · 2 shells)"),
        "{}",
        text(&buf)
    );

    d.snap
        .as_mut()
        .unwrap()
        .terminals
        .retain(|t| t.app == "bash" || t.app == "pwsh");
    let buf = render((140, 40), &state, &d, false);
    assert!(has(&buf, "Terminals (2 shells)"), "{}", text(&buf));
}

#[test]
fn terminals_hides_columns_that_are_all_dashes() {
    let mut d = data(&[Section::Terminals]);
    let snap = d.snap.as_mut().unwrap();
    for i in 0..3 {
        snap.terminals
            .push(empty_terminal(&format!("t{i}"), &format!("plain-{i}")));
    }
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    let header = (0..buf.area.height)
        .map(|y| row_text(&buf, y))
        .find(|row| row.contains("name"))
        .expect("terminals header row");
    let headers: Vec<&str> = header
        .split(|c: char| c.is_whitespace() || c == '│')
        .filter(|t| !t.is_empty())
        .collect();
    for hidden in ["app", "model", "st", "ctx", "tok", "last", "task"] {
        assert!(
            !headers.contains(&hidden),
            "all-dash column {hidden:?} still shown: {header:?}"
        );
    }
    assert!(headers.contains(&"name"), "name column is always shown");
}

#[test]
fn terminals_agents_first_and_shells_dimmed() {
    let mut d = data(&[Section::Terminals]);
    let snap = d.snap.as_mut().unwrap();
    snap.terminals.push(shell("s1", "shell-one", "bash"));
    snap.terminals.push(terminal("a1", "agent-one"));
    snap.terminals.push(shell("s2", "shell-two", "zsh"));
    snap.terminals.push(terminal("a2", "agent-two"));
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("s2".into());
    let buf = render((140, 40), &state, &d, false);
    let (_, agent1) = find(&buf, "agent-one");
    let (_, shell1) = find(&buf, "shell-one");
    let (_, agent2) = find(&buf, "agent-two");
    let (_, shell2) = find(&buf, "shell-two");
    assert!(
        agent1 < agent2 && agent2 < shell1 && shell1 < shell2,
        "visual order must be agents first, shells last: {agent1} {agent2} {shell1} {shell2}"
    );
    let th = theme(ThemeName::CatppuccinMocha);
    assert_eq!(fg_of(&buf, "shell-one"), th.muted, "shell row not dimmed");
    assert_ne!(fg_of(&buf, "agent-one"), th.muted, "agent row was dimmed");
}

#[test]
fn hit_map_records_what_the_overview_drew() {
    let mut d = data(&Section::DEFAULT);
    d.snap
        .as_mut()
        .unwrap()
        .terminals
        .push(terminal("t1", "worker-one"));
    d.snap.as_mut().unwrap().inbox.push(InboxRow {
        id: 7,
        ts_ms: NOW,
        ..InboxRow::default()
    });
    d.snap.as_mut().unwrap().messages.push(MessageRow {
        id: 1,
        ts_ms: NOW,
        from: "t1".into(),
        to: "t2".into(),
        kind: "BÁO CÁO".into(),
        text: "distinct-message-hit".into(),
        ..MessageRow::default()
    });
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    let map = state.hit_map();

    let (tx, ty) = find(&buf, "worker-one");
    assert_eq!(
        hit(&map, tx, ty),
        Some(Hit::Row(RowTarget::Terminal("t1".into()))),
        "terminal row not hit-testable:\n{}",
        text(&buf)
    );
    let (ix, iy) = find(&buf, "#7");
    assert_eq!(
        hit(&map, ix, iy),
        Some(Hit::Row(RowTarget::Inbox(7))),
        "inbox row not hit-testable:\n{}",
        text(&buf)
    );
    let (terms_left, terms_top, _, _) = panel_rect(&buf, "Terminals");
    assert_eq!(
        hit(&map, terms_left, terms_top),
        Some(Hit::Border(BorderId::TopPairVertical)),
        "top-pair divider not recorded"
    );
    let (_, inbox_top, _, _) = panel_rect(&buf, "Inbox (1)");
    assert!(
        matches!(
            hit(&map, 5, inbox_top),
            Some(Hit::Border(BorderId::Horizontal(_)))
        ),
        "horizontal boundary not recorded: {:?}",
        hit(&map, 5, inbox_top)
    );
    let (msg_x, msg_y) = find(&buf, "distinct-message-hit");
    assert_eq!(
        hit(&map, msg_x, msg_y),
        Some(Hit::Row(RowTarget::Message(0))),
        "message row not hit-testable:\n{}",
        text(&buf)
    );
    let (_, messages_top, _, _) = panel_rect(&buf, "Messages");
    assert_eq!(
        hit(&map, 5, messages_top + 5),
        Some(Hit::Panel(HitPanel::Section(Section::Messages))),
        "messages panel not recorded"
    );
    let sections: Vec<Section> = map
        .panels
        .iter()
        .filter_map(|(_, p)| match p {
            HitPanel::Section(s) => Some(*s),
            HitPanel::Output => None,
        })
        .collect();
    assert_eq!(
        sections,
        vec![
            Section::Quota,
            Section::Terminals,
            Section::Inbox,
            Section::Messages
        ]
    );
}

#[test]
fn hit_map_is_rebuilt_every_frame() {
    let mut d = data(&[Section::Terminals]);
    d.snap
        .as_mut()
        .unwrap()
        .terminals
        .push(terminal("t1", "worker-one"));
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let _ = render((140, 40), &state, &d, false);
    assert!(state.hit_map().rows.iter().any(|(_, target)| matches!(
        target,
        RowTarget::Terminal(address) if address == "t1"
    )));

    d.snap.as_mut().unwrap().terminals = vec![terminal("t2", "worker-two")];
    let _ = render((140, 40), &state, &d, false);
    let map: HitMap = state.hit_map();
    assert!(map.rows.iter().any(|(_, target)| matches!(
        target,
        RowTarget::Terminal(address) if address == "t2"
    )));
    assert!(
        !map.rows.iter().any(|(_, target)| matches!(
            target,
            RowTarget::Terminal(address) if address == "t1"
        )),
        "stale row survived the redraw"
    );
}

#[test]
fn narrow_overview_stacks_content_fitted_panels() {
    let mut d = many_terminals_no_inbox();
    d.view = vec![Section::Inbox, Section::Terminals];
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((100, 40), &state, &d, false);
    let (_, _, _, inbox_h) = panel_rect(&buf, "Inbox (0)");
    let (_, _, _, terms_h) = panel_rect(&buf, "Terminals");
    assert_eq!(inbox_h, 3, "{}", text(&buf));
    assert_eq!(terms_h, 14, "{}", text(&buf));
    for title in ["Inbox", "Terminals"] {
        assert!(
            blank_inner_rows(&buf, title) <= 1,
            "{title} has blank rows:\n{}",
            text(&buf)
        );
    }
}

fn rect_inside(outer: ratatui::layout::Rect, inner: ratatui::layout::Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x.saturating_add(inner.width) <= outer.x.saturating_add(outer.width)
        && inner.y.saturating_add(inner.height) <= outer.y.saturating_add(outer.height)
}

fn rects_overlap(a: ratatui::layout::Rect, b: ratatui::layout::Rect) -> bool {
    a.width > 0
        && b.width > 0
        && a.height > 0
        && b.height > 0
        && a.x < b.x.saturating_add(b.width)
        && b.x < a.x.saturating_add(a.width)
        && a.y < b.y.saturating_add(b.height)
        && b.y < a.y.saturating_add(a.height)
}

fn dense_overview_data() -> GlobalData {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.terminals.push(terminal("t1", "worker-1"));
    snap.inbox.push(InboxRow {
        id: 1,
        ts_ms: NOW,
        ..InboxRow::default()
    });
    snap.messages.push(MessageRow {
        id: 1,
        ts_ms: NOW,
        from: "t1".into(),
        to: "t2".into(),
        kind: "BÁO CÁO".into(),
        text: "a message".into(),
        ..MessageRow::default()
    });
    d
}

fn assert_everything_inside_main(size: (u16, u16), d: &GlobalData) {
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render(size, &state, d, false);
    let main = ratatui::layout::Rect::new(0, 0, size.0, size.1.saturating_sub(2));
    let map = state.hit_map();
    let all = map
        .panels
        .iter()
        .map(|(r, _)| *r)
        .chain(map.rows.iter().map(|(r, _)| *r))
        .chain(map.borders.iter().map(|(r, _)| *r));
    for rect in all {
        assert!(
            rect_inside(main, rect),
            "rect {rect:?} leaves main {main:?} at {size:?}:\n{}",
            text(&buf)
        );
    }
}

#[test]
fn every_rect_stays_inside_main_at_tiny_heights() {
    let d = dense_overview_data();
    for height in 1..=8 {
        assert_everything_inside_main((100, height), &d);
    }
    for height in 3..=10 {
        assert_everything_inside_main((140, height), &d);
    }
}

fn assert_no_panel_overlap(size: (u16, u16), d: &GlobalData) {
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render(size, &state, d, false);
    let map = state.hit_map();
    for i in 0..map.panels.len() {
        for j in i + 1..map.panels.len() {
            assert!(
                !rects_overlap(map.panels[i].0, map.panels[j].0),
                "panels {:?} and {:?} overlap at {size:?}:\n{}",
                map.panels[i],
                map.panels[j],
                text(&buf)
            );
        }
    }
}

#[test]
fn no_panels_overlap_at_tiny_heights() {
    let d = dense_overview_data();
    for height in 1..=8 {
        assert_no_panel_overlap((100, height), &d);
    }
    for height in 3..=10 {
        assert_no_panel_overlap((140, height), &d);
    }
}

#[test]
fn top_pair_floor_is_one_panel_and_cap_does_not_squeeze_messages() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.quota.push(quota("claude", 300, 80.0));
    snap.terminals.push(terminal("t1", "worker-1"));

    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    for title in ["Quota", "Terminals", "Inbox"] {
        let blanks = blank_inner_rows(&buf, title);
        assert!(
            blanks <= 1,
            "{title} has {blanks} blank inner rows:\n{}",
            text(&buf)
        );
    }

    // Only 8 rows for the content area: 60% of (8 - 4) is 2, so the pair must not be
    // inflated to 6 rows — Messages keeps the space nobody else needs.
    d.view = vec![Section::Quota, Section::Terminals];
    let buf = render((140, 10), &state, &d, false);
    let (_, _, _, pair_h) = panel_rect(&buf, "Quota");
    let (_, _, _, messages_h) = panel_rect(&buf, "Messages");
    assert_eq!(
        pair_h,
        3,
        "top pair floor must be MIN_SECTION_H:\n{}",
        text(&buf)
    );
    assert!(
        messages_h >= 5,
        "Messages was squeezed to {messages_h} rows:\n{}",
        text(&buf)
    );
}

#[test]
fn tiny_heights_two_and_three_stay_inside_main() {
    let d = dense_overview_data();
    for height in [2, 3] {
        assert_everything_inside_main((140, height), &d);
        assert_no_panel_overlap((140, height), &d);
    }
}

#[test]
fn scrolled_table_row_rects_match_drawn_rows() {
    let mut d = data(&[Section::Terminals]);
    for i in 0..45 {
        d.snap
            .as_mut()
            .unwrap()
            .terminals
            .push(terminal(&format!("t{i}"), &format!("worker-{i}")));
    }
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_terminal = Some("t44".into());
    let buf = render((140, 14), &state, &d, false);
    let map = state.hit_map();
    let (x, y) = find(&buf, "worker-44");
    let rect = map
        .rows
        .iter()
        .find(|(_, target)| matches!(target, RowTarget::Terminal(address) if address == "t44"))
        .expect("selected terminal row")
        .0;
    assert_eq!(
        rect.y,
        y,
        "hit row must sit on the drawn row:\n{}",
        text(&buf)
    );
    assert_eq!(
        hit(&map, x, y),
        Some(Hit::Row(RowTarget::Terminal("t44".into()))),
        "the selected row below the first page must be hit-testable:\n{}",
        text(&buf)
    );
    let (ix, iy) = find(&buf, "worker-40");
    assert_eq!(
        hit(&map, ix, iy),
        Some(Hit::Row(RowTarget::Terminal("t40".into()))),
        "the first visible scrolled row must be hit-testable too"
    );

    let mut d = data(&[Section::Inbox]);
    for i in 0..20 {
        d.snap.as_mut().unwrap().inbox.push(InboxRow {
            id: i,
            ts_ms: NOW,
            text: format!("item-{i}"),
            ..InboxRow::default()
        });
    }
    let mut state = GlobalState::new(ThemeName::CatppuccinMocha);
    state.sel_inbox = Some(19);
    let buf = render((140, 14), &state, &d, false);
    let map = state.hit_map();
    let (ix, iy) = find(&buf, "item-19");
    assert_eq!(
        hit(&map, ix, iy),
        Some(Hit::Row(RowTarget::Inbox(19))),
        "scrolled inbox row must be hit-testable:\n{}",
        text(&buf)
    );
}

#[test]
fn empty_terminals_panel_is_three_rows() {
    let d = data(&[Section::Terminals]);
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    let (_, _, _, h) = panel_rect(&buf, "Terminals");
    assert_eq!(
        h,
        3,
        "empty Terminals is a border and one dash line:\n{}",
        text(&buf)
    );
}

#[test]
fn top_pair_bridged_sections_use_their_column_width() {
    let mut d = data(&[Section::Usage, Section::System]);
    let snap = d.snap.as_mut().unwrap();
    snap.usage_summary.incomplete_coverage = true;
    snap.usage_summary.four_months_start_day =
        "2026-01-01 (then a deliberately long label so the warning wraps in a half-width column)"
            .repeat(3);
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((120, 40), &state, &d, false);
    // Measured at the real 60-column width the warning wraps many times; at 120 columns it fits
    // on far fewer lines, which used to leave the panel too short for its content.
    let (_, _, _, usage_h) = panel_rect(&buf, "Usage");
    assert!(
        usage_h >= 8,
        "Usage was sized for the whole window, not its 60-column half: {usage_h}\n{}",
        text(&buf)
    );
}

#[test]
fn output_overlay_hides_covered_hit_entries() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    for i in 0..11 {
        snap.terminals
            .push(terminal(&format!("t{i}"), &format!("worker-{i}")));
    }
    d.output = Some(
        (0..40)
            .map(|i| format!("out-{i:02}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    let map = state.hit_map();
    let (_, output_top, _, output_h) = panel_rect(&buf, "Output");
    let output = ratatui::layout::Rect::new(0, output_top, 140, output_h);
    assert!(
        map.panels
            .iter()
            .any(|(r, p)| *p == HitPanel::Output && *r == output),
        "the Output panel itself must be in the hit map:\n{}",
        text(&buf)
    );
    let covered = map
        .panels
        .iter()
        .map(|(r, p)| (*r, format!("{p:?}")))
        .chain(map.rows.iter().map(|(r, t)| (*r, format!("{t:?}"))))
        .chain(map.borders.iter().map(|(r, b)| (*r, format!("{b:?}"))))
        .filter(|(r, label)| {
            !matches!(label.as_str(), "Output" | "OutputTop") && r.y < output.bottom()
        })
        .collect::<Vec<_>>();
    assert!(
        covered.iter().all(|(r, _)| r.bottom() <= output_top),
        "hidden entries survived under Output: {covered:?}:\n{}",
        text(&buf)
    );
    assert_eq!(
        hit(&map, 5, output_top),
        Some(Hit::Border(BorderId::OutputTop)),
        "Output's top boundary must be draggable"
    );
    assert_eq!(
        hit(&map, 5, output_top + 2),
        Some(Hit::Panel(HitPanel::Output)),
        "clicking the Output body must not hit a hidden panel"
    );
}

#[test]
fn message_row_rects_follow_wrapped_height() {
    let mut d = data(&Section::DEFAULT);
    let snap = d.snap.as_mut().unwrap();
    snap.messages.push(MessageRow {
        id: 1,
        ts_ms: NOW,
        from: "t1".into(),
        to: "t2".into(),
        kind: "BÁO CÁO".into(),
        text: "x".repeat(250),
        ..MessageRow::default()
    });
    snap.messages.push(MessageRow {
        id: 2,
        ts_ms: NOW,
        from: "t1".into(),
        to: "t2".into(),
        kind: "BÁO CÁO".into(),
        text: "second-message".into(),
        ..MessageRow::default()
    });
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((100, 30), &state, &d, false);
    let map = state.hit_map();
    let r0 = map
        .rows
        .iter()
        .find(|(_, target)| matches!(target, RowTarget::Message(0)))
        .expect("message 0 row")
        .0;
    let r1 = map
        .rows
        .iter()
        .find(|(_, target)| matches!(target, RowTarget::Message(1)))
        .expect("message 1 row")
        .0;
    assert!(
        r0.height >= 3,
        "a 250-cell message wraps into header + 3 text rows, got {r0:?}:\n{}",
        text(&buf)
    );
    assert_eq!(r1.y, r0.y + r0.height, "rows must be stacked without gaps");
    let (x, y) = find(&buf, "second-message");
    assert_eq!(
        hit(&map, x, y),
        Some(Hit::Row(RowTarget::Message(1))),
        "message 1 rect is not where its text was drawn:\n{}",
        text(&buf)
    );
}
