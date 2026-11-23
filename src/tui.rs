//! `agm tui`: periodically redrawn ANSI dashboard + short command prompt.
//! No heavy TUI library (offline build); keybindings are replaced by typed commands (reply, done, send, cleanup,
//! interrupt, kill, policy, quit), each with a single-letter alias.

use std::fmt::Write as _;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;

use agmag::actions::{self, Caller, CleanupCandidate};
use agmag::config::Config;
use agmag::dashboard::{
    Snapshot, bounded_styled_line, one_line, render_scoped, render_view, snapshot,
};
use agmag::hc::HcCli;
use agmag::project::{self, ProjectKey, RootCache};
use agmag::project_render;
use agmag::report::{compute_local_bounds, coverage_status, sqlite_offset_at};
use agmag::store::Store;

use agmag::board::cmd::{NoteKind, Outcome};

use crate::tui_cmd::{self, CmdState, Ctx, Front};

/// Clear the screen AND the scrollback, then home. The full frame is printed on the normal screen, so the part
/// above the pane stays reachable with the mouse wheel, and older frames never pile up above it.
const CLEAR_ALL: &[u8] = b"\x1b[2J\x1b[3J\x1b[H";

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|&n| n > 0)
}

/// Visible size of the console window (columns, rows), if stdout is a console.
#[cfg(windows)]
fn console_size() -> Option<(usize, usize)> {
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo, GetStdHandle, STD_OUTPUT_HANDLE,
    };
    // SAFETY: plain Win32 calls on the process's own stdout handle; `info` is a POD out-parameter.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
            return None;
        }
        let w = i32::from(info.srWindow.Right) - i32::from(info.srWindow.Left) + 1;
        let h = i32::from(info.srWindow.Bottom) - i32::from(info.srWindow.Top) + 1;
        (w > 0 && h > 0).then_some((w as usize, h as usize))
    }
}

#[cfg(unix)]
fn console_size() -> Option<(usize, usize)> {
    // SAFETY: TIOCGWINSZ only writes into the zeroed `winsize` we pass.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) != 0
            || ws.ws_col == 0
            || ws.ws_row == 0
        {
            return None;
        }
        Some((usize::from(ws.ws_col), usize::from(ws.ws_row)))
    }
}

#[cfg(not(any(windows, unix)))]
fn console_size() -> Option<(usize, usize)> {
    None
}

/// Pane width: `COLUMNS` overrides the detected width; 110 when unknown. Height is not used: the whole dashboard
/// is printed and the user scrolls.
pub(crate) fn pane_width() -> usize {
    env_usize("COLUMNS")
        .or(console_size().map(|r| r.0))
        .unwrap_or(110)
        .clamp(60, 200)
}

/// Whether to repaint: always after user input (the echoed line must be wiped), otherwise only when the frame
/// changed, so a user scrolled up to read is not pulled back to the bottom every refresh.
fn needs_redraw(frame: &str, last: Option<&str>, after_input: bool) -> bool {
    after_input || last != Some(frame)
}

pub(crate) fn confirmation_note(action: &str) -> String {
    format!("type y + Enter to confirm {action} (anything else cancels)")
}

