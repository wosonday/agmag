//! Buffer tests for the global screen rendering (`src/board/global_view.rs`).

use std::collections::HashMap;

use agmag::board::global::{GlobalData, GlobalState, Popup};
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
        cjk > 0 && cjk < 500,
        "hostile text not clipped to its panels: {cjk} cells"
    );
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

// The bridge is Task B's `ansi_to_text`; until it merges the stub panics, so this one is ignored.
#[test]
#[ignore = "needs Task B (ansi_to_text stub panics)"]
fn bridged_sections_render_titles_and_body() {
    let d = data(&[Section::Usage, Section::Budgets, Section::System]);
    let state = GlobalState::new(ThemeName::CatppuccinMocha);
    let buf = render((140, 40), &state, &d, false);
    for title in ["Usage", "Budgets", "System"] {
        assert!(has(&buf, title), "missing title {title:?}:\n{}", text(&buf));
    }
}
