//! Full-screen `agm tui`: hosts the global screen and the project board as two screens.
//!
//! The loop owns the terminal, loading and the refresh cadence; both screens are pure state plus
//! `on_key`, so what they do is testable without a console.

use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Position, Rect};

use super::cmd::{Dispatched, NoteKind, Outcome};
use super::global::{CmdKey, Focus, GEffect, GlobalData, GlobalState, Popup};
use super::global_view;
use super::state::{Mode, Panel};
use super::{BoardKeyResult, BoardScreen, theme};
use crate::config::Config;
use crate::dashboard::{self, Section};
use crate::hc::Hc;
use crate::project::{self, ProjectKey, RootCache};
use crate::store::Store;

/// The screen the app currently shows.
enum Screen {
    Global,
    Board(Box<BoardScreen>),
}

impl Screen {
    fn kind(&self) -> ScreenKind {
        match self {
            Screen::Global => ScreenKind::Global,
            Screen::Board(_) => ScreenKind::Board,
        }
    }
}

/// The screen as a plain value, so the refresh decision stays testable without a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenKind {
    Global,
    Board,
}

/// Which data sources one tick of the loop refreshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RefreshTargets {
    pub global: bool,
    pub board: bool,
}

/// The board screen reloads only itself; the global data is loaded while the global screen is
/// shown (and therefore again on the first tick after the board returns to it), not every board
/// tick.
pub(crate) fn refresh_targets(screen: ScreenKind) -> RefreshTargets {
    match screen {
        ScreenKind::Global => RefreshTargets {
            global: true,
            board: false,
        },
        ScreenKind::Board => RefreshTargets {
            global: false,
            board: true,
        },
    }
}

/// What one key on the global screen asks the loop to do next.
enum AppAction {
    Stay,
    /// Refresh the global data (and with it the selections) before drawing again.
    Refresh,
    OpenBoard(ProjectKey),
    Quit,
}

/// What the loop does after a key: redraw at once, refresh first, or switch screens / leave.
enum Next {
    Redraw,
    /// The event changed nothing visible: keep the current frame, no redraw.
    Skip,
    Restart,
    /// Back to the global screen, adopting the board's (possibly changed) theme.
    Global(theme::ThemeName),
    Board(ProjectKey),
    Quit,
}

/// Runs the app until the user quits. `dispatch` runs one command line (shared with `--plain`).
/// Caller must have passed the human-only check.
pub fn run_app(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    refresh_s: u64,
    scan: &mut dyn FnMut(&Store) -> Result<()>,
    initial_view: Vec<Section>,
    dispatch: &mut dyn FnMut(&str) -> Dispatched,
) -> Result<()> {
    let theme_name = store
        .setting(theme::SETTING_KEY)?
        .and_then(|s| theme::ThemeName::parse(&s))
        .unwrap_or_default();
    let mut state = GlobalState::new(theme_name);
    let mut data = GlobalData {
        view: initial_view,
        ..GlobalData::default()
    };
    let mut global = GlobalSide::new();
    let mut screen = Screen::Global;
    let refresh = Duration::from_secs(refresh_s.max(1));

    super::with_console(|| -> Result<()> {
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        terminal.clear()?;
        let ascii = std::env::var("AGM_ASCII").as_deref() == Ok("1");
        let mut board_cmd_focus = false;

        loop {
            let _ = scan(store);
            let targets = refresh_targets(screen.kind());
            if targets.global {
                global.refresh(store, cfg, &mut state, &mut data, super::clock_ms());
            }
            if targets.board
                && let Screen::Board(board) = &mut screen
            {
                board.refresh(store, cfg, super::clock_ms());
            }
            draw(
                &mut terminal,
                &mut screen,
                &state,
                &data,
                board_cmd_focus,
                ascii,
            )?;

            let until = Instant::now() + refresh;
            loop {
                let left = until.saturating_duration_since(Instant::now());
                if left.is_zero() || !event::poll(left)? {
                    break;
                }
                match event::read()? {
                    Event::Key(key) => {
                        // `ctrl+c` quits the whole app from either screen.
                        if is_ctrl_c(&key) {
                            return Ok(());
                        }
                        let next = if let Screen::Board(board) = &mut screen {
                            handle_board_key(
                                key,
                                board,
                                store,
                                hc,
                                &mut state,
                                &mut data,
                                &mut board_cmd_focus,
                                dispatch,
                                super::clock_ms(),
                            )
                        } else {
                            match handle_global_key(key, store, &mut state, &mut data, dispatch) {
                                AppAction::Stay => Next::Redraw,
                                AppAction::Refresh => {
                                    // Commands change what the snapshot holds (done, kill, reply):
                                    // reload before the next key can act on a stale row, keeping
                                    // the note / output / popup the command just produced.
                                    global.refresh(
                                        store,
                                        cfg,
                                        &mut state,
                                        &mut data,
                                        super::clock_ms(),
                                    );
                                    Next::Redraw
                                }
                                AppAction::Quit => Next::Quit,
                                AppAction::OpenBoard(key) => Next::Board(key),
                            }
                        };
                        match next {
                            Next::Skip => {}
                            Next::Quit => return Ok(()),
                            Next::Redraw => {
                                draw(
                                    &mut terminal,
                                    &mut screen,
                                    &state,
                                    &data,
                                    board_cmd_focus,
                                    ascii,
                                )?;
                            }
                            Next::Restart => break,
                            Next::Global(board_theme) => {
                                // `q` / `esc` in the board returns to the global screen, never out
                                // of the app (builder-check answer 5). `t` in the board applies to
                                // the whole app, so the global screen adopts its theme.
                                screen = Screen::Global;
                                state.theme = board_theme;
                                state.focus = Focus::CmdLine;
                                board_cmd_focus = false;
                                data.output = None;
                                state.output_scroll = 0;
                                break;
                            }
                            Next::Board(key) => {
                                data.output = None;
                                state.output_scroll = 0;
                                board_cmd_focus = false;
                                screen = Screen::Board(Box::new(BoardScreen::new(
                                    Some(key),
                                    state.theme,
                                )));
                                break;
                            }
                        }
                    }
                    Event::Mouse(mouse) => {
                        let next = handle_mouse(
                            mouse,
                            &mut screen,
                            store,
                            hc,
                            &mut state,
                            &data,
                            &mut board_cmd_focus,
                            super::clock_ms(),
                        );
                        match next {
                            Next::Skip => {}
                            Next::Quit => return Ok(()),
                            Next::Redraw => {
                                draw(
                                    &mut terminal,
                                    &mut screen,
                                    &state,
                                    &data,
                                    board_cmd_focus,
                                    ascii,
                                )?;
                            }
                            Next::Restart => break,
                            Next::Global(board_theme) => {
                                screen = Screen::Global;
                                state.theme = board_theme;
                                state.focus = Focus::CmdLine;
                                board_cmd_focus = false;
                                data.output = None;
                                state.output_scroll = 0;
                                break;
                            }
                            Next::Board(key) => {
                                data.output = None;
                                state.output_scroll = 0;
                                board_cmd_focus = false;
                                screen = Screen::Board(Box::new(BoardScreen::new(
                                    Some(key),
                                    state.theme,
                                )));
                                break;
                            }
                        }
                    }
                    Event::Resize(_, _) => draw(
                        &mut terminal,
                        &mut screen,
                        &state,
                        &data,
                        board_cmd_focus,
                        ascii,
                    )?,
                    _ => {}
                }
            }
        }
    })
}

