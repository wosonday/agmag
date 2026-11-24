//! One command dispatcher shared by `agm tui --plain` and the full-screen app.
//!
//! Both front ends send every typed line through [`dispatch`], so each command keeps one behaviour,
//! one set of arguments and one set of notes.

use std::fmt::Write as _;

use agmag::actions::{self, Caller, CleanupCandidate, eligible_cleanups, execute_cleanup_batch};
use agmag::board::cmd::{Dispatched, NoteKind, Outcome};
use agmag::config::Config;
use agmag::dashboard::{Section, Snapshot, TuiCmd, one_line, parse_command, parse_view, snapshot};
use agmag::hc::Hc;
use agmag::project::{self, CdMatch, ProjectKey, RootCache};
use agmag::project_render;
use agmag::store::Store;

use crate::tui::pane_width;

/// Settings key that remembers the sections chosen with `view`.
const VIEW_SETTING: &str = "tui_view";

/// Everything one line needs besides its text.
pub struct Ctx<'a> {
    pub store: &'a Store,
    pub cfg: &'a Config,
    pub hc: &'a dyn Hc,
    /// The caller identified once at start; `confirmed` is only raised inside `dispatch`.
    pub who: Caller,
    pub now_ms: i64,
}

/// Session state carried between command lines (moved from `tui::run`).
pub struct CmdState {
    view: Vec<Section>,
    scope: Option<ProjectKey>,
    pending: Option<TuiCmd>,
    cleanup_list: Option<Vec<CleanupCandidate>>,
    cleanup_selected: Option<Vec<CleanupCandidate>>,
    roots_cache: RootCache,
}

impl CmdState {
    pub fn new(store: &Store) -> Self {
        Self {
            view: load_view(store),
            scope: None,
            pending: None,
            cleanup_list: None,
            cleanup_selected: None,
            roots_cache: RootCache::new(),
        }
    }

    pub fn view(&self) -> &[Section] {
        &self.view
    }

    pub fn scope(&self) -> Option<&ProjectKey> {
        self.scope.as_ref()
    }

    /// The candidate list `cleanup` printed, still frozen until it runs or is cancelled.
    pub fn cleanup_list(&self) -> Option<&[CleanupCandidate]> {
        self.cleanup_list.as_deref()
    }

    /// The plain loop shares this cache with the dispatcher, exactly like `tui::run` did.
    pub(crate) fn roots_cache_mut(&mut self) -> &mut RootCache {
        &mut self.roots_cache
    }
}

/// Which front end asked; only the `cd` variants behave differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    Plain,
    App,
}

/// Runs one input line: a pending confirmation answer first, otherwise a parsed command.
pub fn dispatch(line: &str, st: &mut CmdState, ctx: &Ctx, front: Front) -> Dispatched {
    let outcome = outcome(line, st, ctx, front);
    Dispatched {
        outcome,
        view: st.view().to_vec(),
    }
}

/// The caller after a confirmation: only a human can be `confirmed`, and only from here.
fn confirmed_caller(ctx: &Ctx) -> Caller {
    match &ctx.who {
        Caller::User { .. } => Caller::User { confirmed: true },
        agent => agent.clone(),
    }
}

