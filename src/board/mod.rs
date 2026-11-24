//! Full-screen project board (`agm board [project]`, `cd <project>` in `agm tui`).
//!
//! Pure parts: `flows` (edges between agents), `state` (keys → effects), `theme`, `view`
//! (ratatui rendering). This file owns all I/O: terminal guard, data loading, effects, loop.

pub mod ansi;
pub mod app;
pub mod cmd;
pub mod flows;
pub mod global;
pub mod global_view;
pub mod layout;
pub mod state;
pub mod theme;
pub mod view;

use std::io::{self, IsTerminal};
use std::panic::{self, PanicHookInfo};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use self::state::Effect;
use crate::actions::{Caller, DoneFilters, detect_caller};
use crate::config::Config;
use crate::dashboard;
use crate::hc::Hc;
use crate::project::{self, CdMatch, ProjectKey, ProjectSummary, ProjectView, RootCache};
use crate::store::{MessageRow, Store};

/// One-line footer message after an action or a failed refresh.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub ok: bool,
    pub text: String,
}

/// Everything one frame renders; rebuilt on every refresh.
#[derive(Debug, Clone, Default)]
pub struct BoardData {
    pub projects: Vec<ProjectSummary>,
    pub no_project_inbox: usize,
    /// None until a project is chosen.
    pub view: Option<ProjectView>,
    /// Parent/child edges of `view.agents` that exchanged at least one message.
    pub edges: Vec<flows::Edge>,
    /// Selected agent's newest ≤ 30 messages, newest first.
    pub conversation: Vec<MessageRow>,
    pub now_ms: i64,
}

/// `HC_TERMINAL_ID` of the calling process: the terminal an agent command ran in, if any.
fn hc_terminal() -> Option<String> {
    std::env::var("HC_TERMINAL_ID")
        .ok()
        .filter(|s| !s.is_empty())
}

fn clock_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Fails unless a human at an interactive terminal is calling (same rule as `agm tui`).
pub fn require_human(store: &Store, hc: &dyn Hc) -> Result<()> {
    let tty = io::stdin().is_terminal() && io::stdout().is_terminal();
    let term = hc_terminal();
    let who = detect_caller(store, hc, term.as_deref(), tty, false);
    if !matches!(who, Caller::User { .. }) {
        bail!(
            "agm board is only available to a human at an interactive terminal (TTY); agents should use standard agm commands"
        );
    }
    Ok(())
}

/// `match_project` over the current project list; ambiguous / unknown → error.
pub fn resolve_project(
    store: &Store,
    cfg: &Config,
    cache: &mut RootCache,
    query: &str,
) -> Result<ProjectKey> {
    let snap = dashboard::snapshot(store, cfg, clock_ms())?;
    let terms = store.terminals(false)?;
    let inbox = store.inbox(true)?;
    let roots = project::compute_roots(&terms, &snap, cache, &project::git_common_root);
    let (list, _) = project::list_projects(&terms, &inbox, &roots);
    match project::match_project(&list, query) {
        CdMatch::Found(key) => Ok(key),
        CdMatch::Ambiguous(candidates) => bail!("ambiguous: {}", candidates.join(", ")),
        CdMatch::NotFound => bail!("no project matching '{query}'"),
    }
}