/// The global screen's loop-side state: resolved-root cache, CPU baseline, last refresh error.
struct GlobalSide {
    cache: RootCache,
    /// RAM/CPU/disk for the SYSTEM section and the header; kept across frames because CPU % is a
    /// difference between two samples (the same sampler `--plain` keeps).
    sampler: Option<crate::sysmon::SysSampler>,
    /// Text of the error note the last failed refresh left (cleared by the next good refresh).
    refresh_note: Option<String>,
}

impl GlobalSide {
    fn new() -> Self {
        Self {
            cache: RootCache::new(),
            sampler: crate::sysmon::SysSampler::new(),
            refresh_note: None,
        }
    }

    /// Loads one frame; a failure keeps the last good data and shows the error in the footer,
    /// exactly like the board's refresh.
    fn refresh(
        &mut self,
        store: &Store,
        cfg: &Config,
        state: &mut GlobalState,
        data: &mut GlobalData,
        now_ms: i64,
    ) {
        match load_global(store, cfg, &mut self.cache, self.sampler.as_mut(), now_ms) {
            Ok((snap, project_of)) => {
                data.snap = Some(snap);
                data.project_of = project_of;
                data.now_ms = now_ms;
                if state.note.as_ref().map(|(_, text)| text.as_str())
                    == self.refresh_note.as_deref()
                {
                    state.note = None;
                }
                self.refresh_note = None;
            }
            Err(e) => {
                let text = format!("error: {e:#}");
                state.note = Some((NoteKind::Err, text.clone()));
                self.refresh_note = Some(text);
            }
        }
        state.sync(data);
    }
}

/// Everything the global screen shows: the dashboard snapshot and terminal address → project.
fn load_global(
    store: &Store,
    cfg: &Config,
    cache: &mut RootCache,
    sampler: Option<&mut crate::sysmon::SysSampler>,
    now_ms: i64,
) -> Result<(dashboard::Snapshot, HashMap<String, ProjectKey>)> {
    let mut snap = dashboard::snapshot(store, cfg, now_ms)?;
    // The database has no RAM/CPU/disk readings; `agm tui --plain` fills them in, so does the app.
    snap.system = sampler.map(|m| m.sample());
    let terms = store.terminals(false)?;
    let inbox = store.inbox(true)?;
    let roots = project::compute_roots(&terms, &snap, cache, &project::git_common_root);
    let (projects, _) = project::list_projects(&terms, &inbox, &roots);
    let by_root: HashMap<&str, &ProjectKey> = projects
        .iter()
        .map(|p| (p.key.root.as_str(), &p.key))
        .collect();
    let project_of = roots
        .iter()
        .filter_map(|(address, root)| {
            by_root
                .get(root.as_str())
                .map(|key| (address.clone(), (*key).clone()))
        })
        .collect();
    Ok((snap, project_of))
}