/// The body of `dispatch`, borrowing `st` only for one outcome.
fn outcome(line: &str, st: &mut CmdState, ctx: &Ctx, front: Front) -> Outcome {
    // A pending cleanup confirmation: this line is the answer, never a new command.
    if let Some(selected) = st.cleanup_selected.take() {
        if line.trim() == "y" {
            let user = confirmed_caller(ctx);
            return match execute_cleanup_batch(ctx.store, ctx.hc, &user, &selected, ctx.now_ms) {
                Ok(report) => {
                    st.cleanup_list = None;
                    let kind = if !report.errors.is_empty() {
                        NoteKind::Err
                    } else if !report.skipped.is_empty() {
                        NoteKind::Warn
                    } else {
                        NoteKind::Ok
                    };
                    Outcome::Note(
                        kind,
                        format!(
                            "cleaned {}, skipped {}, errors {}",
                            report.done,
                            report.skipped.len(),
                            report.errors.len()
                        ),
                    )
                }
                Err(e) => Outcome::Note(NoteKind::Err, format!("cleanup error: {e:#}")),
            };
        }
        st.cleanup_list = None;
        return Outcome::Note(NoteKind::Warn, "cleanup cancelled".into());
    }

    // Kill/interrupt: the prior command set `pending`; this line must be "y".
    if let Some(cmd) = st.pending.take() {
        if line.trim() == "y" {
            let user = confirmed_caller(ctx);
            let res = match &cmd {
                TuiCmd::Kill(t) => actions::kill(ctx.store, ctx.hc, &user, t, ctx.now_ms)
                    .map(|()| format!("killed {t}")),
                TuiCmd::Interrupt(t) => actions::interrupt(ctx.store, ctx.hc, &user, t, ctx.now_ms)
                    .map(|()| format!("interrupted {t}")),
                _ => Ok(String::new()),
            };
            return match res {
                Ok(msg) => Outcome::Note(NoteKind::Ok, msg),
                Err(e) => Outcome::Note(NoteKind::Err, format!("error: {e:#}")),
            };
        }
        return Outcome::Note(NoteKind::Warn, "cancelled".into());
    }

    let width = pane_width();
    match parse_command(line) {
        Ok(TuiCmd::Quit) => Outcome::Quit,
        Ok(TuiCmd::Refresh) => Outcome::None,
        Ok(TuiCmd::Cleanup(arg)) => cleanup_command(arg, st, ctx, width.saturating_sub(1)),
        Ok(cmd @ (TuiCmd::Kill(_) | TuiCmd::Interrupt(_))) => {
            let prompt_desc = match &cmd {
                TuiCmd::Kill(t) => format!("kill {t}"),
                TuiCmd::Interrupt(t) => format!("interrupt {t}"),
                _ => unreachable!(),
            };
            st.pending = Some(cmd);
            Outcome::NeedsConfirm(crate::tui::confirmation_note(&prompt_desc))
        }
        Ok(TuiCmd::Reply(id, text)) => {
            let (kind, msg) = match actions::reply(ctx.store, ctx.hc, id, &text, ctx.now_ms) {
                Ok(()) => (NoteKind::Ok, format!("replied to #{id}")),
                Err(e) => (NoteKind::Err, format!("error: {e:#}")),
            };
            Outcome::Note(kind, msg)
        }
        Ok(TuiCmd::Send(to, kind, text)) => {
            let (note_kind, msg) = match actions::send(
                ctx.store, ctx.hc, "user", &to, &kind, None, &text, ctx.now_ms,
            ) {
                Ok(m) => (
                    NoteKind::Ok,
                    format!("msg #{} → {}: {}", m.id, m.to, m.status),
                ),
                Err(e) => (NoteKind::Err, format!("error: {e:#}")),
            };
            Outcome::Note(note_kind, msg)
        }
        Ok(TuiCmd::Done {
            ids,
            filters,
            explicit_source_blocked,
            legacy_single,
        }) => {
            let (kind, msg) = match actions::dismiss_done(
                ctx.store,
                ids,
                filters,
                explicit_source_blocked,
                legacy_single,
                ctx.now_ms,
            ) {
                Ok(report) => {
                    if report.done.is_empty() && report.skipped.is_empty() {
                        (NoteKind::Warn, "Nothing to dismiss.".into())
                    } else {
                        (
                            NoteKind::Ok,
                            format!(
                                "dismissed {} (skipped {})",
                                report.done.len(),
                                report.skipped.len()
                            ),
                        )
                    }
                }
                Err(e) => (NoteKind::Err, format!("error: {e:#}")),
            };
            Outcome::Note(kind, msg)
        }
        Ok(TuiCmd::Policy(on)) => {
            // Never swallow errors: the user must know whether policy actually toggled.
            let (kind, msg) = match ctx
                .store
                .set_setting("policy", if on { "on" } else { "off" })
            {
                Ok(()) => (
                    if on { NoteKind::Ok } else { NoteKind::Warn },
                    format!("policy {}", if on { "ON" } else { "OFF" }),
                ),
                Err(e) => (NoteKind::Err, format!("ERROR — policy NOT changed: {e:#}")),
            };
            Outcome::Note(kind, msg)
        }
        Ok(TuiCmd::View(None)) => Outcome::Note(
            NoteKind::Ok,
            format!(
                "view: {} (choose up to 3 of {}; `view default`)",
                view_names(&st.view),
                Section::names()
            ),
        ),
        Ok(TuiCmd::View(Some(v))) => {
            st.view = v;
            let (kind, msg) = match ctx.store.set_setting(VIEW_SETTING, &view_names(&st.view)) {
                Ok(()) => (NoteKind::Ok, format!("view: {}", view_names(&st.view))),
                Err(e) => (
                    NoteKind::Warn,
                    format!("view: {} (not saved: {e:#})", view_names(&st.view)),
                ),
            };
            Outcome::Note(kind, msg)
        }
        Ok(TuiCmd::Cd(None)) => match snapshot(ctx.store, ctx.cfg, ctx.now_ms) {
            Ok(s) => match (ctx.store.terminals(false), ctx.store.inbox(true)) {
                (Ok(terms), Ok(inbox)) => {
                    let roots = project::compute_roots(
                        &terms,
                        &s,
                        &mut st.roots_cache,
                        &project::git_common_root,
                    );
                    let (all, no_project) = project::list_projects(&terms, &inbox, &roots);
                    let (list, idle_hidden) = project::split_idle(all, None);
                    Outcome::Shown(project_render::render_project_list(
                        &list,
                        no_project,
                        idle_hidden,
                        width,
                    ))
                }
                (Err(e), _) | (_, Err(e)) => Outcome::Note(NoteKind::Err, format!("error: {e:#}")),
            },
            Err(_) => Outcome::Note(NoteKind::Err, "failed to read database".into()),
        },
        Ok(TuiCmd::Cd(Some(q))) => {
            if q == ".." || q == "/" {
                let had_scope = st.scope.take().is_some();
                let text = if had_scope || front == Front::Plain {
                    "back to all projects"
                } else {
                    "already at all projects"
                };
                Outcome::Note(NoteKind::Ok, text.into())
            } else {
                let snap = snapshot(ctx.store, ctx.cfg, ctx.now_ms).ok();
                match lookup_project(ctx.store, snap.as_ref(), &q, &mut st.roots_cache) {
                    Ok(key) => Outcome::OpenBoard(key),
                    Err((kind, text)) => Outcome::Note(kind, text),
                }
            }
        }
        Ok(TuiCmd::CdText(q)) => {
            if front == Front::App {
                return Outcome::Note(
                    NoteKind::Warn,
                    "text project view: use `agm tui --plain`".into(),
                );
            }
            if q == ".." || q == "/" {
                st.scope = None;
                Outcome::Note(NoteKind::Ok, "back to all projects".into())
            } else {
                let snap = snapshot(ctx.store, ctx.cfg, ctx.now_ms).ok();
                match lookup_project(ctx.store, snap.as_ref(), &q, &mut st.roots_cache) {
                    // Entering keeps the scope even if the project has no members left.
                    Ok(key) => {
                        let name = key.name.clone();
                        st.scope = Some(key);
                        Outcome::Note(NoteKind::Ok, format!("project: {name}"))
                    }
                    Err((kind, text)) => Outcome::Note(kind, text),
                }
            }
        }
        Ok(TuiCmd::Show(id)) => match ctx.store.inbox_item(id) {
            Ok(Some(item)) => Outcome::Shown(project_render::render_inbox_item(&item, width)),
            Ok(None) => Outcome::Note(NoteKind::Err, format!("no inbox item #{id}")),
            Err(e) => Outcome::Note(NoteKind::Err, format!("error: {e:#}")),
        },
        Err(e) => Outcome::Note(NoteKind::Err, e),
    }
}