/// Every address and name a message may use for one of `view`'s agents, once each.
fn view_parties(view: &ProjectView) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for agent in &view.agents {
        for key in flows::agent_keys(agent) {
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

/// Reloads `data.conversation` for the currently selected agent.
///
/// The old conversation is dropped first, so a query error never leaves another agent's messages
/// under the new agent's title.
pub(crate) fn reload_conversation(
    store: &Store,
    state: &state::BoardState,
    data: &mut BoardData,
) -> Result<()> {
    data.conversation.clear();
    if let Some(agent) = state.selected_agent(data) {
        data.conversation = store.messages_involving(&flows::agent_keys(agent), 30)?;
    }
    Ok(())
}

/// Loads one frame of data and calls `state.sync` before loading the selected agent's conversation.
pub fn load_data(
    store: &Store,
    cfg: &Config,
    cache: &mut RootCache,
    state: &mut state::BoardState,
    now_ms: i64,
) -> Result<BoardData> {
    let snap = dashboard::snapshot(store, cfg, now_ms)?;
    let terms = store.terminals(false)?;
    let inbox = store.inbox(true)?;
    let roots = project::compute_roots(&terms, &snap, cache, &project::git_common_root);
    let (projects, no_project_inbox) = project::list_projects(&terms, &inbox, &roots);
    let mut data = BoardData {
        projects,
        no_project_inbox,
        view: None,
        edges: Vec::new(),
        conversation: Vec::new(),
        now_ms,
    };

    match state.project.clone() {
        Some(key) => {
            let view = project::load_project_view(store, &snap, &key, &roots, now_ms)?;
            let pair_latest = store.pair_latest_messages(&view_parties(&view))?;
            data.edges = flows::edges(&view.agents, &pair_latest);
            data.view = Some(view);
            state.sync(&data);
            reload_conversation(store, state, &mut data)?;
        }
        // Without a project, `sync` clamps the project list and clears agent/inbox selections.
        None => state.sync(&data),
    }

    Ok(data)
}

/// One refresh result: a success replaces the data and clears the error note the previous failed
/// refresh left behind; a failure keeps the last good data and shows the error.
fn apply_refresh(
    data: &mut BoardData,
    state: &mut state::BoardState,
    fresh: Result<BoardData>,
    refresh_note: &mut Option<String>,
) {
    match fresh {
        Ok(fresh) => {
            *data = fresh;
            if state.note.as_ref().map(|note| note.text.clone()) == *refresh_note {
                state.note = None;
            }
            *refresh_note = None;
        }
        Err(e) => {
            let text = format!("error: {e:#}");
            state.note = Some(Note {
                ok: false,
                text: text.clone(),
            });
            *refresh_note = Some(text);
        }
    }
}

/// Runs an effect; the outcome (or error) becomes `state.note`. Never panics.
pub fn apply_effect(
    store: &Store,
    hc: &dyn Hc,
    effect: state::Effect,
    state: &mut state::BoardState,
    now_ms: i64,
) {
    let note = match effect {
        Effect::None | Effect::Quit | Effect::SwitchProject(_) => return,
        Effect::Reply { id, text } => match crate::actions::reply(store, hc, id, &text, now_ms) {
            Ok(()) => Note {
                ok: true,
                text: format!("replied to #{id}"),
            },
            Err(e) => Note {
                ok: false,
                text: format!("error: {e:#}"),
            },
        },
        Effect::Done { id } => {
            // Identical to `agm done <id>`: one explicit id on the legacy single-id path.
            match crate::actions::dismiss_done(
                store,
                vec![id],
                DoneFilters::default(),
                false,
                true,
                now_ms,
            ) {
                Ok(_) => Note {
                    ok: true,
                    text: format!("closed #{id}"),
                },
                Err(e) => Note {
                    ok: false,
                    text: format!("error: {e:#}"),
                },
            }
        }
        Effect::Send { to, kind, text } => {
            match crate::actions::send(store, hc, "user", &to, &kind, None, &text, now_ms) {
                // A delivery failure is recorded in the message row, not returned as an error.
                Ok(m) => Note {
                    ok: m.status != "failed",
                    text: format!("msg #{} → {}: {}", m.id, m.to, m.status),
                },
                Err(e) => Note {
                    ok: false,
                    text: format!("error: {e:#}"),
                },
            }
        }
        Effect::SaveTheme(name) => match store.set_setting(theme::SETTING_KEY, name.as_str()) {
            Ok(()) => Note {
                ok: true,
                text: format!("theme: {}", name.as_str()),
            },
            Err(e) => Note {
                ok: false,
                text: format!("error: {e:#}"),
            },
        },
    };
    state.note = Some(note);
}

/// Brings the console in and out of raw mode + the alternate screen; faked in tests.
pub(crate) trait TermControl {
    fn enter(&mut self) -> io::Result<()>;
    fn leave(&mut self) -> io::Result<()>;
}

/// Restores the terminal once: on the normal path, on an error return, and on unwind.
pub(crate) struct TermGuard<T: TermControl> {
    term: Option<T>,
}

impl<T: TermControl> TermGuard<T> {
    fn new(term: T) -> Self {
        Self { term: Some(term) }
    }

    fn enter(&mut self) -> io::Result<()> {
        match self.term.as_mut() {
            Some(term) => term.enter(),
            None => Ok(()),
        }
    }

    /// Leaves now; `Drop` will not leave a second time.
    fn leave(&mut self) -> io::Result<()> {
        match self.term.take() {
            Some(mut term) => term.leave(),
            None => Ok(()),
        }
    }
}

impl<T: TermControl> Drop for TermGuard<T> {
    fn drop(&mut self) {
        if let Some(mut term) = self.term.take() {
            let _ = term.leave();
        }
    }
}

fn enter_console() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, Hide)
}

fn leave_console() -> io::Result<()> {
    let shown = execute!(io::stdout(), Show, LeaveAlternateScreen);
    let raw = disable_raw_mode();
    shown.and(raw)
}