/// One key on the global screen: `GlobalState::on_key` plus what only the loop can do (dispatch,
/// persisting the theme, closing the Output panel, quitting).
fn handle_global_key(
    key: KeyEvent,
    store: &Store,
    state: &mut GlobalState,
    data: &mut GlobalData,
    dispatch: &mut dyn FnMut(&str) -> Dispatched,
) -> AppAction {
    // The Output panel closes on `esc` as well as on the next command (builder-check answer 1).
    // Windows emits a Release for every key; only Press/Repeat may act.
    if key.kind != KeyEventKind::Release && key.code == KeyCode::Esc && state.popup.is_none() {
        data.output = None;
        state.output_scroll = 0;
    }
    match state.on_key(key, data) {
        GEffect::None => AppAction::Stay,
        GEffect::Run(line) => {
            let dispatched = dispatch(&line);
            let quit = dispatched.outcome == Outcome::Quit;
            let refresh = effect_requests_refresh(&dispatched.outcome);
            let board = apply_dispatched(state, data, dispatched);
            if quit {
                AppAction::Quit
            } else if let Some(key) = board {
                AppAction::OpenBoard(key)
            } else if refresh {
                AppAction::Refresh
            } else {
                AppAction::Stay
            }
        }
        GEffect::OpenBoard(address) => {
            // The row's name makes the "has no project" note readable; the address when unnamed.
            let name = terminal_label(data, &address);
            match terminal_project(data, &address, &name) {
                Ok(key) => AppAction::OpenBoard(key),
                Err(text) => {
                    state.note = Some((NoteKind::Warn, text));
                    AppAction::Stay
                }
            }
        }
        GEffect::SaveTheme(name) => {
            state.note = Some(match store.set_setting(theme::SETTING_KEY, name.as_str()) {
                Ok(()) => (NoteKind::Ok, format!("theme: {}", name.as_str())),
                Err(e) => (NoteKind::Err, format!("error: {e:#}")),
            });
            AppAction::Stay
        }
        GEffect::Quit => AppAction::Quit,
    }
}

/// Whether dispatching this outcome must refresh the global data before the next key.
///
/// Everything except leaving the screen (`Quit`) and switching to the board (`OpenBoard`) can
/// change the snapshot, so the loop reloads it immediately instead of waiting for the next tick.
pub(crate) fn effect_requests_refresh(outcome: &Outcome) -> bool {
    !matches!(outcome, Outcome::Quit | Outcome::OpenBoard(_))
}

/// Applies one dispatch result to the global screen; returns the project whose board to open.
pub(crate) fn apply_dispatched(
    state: &mut GlobalState,
    data: &mut GlobalData,
    dispatched: Dispatched,
) -> Option<ProjectKey> {
    // One command replaces the previous result, exactly as one line does in `--plain`.
    state.note = None;
    data.output = None;
    data.view = dispatched.view;
    match dispatched.outcome {
        Outcome::Note(kind, text) => state.note = Some((kind, text)),
        Outcome::Shown(text) => {
            data.output = Some(text);
            state.output_scroll = 0;
        }
        Outcome::NeedsConfirm(prompt) => {
            state.popup = Some(Popup::Confirm {
                prompt,
                input: String::new(),
            });
        }
        Outcome::OpenBoard(key) => return Some(key),
        Outcome::Quit | Outcome::None => {}
    }
    None
}

/// The project whose board a terminal row opens; `<name> has no project` when unknown.
pub(crate) fn terminal_project(
    data: &GlobalData,
    address: &str,
    name: &str,
) -> Result<ProjectKey, String> {
    match data.project_of.get(address) {
        Some(key) => Ok(key.clone()),
        None if name.is_empty() => Err(format!("{address} has no project")),
        None => Err(format!("{name} has no project")),
    }
}

/// The label of a terminal row (its name, else its address), for notes and messages.
fn terminal_label(data: &GlobalData, address: &str) -> String {
    data.snap
        .as_ref()
        .and_then(|snap| snap.terminals.iter().find(|t| t.address == address))
        .map(|t| {
            if t.name.is_empty() {
                t.address.clone()
            } else {
                t.name.clone()
            }
        })
        .unwrap_or_else(|| address.to_string())
}

/// The key that quits the app from either screen. Windows also emits Release for `ctrl+c`.
fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.kind != KeyEventKind::Release
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    screen: &mut Screen,
    state: &GlobalState,
    data: &GlobalData,
    board_cmd_focus: bool,
    ascii: bool,
) -> Result<()> {
    terminal.draw(|frame| draw_frame(frame, screen, state, data, board_cmd_focus, ascii))?;
    Ok(())
}