/// `cleanup` with no selection lists candidates; with one it freezes them and asks for `y`.
fn cleanup_command(arg: Option<String>, st: &mut CmdState, ctx: &Ctx, max_w: usize) -> Outcome {
    match arg {
        Some(sel_expr) => {
            let list = match st.cleanup_list.take() {
                Some(l) => l,
                None => eligible_cleanups(ctx.store).unwrap_or_default(),
            };
            if list.is_empty() {
                return Outcome::Note(
                    NoteKind::Warn,
                    "no handed-off sessions ready for cleanup".into(),
                );
            }
            match crate::ops::parse_selection(&sel_expr, list.len()) {
                Ok(indices) if !indices.is_empty() => {
                    let chosen: Vec<CleanupCandidate> =
                        indices.into_iter().map(|i| list[i].clone()).collect();
                    let items: Vec<String> = chosen
                        .iter()
                        .map(|c| {
                            format!(
                                "{}→{}",
                                c.old_terminal,
                                c.new_terminal.as_deref().unwrap_or("-")
                            )
                        })
                        .collect();
                    let prompt = crate::tui::confirmation_note(&format!(
                        "cleanup of {} session(s) ({})",
                        chosen.len(),
                        items.join(", ")
                    ));
                    st.cleanup_selected = Some(chosen);
                    Outcome::NeedsConfirm(prompt)
                }
                Ok(_) => Outcome::Note(NoteKind::Warn, "no items selected".into()),
                Err(e) => {
                    st.cleanup_list = Some(list);
                    Outcome::Note(NoteKind::Err, format!("selection error: {e:#}"))
                }
            }
        }
        None => match eligible_cleanups(ctx.store) {
            Ok(list) if list.is_empty() => {
                st.cleanup_list = None;
                Outcome::Note(
                    NoteKind::Warn,
                    "no handed-off sessions ready for cleanup".into(),
                )
            }
            Ok(list) => {
                let block = cleanup_block(&list, max_w);
                st.cleanup_list = Some(list);
                Outcome::Shown(block)
            }
            Err(e) => Outcome::Note(NoteKind::Err, format!("error: {e:#}")),
        },
    }
}

