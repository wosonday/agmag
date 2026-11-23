//! Full-screen project board (`agm board [project]`, `cd <project>` in `agm tui`).
//!
//! Pure parts: `flows` (edges between agents), `state` (keys → effects), `theme`, `view`
//! (ratatui rendering). This file owns all I/O: terminal guard, data loading, effects, loop.

pub mod flows;
pub mod state;
pub mod theme;
pub mod view;

use std::io::{self, IsTerminal};
use std::panic::{self, PanicHookInfo};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
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
trait TermControl {
    fn enter(&mut self) -> io::Result<()>;
    fn leave(&mut self) -> io::Result<()>;
}

/// Restores the terminal once: on the normal path, on an error return, and on unwind.
struct TermGuard<T: TermControl> {
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

/// Mutable board state carried through the loop (one parameter instead of three).
struct Session<'a> {
    state: &'a mut state::BoardState,
    cache: &'a mut RootCache,
    data: &'a mut BoardData,
}

/// One refresh + input round trip. Separated from `run` so the guard and panic hook wrap it.
fn run_loop(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    refresh_s: u64,
    scan: &mut dyn FnMut(&Store) -> Result<()>,
    session: &mut Session<'_>,
) -> Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    let refresh = Duration::from_secs(refresh_s.max(1));
    let ascii = std::env::var("AGM_ASCII").as_deref() == Ok("1");

    loop {
        let _ = scan(store);
        let Session { state, cache, data } = &mut *session;
        match load_data(store, cfg, cache, state, clock_ms()) {
            // A failed refresh keeps the last good data and shows the error in the footer.
            Ok(fresh) => **data = fresh,
            Err(e) => {
                state.note = Some(Note {
                    ok: false,
                    text: format!("error: {e:#}"),
                })
            }
        }
        let palette = theme::theme(state.theme);
        terminal.draw(|frame| view::draw(frame, state, data, &palette, ascii))?;

        let until = Instant::now() + refresh;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() || !event::poll(left)? {
                break;
            }
            match event::read()? {
                Event::Key(key) => {
                    let prev_agent = state.sel_agent.clone();
                    let effect = state.on_key(key, data);
                    if let Effect::SwitchProject(key) = &effect {
                        state.project = Some(key.clone());
                    }
                    match effect {
                        Effect::None => {
                            // The conversation belongs to the selected agent, so moving the
                            // selection must reload it before the next frame is drawn.
                            if state.sel_agent != prev_agent
                                && let Err(e) = reload_conversation(store, state, data)
                            {
                                state.note = Some(Note {
                                    ok: false,
                                    text: format!("error: {e:#}"),
                                });
                            }
                            terminal
                                .draw(|frame| view::draw(frame, state, data, &palette, ascii))?;
                        }
                        Effect::Quit => return Ok(()),
                        other => {
                            apply_effect(store, hc, other, state, clock_ms());
                            break;
                        }
                    }
                }
                Event::Resize(_, _) => {
                    terminal.draw(|frame| view::draw(frame, state, data, &palette, ascii))?;
                }
                _ => {}
            }
        }
    }
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
    let mut state = state::BoardState::new(project, theme_name);
    let mut cache = RootCache::new();
    let mut data = BoardData::default();

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
    let outcome = run_loop(
        store,
        cfg,
        hc,
        refresh_s,
        scan,
        &mut Session {
            state: &mut state,
            cache: &mut cache,
            data: &mut data,
        },
    );
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
}