fn draw_frame(
    frame: &mut ratatui::Frame,
    screen: &mut Screen,
    state: &GlobalState,
    data: &GlobalData,
    board_cmd_focus: bool,
    ascii: bool,
) {
    match screen {
        Screen::Global => {
            let palette = theme::theme(state.theme);
            global_view::draw_global(frame, state, data, &palette, ascii);
        }
        Screen::Board(board) => {
            let area = frame.area();
            if area.width == 0 || area.height == 0 {
                return;
            }
            let palette = theme::theme(board.theme());
            let footer_h = area.height.min(1);
            let cmd_h = area.height.saturating_sub(1).min(1);
            let main_h = area.height.saturating_sub(footer_h + cmd_h);
            let board_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height.saturating_sub(1),
            };
            let main_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: main_h,
            };
            let cmd_area = Rect {
                x: area.x,
                y: area.y + main_h,
                width: area.width,
                height: cmd_h,
            };
            let footer_area = Rect {
                x: area.x,
                y: area.y + main_h + cmd_h,
                width: area.width,
                height: footer_h,
            };
            board.hit_map = super::view::draw_in(
                frame,
                board_area,
                &board.state,
                &board.data,
                &palette,
                ascii,
            );
            if let Some(output) = &data.output {
                global_view::draw_output_overlay(
                    frame,
                    main_area,
                    output,
                    state.output_scroll,
                    &palette,
                    ascii,
                );
                // The Output panel covers the board: nothing under it may stay hit-testable.
                board.hit_map.begin_overlay(main_area);
            }
            let mut cursor = global_view::draw_cmdline_bar(
                frame,
                cmd_area,
                &state.cmd,
                state.popup.is_none() && board_cmd_focus,
                &palette,
            );
            let board_note = board.state.note.as_ref().map(|n| {
                (
                    if n.ok { NoteKind::Ok } else { NoteKind::Err },
                    n.text.clone(),
                )
            });
            let note = state.note.as_ref().or(board_note.as_ref());
            let help = if ascii {
                " up/down select  tab panel  enter open  r reply  d done  s send  / filter  : cmd  t theme  q back"
            } else {
                " ↑↓ select  tab panel  ⏎ open  r reply  d done  s send  / filter  : cmd  t theme  q back"
            };
            global_view::draw_footer_bar(frame, footer_area, note, help, &palette);
            if let Some(pos) = global_view::draw_confirm_popup(frame, area, state, &palette, ascii)
            {
                cursor = Some(pos);
            }
            if let Some(pos) = cursor {
                let x = pos.x.min(area.right().saturating_sub(1));
                let y = pos.y.min(area.bottom().saturating_sub(1));
                frame.set_cursor_position(Position::new(x, y));
            }
        }
    }
}

fn run_board_dispatch(
    line: &str,
    board: &mut Box<BoardScreen>,
    state: &mut GlobalState,
    data: &mut GlobalData,
    board_cmd_focus: &mut bool,
    dispatch: &mut dyn FnMut(&str) -> Dispatched,
) -> Next {
    let dispatched = dispatch(line);
    let quit = dispatched.outcome == Outcome::Quit;
    let refresh = effect_requests_refresh(&dispatched.outcome);
    let open_board = apply_dispatched(state, data, dispatched);
    if quit {
        Next::Quit
    } else if let Some(key) = open_board {
        **board = BoardScreen::new(Some(key), board.theme());
        *board_cmd_focus = false;
        data.output = None;
        state.output_scroll = 0;
        Next::Restart
    } else if refresh {
        Next::Restart
    } else {
        Next::Redraw
    }
}