fn load_view(store: &Store) -> Vec<Section> {
    store
        .setting(VIEW_SETTING)
        .ok()
        .flatten()
        .and_then(|v| parse_view(&v).ok())
        .unwrap_or_else(|| Section::DEFAULT.to_vec())
}

fn view_names(view: &[Section]) -> String {
    view.iter().map(|s| s.name()).collect::<Vec<_>>().join(" ")
}

/// Resolves a `cd` / `cd -t` query against the current project list. Errors carry the note kind the
/// text UI shows (`ambiguous` is a warning, unknown is an error), as before the board existed.
fn lookup_project(
    store: &Store,
    snap: Option<&Snapshot>,
    query: &str,
    cache: &mut RootCache,
) -> std::result::Result<ProjectKey, (NoteKind, String)> {
    let Some(s) = snap else {
        return Err((NoteKind::Err, "failed to read database".into()));
    };
    let inbox = store
        .inbox(true)
        .map_err(|e| (NoteKind::Err, format!("error: {e:#}")))?;
    let terms = store
        .terminals(false)
        .map_err(|e| (NoteKind::Err, format!("error: {e:#}")))?;
    let roots = project::compute_roots(&terms, s, cache, &project::git_common_root);
    let (list, _) = project::list_projects(&terms, &inbox, &roots);
    match project::match_project(&list, query) {
        CdMatch::Found(key) => Ok(key),
        CdMatch::Ambiguous(candidates) => Err((
            NoteKind::Warn,
            format!("ambiguous: {}", candidates.join(", ")),
        )),
        CdMatch::NotFound => Err((NoteKind::Err, format!("no project matching '{query}'"))),
    }
}

/// First characters of the candidate-list header (with its bold-yellow SGR), before any clipping.
const CLEANUP_HEADER: &str = "\x1b[1;93m── MANUAL CLEANUP";

/// Whether `text` is the candidate list `cleanup` prints (the plain frame draws it itself).
pub(crate) fn is_cleanup_listing(text: &str) -> bool {
    text.starts_with(CLEANUP_HEADER)
}

