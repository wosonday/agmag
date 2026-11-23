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

use super::cmd::{Dispatched, NoteKind, Outcome};
use super::global::{Focus, GEffect, GlobalData, GlobalState, Popup};
use super::global_view;
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
            draw(&mut terminal, &screen, &state, &data, ascii)?;

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
                            match board.on_key(key, store, hc, super::clock_ms()) {
                                BoardKeyResult::Leave => Next::Global(board.theme()),
                                BoardKeyResult::Stay if board.needs_refresh() => Next::Restart,
                                BoardKeyResult::Stay => Next::Redraw,
                            }
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
                            Next::Quit => return Ok(()),
                            Next::Redraw => {
                                draw(&mut terminal, &screen, &state, &data, ascii)?;
                            }
                            Next::Restart => break,
                            Next::Global(board_theme) => {
                                // `q` / `esc` in the board returns to the global screen, never out
                                // of the app (builder-check answer 5). `t` in the board applies to
                                // the whole app, so the global screen adopts its theme.
                                screen = Screen::Global;
                                state.theme = board_theme;
                                state.focus = Focus::CmdLine;
                                break;
                            }
                            Next::Board(key) => {
                                data.output = None;
                                state.output_scroll = 0;
                                screen = Screen::Board(Box::new(BoardScreen::new(
                                    Some(key),
                                    state.theme,
                                )));
                                break;
                            }
                        }
                    }
                    Event::Resize(_, _) => draw(&mut terminal, &screen, &state, &data, ascii)?,
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
    screen: &Screen,
    state: &GlobalState,
    data: &GlobalData,
    ascii: bool,
) -> Result<()> {
    terminal.draw(|frame| match screen {
        Screen::Global => {
            let palette = theme::theme(state.theme);
            global_view::draw_global(frame, state, data, &palette, ascii);
        }
        Screen::Board(board) => {
            let palette = theme::theme(board.theme());
            board.draw(frame, &palette, ascii);
        }
    })?;
    Ok(())
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
}