/// Routes one mouse event: T3 owns the overview's mouse, and ignoring it must not force a redraw.
/// Routes one mouse event: T3 owns the overview's mouse, and ignoring it must not force a redraw.
#[allow(clippy::too_many_arguments)]
fn handle_mouse(
    mouse: event::MouseEvent,
    screen: &mut Screen,
    store: &Store,
    hc: &dyn Hc,
    state: &mut GlobalState,
    data: &GlobalData,
    board_cmd_focus: &mut bool,
    now_ms: i64,
) -> Next {
    match screen {
        Screen::Board(board) => handle_board_mouse(
            mouse,
            board,
            store,
            hc,
            state,
            data,
            board_cmd_focus,
            now_ms,
        ),
        Screen::Global => Next::Skip,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_board_mouse(
    mouse: event::MouseEvent,
    board: &mut Box<BoardScreen>,
    store: &Store,
    hc: &dyn Hc,
    state: &mut GlobalState,
    data: &GlobalData,
    board_cmd_focus: &mut bool,
    now_ms: i64,
) -> Next {
    // Popups, the board command bar and the Output panel all own the mouse while they are up.
    if state.popup.is_some() || *board_cmd_focus || data.output.is_some() {
        return Next::Skip;
    }
    let redraw = board.on_mouse(mouse, store, hc, now_ms);
    if board.needs_refresh() {
        Next::Restart
    } else if redraw {
        Next::Redraw
    } else {
        Next::Skip
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_board_key(
    key: KeyEvent,
    board: &mut Box<BoardScreen>,
    store: &Store,
    hc: &dyn Hc,
    state: &mut GlobalState,
    data: &mut GlobalData,
    board_cmd_focus: &mut bool,
    dispatch: &mut dyn FnMut(&str) -> Dispatched,
    now_ms: i64,
) -> Next {
    if key.kind == KeyEventKind::Release {
        return Next::Redraw;
    }
    if state.popup.is_some() {
        return match state.handle_popup_key(key) {
            GEffect::Run(line) => {
                run_board_dispatch(&line, board, state, data, board_cmd_focus, dispatch)
            }
            GEffect::Quit => Next::Quit,
            _ => Next::Redraw,
        };
    }
    if *board_cmd_focus {
        return match state.cmd.on_key(key) {
            CmdKey::Submit(line) => {
                run_board_dispatch(&line, board, state, data, board_cmd_focus, dispatch)
            }
            CmdKey::Esc => {
                *board_cmd_focus = false;
                data.output = None;
                state.output_scroll = 0;
                Next::Redraw
            }
            CmdKey::Edited | CmdKey::SubmitEmpty => Next::Redraw,
            CmdKey::Unhandled => match key.code {
                KeyCode::PageUp => {
                    if data.output.is_some() {
                        state.output_scroll = state.output_scroll.saturating_sub(5);
                    }
                    Next::Redraw
                }
                KeyCode::PageDown => {
                    if data.output.is_some() {
                        state.output_scroll = state.output_scroll.saturating_add(5);
                    }
                    Next::Redraw
                }
                KeyCode::Tab => {
                    *board_cmd_focus = false;
                    board.state.panel = Panel::Orchestration;
                    Next::Redraw
                }
                KeyCode::BackTab => {
                    *board_cmd_focus = false;
                    board.state.panel = Panel::Inbox;
                    Next::Redraw
                }
                _ => Next::Redraw,
            },
        };
    }
    if data.output.is_some() && key.code == KeyCode::Esc {
        data.output = None;
        state.output_scroll = 0;
        return Next::Redraw;
    }
    if board.state.mode == Mode::Normal {
        state.note = None;
        if key.code == KeyCode::Char(':')
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
        {
            *board_cmd_focus = true;
            return Next::Redraw;
        }
        if key.code == KeyCode::Tab && board.state.panel == Panel::Inbox {
            *board_cmd_focus = true;
            return Next::Redraw;
        }
        // Shift+Tab from the first board panel wraps to the command bar (mirror of Tab from Inbox).
        if key.code == KeyCode::BackTab && board.state.panel == Panel::Orchestration {
            *board_cmd_focus = true;
            return Next::Redraw;
        }
    }
    match board.on_key(key, store, hc, now_ms) {
        BoardKeyResult::Leave => Next::Global(board.theme()),
        BoardKeyResult::Stay if board.needs_refresh() => Next::Restart,
        BoardKeyResult::Stay => Next::Redraw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(name: &str) -> ProjectKey {
        ProjectKey {
            root: format!("/repo/{name}"),
            name: name.into(),
        }
    }

    fn dispatched(outcome: Outcome, view: Vec<Section>) -> Dispatched {
        Dispatched { outcome, view }
    }

    #[test]
    fn apply_dispatched_maps_each_outcome() {
        let mut state = GlobalState::new(theme::ThemeName::default());
        let mut data = GlobalData::default();
        let view = vec![Section::Quota, Section::Inbox];

        // Note: a footer note, the Output panel closed, the current view kept.
        let opened = apply_dispatched(
            &mut state,
            &mut data,
            dispatched(
                Outcome::Note(NoteKind::Ok, "killed t1".into()),
                view.clone(),
            ),
        );
        assert_eq!(opened, None);
        assert_eq!(
            state.note,
            Some((NoteKind::Ok, "killed t1".to_string())),
            "a Note must become the footer note"
        );
        assert_eq!(data.output, None, "a Note must close the Output panel");
        assert_eq!(data.view, view, "every result carries the current view");

        // Shown: the text goes to the Output panel, the footer note is cleared (as in --plain).
        apply_dispatched(
            &mut state,
            &mut data,
            dispatched(Outcome::Shown("cleanup 1 t2".into()), view.clone()),
        );
        assert_eq!(data.output.as_deref(), Some("cleanup 1 t2"));
        assert_eq!(state.note, None);

        // NeedsConfirm: a popup with an empty input, no Output text.
        apply_dispatched(
            &mut state,
            &mut data,
            dispatched(
                Outcome::NeedsConfirm("kill t1? type y then enter".into()),
                view.clone(),
            ),
        );
        assert_eq!(
            state.popup,
            Some(Popup::Confirm {
                prompt: "kill t1? type y then enter".into(),
                input: String::new(),
            })
        );
        assert_eq!(data.output, None);

        // OpenBoard: returns the project to open and closes the Output panel.
        let opened = apply_dispatched(
            &mut state,
            &mut data,
            dispatched(Outcome::OpenBoard(pk("shop")), vec![Section::Usage]),
        );
        assert_eq!(opened, Some(pk("shop")));
        assert_eq!(data.output, None);
        assert_eq!(data.view, vec![Section::Usage], "view is replaced too");

        // Quit and None: nothing to open, no note invented.
        assert_eq!(
            apply_dispatched(
                &mut state,
                &mut data,
                dispatched(Outcome::Quit, view.clone())
            ),
            None
        );
        assert_eq!(state.note, None);
        assert_eq!(
            apply_dispatched(
                &mut state,
                &mut data,
                dispatched(Outcome::None, view.clone())
            ),
            None
        );
        assert_eq!(data.view, view);
    }

    #[test]
    fn run_effect_requests_refresh() {
        for outcome in [
            Outcome::Note(NoteKind::Ok, "killed t1".into()),
            Outcome::Shown("cleanup list".into()),
            Outcome::NeedsConfirm("kill t1?".into()),
            Outcome::None,
        ] {
            assert!(
                effect_requests_refresh(&outcome),
                "{outcome:?} should refresh right after dispatching"
            );
        }
        for outcome in [Outcome::Quit, Outcome::OpenBoard(pk("shop"))] {
            assert!(
                !effect_requests_refresh(&outcome),
                "{outcome:?} must not refresh the global data"
            );
        }
    }

    #[test]
    fn board_tick_skips_the_global_refresh() {
        assert_eq!(
            refresh_targets(ScreenKind::Global),
            RefreshTargets {
                global: true,
                board: false,
            }
        );
        // Leaving the board switches the screen back to Global, whose tick refreshes there.
        assert_eq!(
            refresh_targets(ScreenKind::Board),
            RefreshTargets {
                global: false,
                board: true,
            }
        );
    }

    #[test]
    fn terminal_without_project_is_warn() {
        let mut data = GlobalData::default();
        // The message names the terminal the user selected, not its address.
        assert_eq!(
            terminal_project(&data, "t9", "worker-b"),
            Err("worker-b has no project".to_string())
        );

        data.project_of.insert("t9".into(), pk("agmag"));
        assert_eq!(terminal_project(&data, "t9", "worker-b"), Ok(pk("agmag")));

        // An unnamed terminal falls back to its address in the message.
        assert_eq!(
            terminal_project(&data, "t7", ""),
            Err("t7 has no project".to_string())
        );
    }

    fn key_ev(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    struct FakeHc;

    impl Hc for FakeHc {
        fn ls(&self, _all: bool) -> Result<Vec<serde_json::Value>> {
            Ok(vec![])
        }
        fn get(&self, _terminal: &str) -> Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }
        fn send(
            &self,
            _terminal: &str,
            _text: &str,
            _if_idle: bool,
            _force: bool,
        ) -> Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }
        fn run(&self, _spec: &crate::hc::RunSpec) -> Result<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }
        fn kill(&self, _terminal: &str) -> Result<()> {
            Ok(())
        }
        fn interrupt(&self, _terminal: &str) -> Result<()> {
            Ok(())
        }
    }

    fn fake_board_screen() -> Box<BoardScreen> {
        Box::new(BoardScreen::new(
            Some(pk("agmag")),
            theme::ThemeName::CatppuccinMocha,
        ))
    }

    #[test]
    fn board_in_app_shows_command_bar() {
        let mut screen = Screen::Board(fake_board_screen());
        let state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        let backend = ratatui::backend::TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_frame(f, &mut screen, &state, &data, false, false))
            .unwrap();
        let buf = terminal.backend().buffer();
        let cmd_row: String = (0..buf.area.width).map(|x| buf[(x, 22)].symbol()).collect();
        assert!(
            cmd_row.contains("agm> "),
            "board screen in app missing command bar on row above footer: {cmd_row:?}"
        );
    }

    #[test]
    fn output_overlay_hides_the_board_from_mouse() {
        let mut screen = Screen::Board(fake_board_screen());
        let state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let data = GlobalData {
            output: Some("cleanup 1 t2".into()),
            ..GlobalData::default()
        };
        let backend = ratatui::backend::TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_frame(f, &mut screen, &state, &data, false, false))
            .unwrap();
        let Screen::Board(board) = &screen else {
            panic!("screen changed")
        };
        // The Output panel covers the board area (command bar and footer stay visible).
        assert_eq!(board.hit_map.overlay, Some(Rect::new(0, 0, 100, 22)));
        assert!(
            board.hit_map.panels.is_empty() && board.hit_map.inbox_rows.is_empty(),
            "board targets stayed hit-testable under the Output panel"
        );
        assert!(
            crate::board::view::hit(&board.hit_map, 1, 1).is_none(),
            "a click under the Output panel still reached the board"
        );
    }

    fn mouse_ev(kind: event::MouseEventKind, column: u16, row: u16) -> event::MouseEvent {
        event::MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_events_that_change_nothing_skip_the_redraw() {
        use event::{MouseButton, MouseEventKind};

        let store = Store::open_in_memory().unwrap();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let data = GlobalData::default();
        let mut cmd_focus = false;

        // The overview's mouse belongs to T3: ignoring it must not repaint the frame.
        let mut screen = Screen::Global;
        assert!(matches!(
            handle_mouse(
                mouse_ev(MouseEventKind::Moved, 1, 1),
                &mut screen,
                &store,
                &FakeHc,
                &mut state,
                &data,
                &mut cmd_focus,
                0
            ),
            Next::Skip
        ));

        // On the board, events the state ignores do not repaint either.
        let mut board = fake_board_screen();
        board.hit_map = crate::board::state::BoardHitMap {
            panels: vec![(Rect::new(0, 0, 20, 10), crate::board::state::Panel::Detail)],
            ..Default::default()
        };
        let mut screen = Screen::Board(board);
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Down(MouseButton::Middle),
        ] {
            assert!(
                matches!(
                    handle_mouse(
                        mouse_ev(kind, 1, 1),
                        &mut screen,
                        &store,
                        &FakeHc,
                        &mut state,
                        &data,
                        &mut cmd_focus,
                        0
                    ),
                    Next::Skip
                ),
                "{kind:?} forced a redraw"
            );
        }

        // A click that moves the focus does repaint.
        assert!(matches!(
            handle_mouse(
                mouse_ev(MouseEventKind::Down(MouseButton::Left), 1, 1),
                &mut screen,
                &store,
                &FakeHc,
                &mut state,
                &data,
                &mut cmd_focus,
                10
            ),
            Next::Redraw
        ));
    }

    #[test]
    fn board_mouse_is_blocked_by_a_popup_or_the_command_bar() {
        use event::{MouseButton, MouseEventKind};

        let store = Store::open_in_memory().unwrap();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData::default();
        let mut board = fake_board_screen();
        board.hit_map = crate::board::state::BoardHitMap {
            panels: vec![
                (Rect::new(0, 0, 20, 10), crate::board::state::Panel::Detail),
                (Rect::new(20, 0, 20, 10), crate::board::state::Panel::Inbox),
            ],
            ..Default::default()
        };
        let mut cmd_focus = false;

        // Baseline: a click on another panel moves the focus and asks for a redraw.
        assert!(matches!(
            handle_board_mouse(
                mouse_ev(MouseEventKind::Down(MouseButton::Left), 1, 1),
                &mut board,
                &store,
                &FakeHc,
                &mut state,
                &data,
                &mut cmd_focus,
                10
            ),
            Next::Redraw
        ));
        assert_eq!(board.state.panel, Panel::Detail);

        // While the board command bar has the focus the mouse does nothing and nothing redraws.
        board.state.panel = Panel::Orchestration;
        cmd_focus = true;
        assert!(matches!(
            handle_board_mouse(
                mouse_ev(MouseEventKind::Down(MouseButton::Left), 1, 1),
                &mut board,
                &store,
                &FakeHc,
                &mut state,
                &data,
                &mut cmd_focus,
                20
            ),
            Next::Skip
        ));
        assert_eq!(board.state.panel, Panel::Orchestration);
        cmd_focus = false;

        // A global popup blocks the mouse the same way (it can only be answered by keys).
        let popup = Popup::Confirm {
            prompt: "kill t1?".into(),
            input: String::new(),
        };
        state.popup = Some(popup.clone());
        assert!(matches!(
            handle_board_mouse(
                mouse_ev(MouseEventKind::Down(MouseButton::Left), 1, 1),
                &mut board,
                &store,
                &FakeHc,
                &mut state,
                &data,
                &mut cmd_focus,
                30
            ),
            Next::Skip
        ));
        assert_eq!(board.state.panel, Panel::Orchestration);
        assert_eq!(state.popup, Some(popup));

        // An open Output panel is modal too: `Tab` can leave the command bar while it stays open,
        // and then a click on the panel text must not reach the board under it.
        state.popup = None;
        data.output = Some("cleanup 1 t2".into());
        // A different target than every earlier click, so this is a plain (not double) click.
        assert!(matches!(
            handle_board_mouse(
                mouse_ev(MouseEventKind::Down(MouseButton::Left), 21, 1),
                &mut board,
                &store,
                &FakeHc,
                &mut state,
                &data,
                &mut cmd_focus,
                40
            ),
            Next::Skip
        ));
        assert_eq!(board.state.panel, Panel::Orchestration);
    }

    fn press(
        code: KeyCode,
        board: &mut Box<BoardScreen>,
        state: &mut GlobalState,
        data: &mut GlobalData,
        cmd_focus: &mut bool,
        dispatch: &mut dyn FnMut(&str) -> Dispatched,
    ) -> Next {
        let store = Store::open_in_memory().unwrap();
        handle_board_key(
            key_ev(code),
            board,
            &store,
            &FakeHc,
            state,
            data,
            cmd_focus,
            dispatch,
            0,
        )
    }

    #[test]
    fn tab_from_board_command_bar_goes_to_orchestration() {
        let mut board = fake_board_screen();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData::default();
        let mut cmd_focus = true;
        let mut noop = |_: &str| Dispatched {
            outcome: Outcome::None,
            view: vec![],
        };
        press(
            KeyCode::Tab,
            &mut board,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut noop,
        );
        assert!(!cmd_focus);
        assert_eq!(board.state.panel, Panel::Orchestration);
    }

    #[test]
    fn backtab_from_orchestration_focuses_board_command_bar() {
        let mut board = fake_board_screen();
        board.state.panel = Panel::Orchestration;
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData::default();
        let mut cmd_focus = false;
        let mut noop = |_: &str| Dispatched {
            outcome: Outcome::None,
            view: vec![],
        };
        press(
            KeyCode::BackTab,
            &mut board,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut noop,
        );
        assert!(
            cmd_focus,
            "shift+tab from the first panel must reach the command bar"
        );
    }

    #[test]
    fn open_board_from_board_bar_clears_output() {
        let mut board = fake_board_screen();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData {
            output: Some("old list".into()),
            ..GlobalData::default()
        };
        state.output_scroll = 3;
        let mut cmd_focus = true;
        let mut open = |_: &str| Dispatched {
            outcome: Outcome::OpenBoard(pk("other")),
            view: vec![],
        };
        for c in "cd other".chars() {
            press(
                KeyCode::Char(c),
                &mut board,
                &mut state,
                &mut data,
                &mut cmd_focus,
                &mut open,
            );
        }
        press(
            KeyCode::Enter,
            &mut board,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut open,
        );
        assert_eq!(
            data.output, None,
            "old Output overlay must not cover the new board"
        );
        assert_eq!(state.output_scroll, 0);
    }

    #[test]
    fn colon_focuses_board_command_bar() {
        let store = Store::open_in_memory().unwrap();
        let hc = FakeHc;
        let mut board = fake_board_screen();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData::default();
        let mut cmd_focus = false;
        let mut noop = |_: &str| Dispatched {
            outcome: Outcome::None,
            view: vec![],
        };
        let next = handle_board_key(
            key_ev(KeyCode::Char(':')),
            &mut board,
            &store,
            &hc,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut noop,
            0,
        );
        assert!(matches!(next, Next::Redraw));
        assert!(cmd_focus, "':' must focus the board command bar");
    }

    #[test]
    fn board_command_bar_enter_runs_dispatch() {
        let store = Store::open_in_memory().unwrap();
        let hc = FakeHc;
        let mut board = fake_board_screen();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData::default();
        let mut cmd_focus = true;
        let mut recorded = Vec::new();
        let mut fake_dispatch = |line: &str| {
            recorded.push(line.to_string());
            Dispatched {
                outcome: Outcome::Note(NoteKind::Ok, "ok-note".into()),
                view: vec![],
            }
        };
        for c in "ls".chars() {
            handle_board_key(
                key_ev(KeyCode::Char(c)),
                &mut board,
                &store,
                &hc,
                &mut state,
                &mut data,
                &mut cmd_focus,
                &mut fake_dispatch,
                0,
            );
        }
        let next = handle_board_key(
            key_ev(KeyCode::Enter),
            &mut board,
            &store,
            &hc,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut fake_dispatch,
            0,
        );
        assert_eq!(recorded, vec!["ls".to_string()]);
        assert_eq!(state.note, Some((NoteKind::Ok, "ok-note".into())));
        assert!(matches!(next, Next::Restart));
    }

    #[test]
    fn esc_returns_to_board_keys() {
        let store = Store::open_in_memory().unwrap();
        let hc = FakeHc;
        let mut board = fake_board_screen();
        let mut state = GlobalState::new(theme::ThemeName::CatppuccinMocha);
        let mut data = GlobalData::default();
        let mut cmd_focus = true;
        let mut noop = |_: &str| Dispatched {
            outcome: Outcome::None,
            view: vec![],
        };
        let next = handle_board_key(
            key_ev(KeyCode::Esc),
            &mut board,
            &store,
            &hc,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut noop,
            0,
        );
        assert!(
            matches!(next, Next::Redraw),
            "esc in board command bar must stay on board, not leave"
        );
        assert!(!cmd_focus, "esc must return focus to board panels");
        // Pressing 'q' now that we're unfocused runs the board key (leaves board).
        let next = handle_board_key(
            key_ev(KeyCode::Char('q')),
            &mut board,
            &store,
            &hc,
            &mut state,
            &mut data,
            &mut cmd_focus,
            &mut noop,
            0,
        );
        assert!(matches!(next, Next::Global(_)));
    }
}