fn render_frame(
    text: &str,
    w: usize,
    cleanup_list: Option<&[CleanupCandidate]>,
    note: Option<(NoteKind, &str)>,
    scope: Option<&str>,
) -> String {
    let max_w = w.saturating_sub(1);
    let mut body = String::with_capacity(text.len() + 512);
    body.push_str(text);
    if let Some(list) = cleanup_list {
        body.push_str(&crate::tui_cmd::cleanup_block(list, max_w));
    }
    if let Some((kind, msg)) = note {
        let color = match kind {
            NoteKind::Ok => "\x1b[92m",
            NoteKind::Warn => "\x1b[1;93m",
            NoteKind::Err => "\x1b[1;91m",
        };
        let clean = one_line(msg, max_w.saturating_sub(2));
        let _ = writeln!(body, "{color}» {clean}\x1b[0m");
    }
    // Move cursor home, clear trailing chars on each line, and clear below the frame in a single write.
    let mut frame = String::with_capacity(body.len() + 128);
    frame.push_str("\x1b[H");
    for line in body.split_inclusive('\n') {
        if let Some(stripped) = line.strip_suffix('\n') {
            frame.push_str(&bounded_styled_line(stripped, max_w));
            frame.push_str("\x1b[K\n");
        } else {
            frame.push_str(&bounded_styled_line(line, max_w));
        }
    }
    frame.push_str("\x1b[J\x1b[1;96m");
    match scope {
        // The project name comes from a directory name: sanitize it before painting.
        Some(name) => {
            let name = one_line(name, max_w.saturating_sub(7));
            let _ = write!(frame, "agm[{name}]>");
        }
        None => frame.push_str("agm>"),
    }
    frame.push_str("\x1b[0m ");
    frame
}

/// The body shown when the database cannot be read, shared by the global and scoped frames.
fn db_error_text(e: &anyhow::Error, w: usize) -> String {
    format!(
        "\x1b[1;91m{}\x1b[0m\n",
        one_line(
            &format!("failed to read database: {e:#}"),
            w.saturating_sub(1)
        )
    )
}

/// A scoped frame body: roots for every terminal of the project, then AGENTS/INBOX/MESSAGES.
fn scoped_text(
    store: &Store,
    snap: &Snapshot,
    key: &ProjectKey,
    cache: &mut RootCache,
    w: usize,
    now: i64,
) -> String {
    let terms = match store.terminals(false) {
        Ok(t) => t,
        Err(e) => return db_error_text(&e, w),
    };
    let roots = project::compute_roots(&terms, snap, cache, &project::git_common_root);
    match project::load_project_view(store, snap, key, &roots, now) {
        Ok(pv) => render_scoped(snap, &project_render::render_project_body(&pv, w, now), w),
        Err(e) => db_error_text(&e, w),
    }
}

/// Identifies the caller once, fail-closed: agents and piped stdin/stdout cannot use the TUI.
fn require_human(store: &Store, hc: &HcCli) -> Result<Caller> {
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let who = actions::detect_caller(store, hc, crate::ops::hc_terminal().as_deref(), tty, false);
    if !matches!(who, Caller::User { .. }) {
        anyhow::bail!(
            "agm tui is only available to a human at an interactive terminal (TTY); agents should use standard agm commands"
        );
    }
    Ok(who)
}

/// Backfills the configured recent calendar months on a background thread (separate Store since
/// Connection is !Sync).
fn start_backfill(
    store: &Store,
    db_path: &Path,
    cfg: &Config,
    backfill: impl FnOnce(&Store, i64) -> Result<()> + Send + 'static,
) {
    let now0 = crate::now_ms();
    let bounds0 = {
        let mut off = sqlite_offset_at(store.conn());
        compute_local_bounds(now0, cfg.policy.stats_months, &mut off)
    };
    if matches!(
        coverage_status(store, bounds0.four_months_start_ms),
        Ok((_, true))
    ) {
        let path_owned = db_path.to_path_buf();
        let since = bounds0.four_months_start_ms;
        std::thread::spawn(move || {
            if let Ok(bg_store) = Store::open(&path_owned) {
                let _ = backfill(&bg_store, since);
            }
        });
    }
}

/// Full-screen `agm tui` (ratatui): the global screen and the project board, commands through the
/// same dispatcher as `--plain`.
pub fn run_app(
    store: &Store,
    db_path: &Path,
    cfg: &Config,
    refresh: u64,
    mut scan: impl FnMut(&Store) -> Result<()>,
    backfill: impl FnOnce(&Store, i64) -> Result<()> + Send + 'static,
) -> Result<()> {
    let hc = HcCli::default();
    let who = require_human(store, &hc)?;
    start_backfill(store, db_path, cfg, backfill);
    let mut st = CmdState::new(store);
    let initial_view = st.view().to_vec();
    let mut dispatch = |line: &str| {
        let ctx = Ctx {
            store,
            cfg,
            hc: &hc,
            who: who.clone(),
            now_ms: crate::now_ms(),
        };
        tui_cmd::dispatch(line, &mut st, &ctx, Front::App)
    };
    agmag::board::app::run_app(
        store,
        cfg,
        &hc,
        refresh,
        &mut scan,
        initial_view,
        &mut dispatch,
    )
}