/// The real console: crossterm raw mode and the alternate screen.
struct ConsoleTerm;

impl TermControl for ConsoleTerm {
    fn enter(&mut self) -> io::Result<()> {
        enter_console()
    }

    fn leave(&mut self) -> io::Result<()> {
        leave_console()
    }
}

/// The board as one screen of the app: state, the data of the last good refresh, and the cache
/// that remembers resolved project roots.
pub(crate) struct BoardScreen {
    pub(crate) state: state::BoardState,
    pub(crate) data: BoardData,
    pub(crate) cache: RootCache,
    /// Text of the error note the last failed refresh left (cleared by the next good refresh).
    refresh_note: Option<String>,
    /// The last key ran an action, so the caller refreshes before drawing again.
    needs_refresh: bool,
}

/// What a key on the board screen did: stay on it, or leave it (`q` / `esc` / `ctrl+c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoardKeyResult {
    Stay,
    Leave,
}

impl BoardScreen {
    pub(crate) fn new(project: Option<ProjectKey>, theme: theme::ThemeName) -> Self {
        Self {
            state: initial_state(project, theme),
            data: BoardData::default(),
            cache: RootCache::new(),
            refresh_note: None,
            needs_refresh: false,
        }
    }

    pub(crate) fn theme(&self) -> theme::ThemeName {
        self.state.theme
    }

    /// Loads one frame; a failed refresh keeps the last good data and shows the error.
    pub(crate) fn refresh(&mut self, store: &Store, cfg: &Config, now_ms: i64) {
        let fresh = load_data(store, cfg, &mut self.cache, &mut self.state, now_ms);
        apply_refresh(
            &mut self.data,
            &mut self.state,
            fresh,
            &mut self.refresh_note,
        );
    }

    /// Whether the last key ran an action, so the caller should refresh before drawing again.
    pub(crate) fn needs_refresh(&self) -> bool {
        self.needs_refresh
    }

    pub(crate) fn draw(&self, frame: &mut Frame, palette: &theme::Theme, ascii: bool) {
        view::draw(frame, &self.state, &self.data, palette, ascii);
    }

    pub(crate) fn on_key(
        &mut self,
        key: event::KeyEvent,
        store: &Store,
        hc: &dyn Hc,
        now_ms: i64,
    ) -> BoardKeyResult {
        self.needs_refresh = false;
        let prev_agent = self.state.sel_agent.clone();
        let effect = self.state.on_key(key, &self.data);
        if let Effect::SwitchProject(key) = &effect {
            self.state.project = Some(key.clone());
        }
        match effect {
            Effect::None => {
                // The conversation belongs to the selected agent, so moving the selection must
                // reload it before the next frame is drawn.
                if self.state.sel_agent != prev_agent
                    && let Err(e) = reload_conversation(store, &self.state, &mut self.data)
                {
                    self.state.note = Some(Note {
                        ok: false,
                        text: format!("error: {e:#}"),
                    });
                }
            }
            Effect::Quit => return BoardKeyResult::Leave,
            other => {
                apply_effect(store, hc, other, &mut self.state, now_ms);
                self.needs_refresh = true;
            }
        }
        BoardKeyResult::Stay
    }
}

/// One refresh + input round trip. Separated from `run` so the guard and panic hook wrap it.
fn run_loop(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    refresh_s: u64,
    scan: &mut dyn FnMut(&Store) -> Result<()>,
    screen: &mut BoardScreen,
) -> Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let refresh = Duration::from_secs(refresh_s.max(1));
    let ascii = std::env::var("AGM_ASCII").as_deref() == Ok("1");

    loop {
        let _ = scan(store);
        screen.refresh(store, cfg, clock_ms());
        draw_board(&mut terminal, screen, ascii)?;

        let until = Instant::now() + refresh;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() || !event::poll(left)? {
                break;
            }
            match event::read()? {
                Event::Key(key) => match screen.on_key(key, store, hc, clock_ms()) {
                    BoardKeyResult::Leave => return Ok(()),
                    BoardKeyResult::Stay => {
                        // An action changed project data: refresh through the outer loop, exactly
                        // as the board did before it became a screen of the app.
                        if screen.needs_refresh() {
                            break;
                        }
                        draw_board(&mut terminal, screen, ascii)?;
                    }
                },
                Event::Resize(_, _) => draw_board(&mut terminal, screen, ascii)?,
                _ => {}
            }
        }
    }
}

fn draw_board(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    screen: &BoardScreen,
    ascii: bool,
) -> Result<()> {
    let palette = theme::theme(screen.theme());
    terminal.draw(|frame| screen.draw(frame, &palette, ascii))?;
    Ok(())
}