/// The header + numbered candidates block of a `cleanup` listing, as `--plain` always drew it.
pub(crate) fn cleanup_block(list: &[CleanupCandidate], max_w: usize) -> String {
    let hdr = one_line(
        "── MANUAL CLEANUP (enter 'cleanup 1,2' or 'cleanup all'; Enter to refresh) ──",
        max_w,
    );
    let mut out = String::new();
    let _ = writeln!(out, "\x1b[1;93m{hdr}\x1b[0m");
    for (i, c) in list.iter().enumerate() {
        let state = if c.old_live { "live" } else { "exited" };
        let old = one_line(&c.old_terminal, 24);
        let succ = one_line(
            c.new_terminal.as_deref().unwrap_or("-"),
            max_w.saturating_sub(old.chars().count() + 18),
        );
        let _ = writeln!(
            out,
            "  \x1b[1;93m#{}\x1b[0m {old} ({state}) \x1b[90m→\x1b[0m \x1b[96m{succ}\x1b[0m",
            i + 1
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use agmag::hc::{Hc, RunSpec};
    use agmag::model::Source;
    use agmag::store::{Store, TerminalRow};
    use serde_json::{Value, json};

    use super::*;

    /// hc giả: records every mutating call and answers `get` for `kill`/`interrupt` authorization.
    #[derive(Default)]
    struct FakeHc {
        calls: RefCell<Vec<String>>,
    }

    impl Hc for FakeHc {
        fn ls(&self, _all: bool) -> anyhow::Result<Vec<Value>> {
            Ok(Vec::new())
        }
        fn get(&self, terminal: &str) -> anyhow::Result<Value> {
            Ok(json!({"address": terminal}))
        }
        fn send(
            &self,
            terminal: &str,
            text: &str,
            _if_idle: bool,
            _force: bool,
        ) -> anyhow::Result<Value> {
            self.calls
                .borrow_mut()
                .push(format!("send {terminal} {text}"));
            Ok(json!({"id": 1, "to": terminal, "status": "sent"}))
        }
        fn run(&self, _spec: &RunSpec) -> anyhow::Result<Value> {
            Ok(json!({"address": "t99"}))
        }
        fn kill(&self, terminal: &str) -> anyhow::Result<()> {
            self.calls.borrow_mut().push(format!("kill {terminal}"));
            Ok(())
        }
        fn interrupt(&self, terminal: &str) -> anyhow::Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("interrupt {terminal}"));
            Ok(())
        }
    }

    fn user() -> Caller {
        Caller::User { confirmed: false }
    }

    fn run(
        line: &str,
        st: &mut CmdState,
        store: &Store,
        cfg: &Config,
        hc: &FakeHc,
        who: &Caller,
        front: Front,
    ) -> Dispatched {
        let ctx = Ctx {
            store,
            cfg,
            hc,
            who: who.clone(),
            now_ms: 1_000,
        };
        dispatch(line, st, &ctx, front)
    }

    fn term(address: &str, name: &str, cwd: &str) -> TerminalRow {
        TerminalRow {
            address: address.into(),
            name: Some(name.into()),
            cwd: Some(cwd.into()),
            state: "live".into(),
            created_ms: 1,
            ..Default::default()
        }
    }

    /// A store with one live terminal whose project is a temp directory called `shop`.
    fn shop_store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("shop");
        std::fs::create_dir_all(&dir).expect("shop dir");
        let store = Store::open_in_memory().expect("store");
        store
            .upsert_terminal(&term("t1", "one", dir.to_str().expect("utf-8 path")))
            .expect("terminal");
        (tmp, store)
    }

    /// A store with one eligible cleanup pair `t1 → t2`.
    fn cleanup_store() -> Store {
        let store = Store::open_in_memory().expect("store");
        add_cleanup_pair(&store, "t1", "t2", 1);
        store
    }

    fn add_cleanup_pair(store: &Store, old: &str, new: &str, rotation_id: i64) {
        for address in [old, new] {
            store
                .upsert_terminal(&TerminalRow {
                    address: address.into(),
                    app: Some("codex".into()),
                    state: "live".into(),
                    created_ms: 1,
                    ..Default::default()
                })
                .expect("terminal");
            let sid = store
                .session_id_for_path(Source::Codex, &format!("codex://{address}"))
                .expect("session");
            store.link_terminal(address, sid, "rotation").expect("link");
        }
        store
            .set_terminal_handoff_state(old, "handed_off")
            .expect("handoff");
        store
            .conn()
            .execute(
                "INSERT INTO rotations (id,old_terminal,new_terminal,app,state,updated_ms) VALUES (?1,?2,?3,'codex','done',1)",
                rusqlite::params![rotation_id, old, new],
            )
            .expect("rotation");
    }

    #[test]
    fn cd_list_hides_idle_projects_and_reports_them() {
        let (tmp, store) = shop_store();
        let ghost_dir = tmp.path().join("ghost");
        std::fs::create_dir_all(&ghost_dir).expect("ghost dir");
        store
            .upsert_terminal(&TerminalRow {
                address: "t2".into(),
                name: Some("ghost".into()),
                cwd: Some(ghost_dir.to_str().expect("utf-8 path").into()),
                state: "exited".into(),
                created_ms: 2,
                ..Default::default()
            })
            .expect("terminal");

        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        let d = run("cd", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        match &d.outcome {
            Outcome::Shown(text) => {
                assert!(text.contains("shop"), "{text}");
                assert!(!text.contains("ghost"), "{text}");
                assert!(
                    text.contains("1 idle project(s) hidden — cd <name> still opens them"),
                    "{text}"
                );
            }
            other => panic!("expected Shown, got {other:?}"),
        }
    }

    #[test]
    fn kill_needs_confirm_then_y_kills() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        let d = run("kill t1", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        match &d.outcome {
            Outcome::NeedsConfirm(prompt) => assert!(prompt.contains("kill t1"), "{prompt}"),
            other => panic!("expected NeedsConfirm, got {other:?}"),
        }
        assert!(hc.calls.borrow().is_empty(), "nothing runs before `y`");

        let d = run("y", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(d.outcome, Outcome::Note(NoteKind::Ok, "killed t1".into()));
        assert!(
            hc.calls.borrow().iter().any(|c| c == "kill t1"),
            "{:?}",
            hc.calls.borrow()
        );
    }

    #[test]
    fn kill_confirm_other_input_cancels() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        run("kill t1", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        let d = run("n", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(d.outcome, Outcome::Note(NoteKind::Warn, "cancelled".into()));
        assert!(hc.calls.borrow().is_empty(), "{:?}", hc.calls.borrow());
    }

    #[test]
    fn agent_caller_never_confirms() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = Caller::Agent(Some("t0".into()));
        let mut st = CmdState::new(&store);

        let d = run("kill t1", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert!(
            matches!(d.outcome, Outcome::NeedsConfirm(_)),
            "{:?}",
            d.outcome
        );

        let d = run("y", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        match &d.outcome {
            Outcome::Note(NoteKind::Err, text) => {
                assert!(text.starts_with("error: "), "{text}");
                assert!(text.contains("chỉ người dùng"), "{text}");
            }
            other => panic!("expected an error note, got {other:?}"),
        }
        assert!(
            !hc.calls.borrow().iter().any(|c| c.starts_with("kill ")),
            "an agent must never get a kill through: {:?}",
            hc.calls.borrow()
        );
    }

    #[test]
    fn cleanup_two_step_and_stale_list() {
        let store = cleanup_store();
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        let d = run("cleanup", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        let listing = match &d.outcome {
            Outcome::Shown(text) => text.clone(),
            other => panic!("expected the candidate list, got {other:?}"),
        };
        assert!(listing.contains("#1"), "{listing}");
        assert!(
            listing.contains("t1") && listing.contains("t2"),
            "{listing}"
        );
        assert!(is_cleanup_listing(&listing), "{listing}");
        assert_eq!(st.cleanup_list().map(|l| l.len()), Some(1));

        // Candidates change after the list was shown: `cleanup 1` keeps the frozen list, like `--plain`.
        add_cleanup_pair(&store, "t3", "t4", 2);
        let d = run("cleanup 1", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        match &d.outcome {
            Outcome::NeedsConfirm(prompt) => {
                assert!(prompt.contains("cleanup of 1 session(s)"), "{prompt}");
                assert!(prompt.contains("t1→t2"), "{prompt}");
                assert!(!prompt.contains("t3"), "list was not frozen: {prompt}");
            }
            other => panic!("expected NeedsConfirm, got {other:?}"),
        }

        let d = run("x", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(
            d.outcome,
            Outcome::Note(NoteKind::Warn, "cleanup cancelled".into())
        );
        assert!(st.cleanup_list().is_none());
        assert!(hc.calls.borrow().is_empty(), "{:?}", hc.calls.borrow());
    }

    #[test]
    fn view_sets_and_returns_view() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        assert_eq!(st.view(), &Section::DEFAULT);
        let d = run(
            "view quota inbox",
            &mut st,
            &store,
            &cfg,
            &hc,
            &who,
            Front::Plain,
        );
        assert_eq!(d.view, vec![Section::Quota, Section::Inbox]);
        assert_eq!(
            d.outcome,
            Outcome::Note(NoteKind::Ok, "view: quota inbox".into())
        );
        assert_eq!(
            store.setting(VIEW_SETTING).expect("setting").as_deref(),
            Some("quota inbox")
        );

        // Every later line carries the current view too.
        let d = run("refresh", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(d.view, vec![Section::Quota, Section::Inbox]);
    }

    #[test]
    fn policy_toggles() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        let d = run("policy on", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(d.outcome, Outcome::Note(NoteKind::Ok, "policy ON".into()));
        assert_eq!(
            store.setting("policy").expect("setting").as_deref(),
            Some("on")
        );

        let d = run("policy off", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(
            d.outcome,
            Outcome::Note(NoteKind::Warn, "policy OFF".into())
        );
        assert_eq!(
            store.setting("policy").expect("setting").as_deref(),
            Some("off")
        );
    }

    #[test]
    fn cd_found_opens_board_both_fronts() {
        let (_tmp, store) = shop_store();
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();

        for front in [Front::Plain, Front::App] {
            let mut st = CmdState::new(&store);
            let d = run("cd shop", &mut st, &store, &cfg, &hc, &who, front);
            match d.outcome {
                Outcome::OpenBoard(key) => assert_eq!(key.name, "shop"),
                other => panic!("{front:?}: expected OpenBoard, got {other:?}"),
            }
        }
    }

    /// Idle projects are hidden from the `cd` listing but `cd <name>` must still open them.
    #[test]
    fn cd_name_still_opens_an_idle_project() {
        let (tmp, store) = shop_store();
        let ghost_dir = tmp.path().join("ghost");
        std::fs::create_dir_all(&ghost_dir).expect("ghost dir");
        store
            .upsert_terminal(&TerminalRow {
                address: "t2".into(),
                name: Some("ghost".into()),
                cwd: Some(ghost_dir.to_str().expect("utf-8 path").into()),
                state: "exited".into(),
                created_ms: 2,
                ..Default::default()
            })
            .expect("terminal");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);
        let d = run("cd ghost", &mut st, &store, &cfg, &hc, &who, Front::App);
        match d.outcome {
            Outcome::OpenBoard(key) => assert_eq!(key.name, "ghost"),
            other => panic!("expected OpenBoard, got {other:?}"),
        }
    }

    #[test]
    fn cd_t_plain_scopes_app_warns() {
        let (_tmp, store) = shop_store();
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();

        let mut st = CmdState::new(&store);
        let d = run("cd -t shop", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(
            d.outcome,
            Outcome::Note(NoteKind::Ok, "project: shop".into())
        );
        assert_eq!(st.scope().map(|k| k.name.as_str()), Some("shop"));

        let mut st = CmdState::new(&store);
        let d = run("cd -t shop", &mut st, &store, &cfg, &hc, &who, Front::App);
        assert_eq!(
            d.outcome,
            Outcome::Note(
                NoteKind::Warn,
                "text project view: use `agm tui --plain`".into()
            )
        );
        assert!(st.scope().is_none());
    }

    #[test]
    fn cd_up_notes_all_projects() {
        let (_tmp, store) = shop_store();
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();

        // Plain keeps today's wording, with or without a scope.
        let mut st = CmdState::new(&store);
        let d = run("cd ..", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(
            d.outcome,
            Outcome::Note(NoteKind::Ok, "back to all projects".into())
        );

        // The app has no text scope, so it says so.
        let mut st = CmdState::new(&store);
        let d = run("cd /", &mut st, &store, &cfg, &hc, &who, Front::App);
        assert_eq!(
            d.outcome,
            Outcome::Note(NoteKind::Ok, "already at all projects".into())
        );
    }

    #[test]
    fn unknown_command_is_err_note() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        let d = run("frobnicate", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        match d.outcome {
            Outcome::Note(NoteKind::Err, text) => {
                assert!(text.starts_with("unknown command 'frobnicate'"), "{text}");
            }
            other => panic!("expected an error note, got {other:?}"),
        }
    }

    #[test]
    fn quit_is_quit() {
        let store = Store::open_in_memory().expect("store");
        let cfg = Config::builtin();
        let hc = FakeHc::default();
        let who = user();
        let mut st = CmdState::new(&store);

        let d = run("quit", &mut st, &store, &cfg, &hc, &who, Front::Plain);
        assert_eq!(d.outcome, Outcome::Quit);
    }
}