pub fn run(
    store: &Store,
    db_path: &Path,
    cfg: &Config,
    refresh: u64,
    mut scan: impl FnMut(&Store) -> Result<()>,
    backfill: impl FnOnce(&Store, i64) -> Result<()> + Send + 'static,
) -> Result<()> {
    // The reader hands one line over and then waits for `resume`, so no line read is pending while
    // the board owns the console (the board must not lose key events to a blocked stdin reader).
    let (tx, rx) = mpsc::channel::<String>();
    let (resume_tx, resume_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let input = stdin.lock();
        for line in input.lines().map_while(Result::ok) {
            if tx.send(line).is_err() || resume_rx.recv().is_err() {
                break;
            }
        }
    });
    let hc = HcCli::default();
    let who = require_human(store, &hc)?;
    start_backfill(store, db_path, cfg, backfill);

    let mut note: Option<(NoteKind, String)> = None;
    // `view`, `scope`, the pending confirmations and the printed candidate list all live in the
    // dispatcher now; the plain loop only keeps what it prints itself.
    let mut st = CmdState::new(store);
    let mut shown: Option<String> = None;

    // RAM/CPU/disk for the header and SYSTEM section; kept across frames because CPU % is a difference.
    let mut sampler = agmag::sysmon::SysSampler::new();

    let mut out = std::io::stdout();
    let mut last_frame: Option<String> = None;
    let mut after_input = true;

    loop {
        let _ = scan(store);
        let now = crate::now_ms();
        let w = pane_width();
        let scope = st.scope().cloned();
        let text = match snapshot(store, cfg, now) {
            Ok(mut s) => {
                s.system = sampler.as_mut().map(|m| m.sample());
                match &scope {
                    Some(key) => scoped_text(store, &s, key, st.roots_cache_mut(), w, now),
                    None => render_view(&s, w, st.view(), None),
                }
            }
            Err(e) => db_error_text(&e, w),
        };
        let mut text = text;
        if let Some(block) = &shown {
            text.push_str(block);
        }
        let frame = render_frame(
            &text,
            w,
            st.cleanup_list(),
            note.as_ref().map(|(k, s)| (*k, s.as_str())),
            scope.as_ref().map(|k| k.name.as_str()),
        );
        if needs_redraw(&frame, last_frame.as_deref(), after_input) {
            out.write_all(CLEAR_ALL)?;
            out.write_all(frame.as_bytes())?;
            out.flush()?;
            last_frame = Some(frame);
        }
        after_input = false;

        let line = match rx.recv_timeout(Duration::from_secs(refresh.max(1))) {
            Ok(l) => l,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        after_input = true;
        let now = crate::now_ms();
        note = None;
        shown = None;

        let ctx = Ctx {
            store,
            cfg,
            hc: &hc,
            who: who.clone(),
            now_ms: now,
        };
        match tui_cmd::dispatch(&line, &mut st, &ctx, Front::Plain).outcome {
            Outcome::Note(kind, text) => note = Some((kind, text)),
            Outcome::Shown(text) => {
                // `render_frame` draws the cleanup candidate list itself; never print it twice.
                if !(st.cleanup_list().is_some() && tui_cmd::is_cleanup_listing(&text)) {
                    shown = Some(text);
                }
            }
            Outcome::NeedsConfirm(prompt) => note = Some((NoteKind::Warn, prompt)),
            Outcome::OpenBoard(key) => {
                // The board owns the console until the user quits; the scope stays global.
                if let Err(e) = agmag::board::run(store, cfg, &hc, Some(key), refresh, &mut scan) {
                    note = Some((NoteKind::Err, format!("error: {e:#}")));
                }
                // The alternate screen is gone: repaint the whole text frame.
                last_frame = None;
                after_input = true;
            }
            Outcome::Quit => return Ok(()),
            Outcome::None => {}
        }
        // This line is fully handled (the board, if it ran, has given the console back): let the
        // reader fetch exactly one more line.
        let _ = resume_tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::{CLEAR_ALL, NoteKind, confirmation_note, needs_redraw, render_frame};

    #[test]
    fn repaint_only_on_change_or_after_input() {
        assert!(needs_redraw("a", None, false), "first frame");
        assert!(
            !needs_redraw("a", Some("a"), false),
            "unchanged frame must not yank a scrolled-up user"
        );
        assert!(needs_redraw("b", Some("a"), false), "changed frame");
        assert!(
            needs_redraw("a", Some("a"), true),
            "user input always repaints (wipes the echoed line)"
        );
    }

    #[test]
    fn clear_wipes_screen_and_scrollback_without_alternate_screen() {
        let clear = std::str::from_utf8(CLEAR_ALL).unwrap();
        assert!(
            clear.contains("\x1b[2J") && clear.contains("\x1b[3J") && clear.ends_with("\x1b[H")
        );
        assert!(
            !clear.contains("?1049"),
            "normal screen keeps mouse scrolling"
        );
    }
    use unicode_width::UnicodeWidthStr;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn prompt_shows_scope() {
        let scoped = render_frame("", 60, None, None, Some("shop"));
        let plain = strip_ansi(&scoped);
        assert!(
            plain
                .lines()
                .last()
                .is_some_and(|l| l.contains("agm[shop]> ")),
            "scoped prompt missing:\n{plain}"
        );

        let global = render_frame("", 60, None, None, None);
        let plain = strip_ansi(&global);
        assert!(
            plain.lines().last().is_some_and(|l| l.contains("agm> ")),
            "global prompt missing:\n{plain}"
        );
        assert!(!plain.contains("agm["), "{plain}");

        // A project name is untrusted (a directory): its escapes must render inert.
        let hostile = render_frame("", 60, None, None, Some("\x1b]0;pwn\x07shop\x1b[2J"));
        assert!(!hostile.contains("\x1b]0;"), "OSC must not reach the frame");
        assert!(!hostile.contains("\x1b[2J"), "CSI must not reach the frame");
        let plain = strip_ansi(&hostile);
        assert!(
            plain
                .lines()
                .last()
                .is_some_and(|l| l.contains("agm[shop]> ")),
            "{plain}"
        );
    }

    #[test]
    fn frame_note_strips_untrusted_escapes_and_bounds_wide_text() {
        let malicious = format!("\x1b[2J\x1b]52;c;c2VjcmV0\x07\x1b(B{}", "界👩‍💻".repeat(30));
        let frame = render_frame("", 60, None, Some((NoteKind::Err, &malicious)), None);
        assert!(!frame.contains("\x1b[2J"));
        assert!(!frame.contains("\x1b]52"));
        assert!(!frame.contains("\x1b(B"));
        let mut plain = String::new();
        let mut chars = frame.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            } else {
                plain.push(c);
            }
        }
        assert!(plain.lines().all(|line| line.width() < 60), "{plain}");
        assert!(plain.contains("agm>"));
    }

    #[test]
    fn confirmation_instruction_precedes_long_action_details() {
        let note = confirmation_note(&format!("kill {}", "long-target".repeat(30)));
        assert!(note.starts_with("type y + Enter to confirm kill "));
        let clipped = agmag::dashboard::one_line(&note, 57);
        assert!(clipped.starts_with("type y + Enter to confirm kill "));
        assert!(clipped.width() <= 57);
    }
}