/// Without a project the board opens on the project picker, which every layout can show (the
/// Projects column is hidden below 120 columns).
pub(crate) fn initial_state(
    project: Option<ProjectKey>,
    theme_name: theme::ThemeName,
) -> state::BoardState {
    let mut state = state::BoardState::new(project, theme_name);
    if state.project.is_none() {
        state.mode = state::Mode::PickProject { idx: 0 };
    }
    state
}

/// Runs the board until the user quits. Caller must have passed `require_human`.
pub fn run(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    project: Option<ProjectKey>,
    refresh_s: u64,
    scan: &mut dyn FnMut(&Store) -> Result<()>,
) -> Result<()> {
    let theme_name = store
        .setting(theme::SETTING_KEY)?
        .and_then(|s| theme::ThemeName::parse(&s))
        .unwrap_or_default();
    let mut screen = BoardScreen::new(project, theme_name);
    with_console(|| run_loop(store, cfg, hc, refresh_s, scan, &mut screen))
}

/// Runs `body` on the alternate screen in raw mode and puts the console (and the panic hook) back
/// afterwards — on the normal path, on an error return, and on unwind.
pub(crate) fn with_console<F>(body: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let mut guard = TermGuard::new(ConsoleTerm);
    guard.enter()?;
    // A panic must not leave the console in raw mode on the alternate screen: leave it before the
    // previous hook reports the panic. The previous hook is put back on the normal path.
    let previous: Arc<dyn Fn(&PanicHookInfo<'_>) + Send + Sync> = Arc::from(panic::take_hook());
    let chained = Arc::clone(&previous);
    panic::set_hook(Box::new(move |info| {
        let _ = leave_console();
        chained(info);
    }));
    let outcome = body();
    let _ = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        previous(info);
    }));
    let restored = guard.leave();
    outcome?;
    restored?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct FakeTerm {
        calls: Rc<RefCell<Vec<&'static str>>>,
    }

    impl TermControl for FakeTerm {
        fn enter(&mut self) -> io::Result<()> {
            self.calls.borrow_mut().push("enter");
            Ok(())
        }

        fn leave(&mut self) -> io::Result<()> {
            self.calls.borrow_mut().push("leave");
            Ok(())
        }
    }

    #[test]
    fn initial_state_opens_picker_without_project() {
        let st = initial_state(None, theme::ThemeName::default());
        assert_eq!(st.mode, state::Mode::PickProject { idx: 0 });
        let key = ProjectKey {
            root: "d:/p/agmag".into(),
            name: "agmag".into(),
        };
        let st = initial_state(Some(key), theme::ThemeName::default());
        assert_eq!(st.mode, state::Mode::Normal);
    }

    #[test]
    fn guard_leaves_once_on_drop() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut guard = TermGuard::new(FakeTerm {
            calls: Rc::clone(&calls),
        });
        guard.enter().unwrap();
        drop(guard);
        assert_eq!(*calls.borrow(), vec!["enter", "leave"]);
    }

    #[test]
    fn guard_leaves_on_early_return() {
        fn fail_after_entering(guard: TermGuard<FakeTerm>) -> io::Result<()> {
            // e.g. `event::poll` failing right after the screen was taken over.
            Err(io::Error::other("boom"))?;
            drop(guard);
            Ok(())
        }

        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut guard = TermGuard::new(FakeTerm {
            calls: Rc::clone(&calls),
        });
        guard.enter().unwrap();
        let err = fail_after_entering(guard).unwrap_err();
        assert_eq!(err.to_string(), "boom");
        assert_eq!(*calls.borrow(), vec!["enter", "leave"]);
    }

    #[test]
    fn explicit_leave_is_not_repeated_on_drop() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut guard = TermGuard::new(FakeTerm {
            calls: Rc::clone(&calls),
        });
        guard.enter().unwrap();
        guard.leave().unwrap();
        drop(guard);
        assert_eq!(*calls.borrow(), vec!["enter", "leave"]);
    }

    fn board_agent(address: &str) -> crate::project::AgentView {
        crate::project::AgentView {
            address: address.into(),
            label: address.into(),
            app: "claude".into(),
            model: "opus".into(),
            effort: String::new(),
            status: "-".into(),
            state: "live".into(),
            depth: 0,
            last_sibling: false,
            doing: None,
            last_msg: None,
            context_pct: None,
            tokens: None,
            spark: None,
            stage: None,
            task: None,
            role: None,
        }
    }

    #[test]
    fn conversation_follows_selected_agent() -> Result<()> {
        let store = Store::open_in_memory()?;
        store.add_message(&MessageRow {
            ts_ms: 1,
            from: "a".into(),
            to: "user".into(),
            kind: "GIAO".into(),
            text: "for a".into(),
            ..MessageRow::default()
        })?;
        store.add_message(&MessageRow {
            ts_ms: 2,
            from: "b".into(),
            to: "user".into(),
            kind: "GIAO".into(),
            text: "for b".into(),
            ..MessageRow::default()
        })?;

        let mut data = BoardData {
            view: Some(ProjectView {
                key: ProjectKey {
                    root: "/repo/agmag".into(),
                    name: "agmag".into(),
                },
                agents: vec![board_agent("a"), board_agent("b")],
                inbox: Vec::new(),
                messages: Vec::new(),
            }),
            ..BoardData::default()
        };
        let mut state = state::BoardState::new(None, theme::ThemeName::default());
        state.sel_agent = Some("a".into());

        reload_conversation(&store, &state, &mut data)?;
        assert_eq!(data.conversation.len(), 1);
        assert_eq!(data.conversation[0].text, "for a");

        // Moving the selection must replace the conversation, never keep the old agent's.
        state.sel_agent = Some("b".into());
        reload_conversation(&store, &state, &mut data)?;
        assert_eq!(data.conversation.len(), 1);
        assert_eq!(data.conversation[0].text, "for b");
        Ok(())
    }

    #[test]
    fn refresh_error_note_clears_on_success() {
        let mut data = BoardData::default();
        let mut state = state::BoardState::new(None, theme::ThemeName::default());
        let mut refresh_note = None;

        apply_refresh(
            &mut data,
            &mut state,
            Err(anyhow::anyhow!("boom")),
            &mut refresh_note,
        );
        assert_eq!(
            state.note.as_ref().map(|note| note.text.as_str()),
            Some("error: boom")
        );

        apply_refresh(
            &mut data,
            &mut state,
            Ok(BoardData::default()),
            &mut refresh_note,
        );
        assert_eq!(
            state.note, None,
            "a refresh error outlived the next successful refresh"
        );

        // Action results are not refresh errors: a good refresh must not hide them.
        state.note = Some(Note {
            ok: true,
            text: "replied to #1".into(),
        });
        apply_refresh(
            &mut data,
            &mut state,
            Ok(BoardData::default()),
            &mut refresh_note,
        );
        assert!(
            state.note.is_some(),
            "a successful refresh hid an action result"
        );
    }

    /// Minimal `Hc` for keys that never call the host controller.
    struct NoHc;

    impl crate::hc::Hc for NoHc {
        fn ls(&self, _all: bool) -> Result<Vec<serde_json::Value>> {
            unreachable!("board key handling must not call hc")
        }

        fn get(&self, _terminal: &str) -> Result<serde_json::Value> {
            unreachable!("board key handling must not call hc")
        }

        fn send(
            &self,
            _terminal: &str,
            _text: &str,
            _if_idle: bool,
            _force: bool,
        ) -> Result<serde_json::Value> {
            unreachable!("board key handling must not call hc")
        }

        fn run(&self, _spec: &crate::hc::RunSpec) -> Result<serde_json::Value> {
            unreachable!("board key handling must not call hc")
        }

        fn kill(&self, _terminal: &str) -> Result<()> {
            unreachable!("board key handling must not call hc")
        }

        fn interrupt(&self, _terminal: &str) -> Result<()> {
            unreachable!("board key handling must not call hc")
        }
    }

    #[test]
    fn board_screen_leave_on_q() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
            KeyEvent::new(code, modifiers)
        }

        let store = Store::open_in_memory().unwrap();
        let hc = NoHc;
        let project = ProjectKey {
            root: "/repo/agmag".into(),
            name: "agmag".into(),
        };
        let mut screen = BoardScreen::new(Some(project), theme::ThemeName::default());

        // Navigation stays on the screen and needs no reload.
        let down = key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(
            screen.on_key(down, &store, &hc, 1_000),
            BoardKeyResult::Stay
        );
        assert!(!screen.needs_refresh());

        // `q` leaves the board screen (the app keeps running on the global screen).
        let q = key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(screen.on_key(q, &store, &hc, 1_000), BoardKeyResult::Leave);

        // `esc` does the same, as on the standalone board.
        let esc = key(KeyCode::Esc, KeyModifiers::NONE);
        let mut screen = BoardScreen::new(None, theme::ThemeName::default());
        // Without a project the board opens on the picker; `esc` there cancels the picker only.
        assert_eq!(screen.on_key(esc, &store, &hc, 1_000), BoardKeyResult::Stay);
    }
}
