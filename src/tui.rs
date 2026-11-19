//! `agm tui`: periodically redrawn ANSI dashboard + short command prompt.
//! No heavy TUI library (offline build); keybindings are replaced by single-letter commands: r d s c i k p q.

use std::fmt::Write as _;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;

use agmag::actions::{self, eligible_cleanups, execute_cleanup_batch, Caller, CleanupCandidate};
use agmag::config::Config;
use agmag::dashboard::{parse_command, render, snapshot, TuiCmd};
use agmag::hc::HcCli;
use agmag::report::{compute_local_bounds, coverage_status, sqlite_offset_at};
use agmag::store::Store;

#[derive(Clone, Copy)]
enum NoteKind {
    Ok,
    Warn,
    Err,
}

fn width() -> usize {
    std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(110).clamp(60, 200)
}

fn render_frame(text: &str, cleanup_list: Option<&[CleanupCandidate]>, note: Option<(NoteKind, &str)>) -> String {
    let mut body = String::with_capacity(text.len() + 512);
    body.push_str(text);
    if let Some(list) = cleanup_list {
        body.push_str("\x1b[1;93m── MANUAL CLEANUP (enter 'c 1,2' or 'c all'; Enter to refresh) ──\x1b[0m\n");
        for (i, c) in list.iter().enumerate() {
            let st = if c.old_live { "live" } else { "exited" };
            let succ = c.new_terminal.as_deref().unwrap_or("-");
            let _ = writeln!(body, "  \x1b[1;93m#{}\x1b[0m {} ({st}) \x1b[90m→\x1b[0m \x1b[96m{succ}\x1b[0m", i + 1, c.old_terminal);
        }
    }
    if let Some((kind, msg)) = note {
        let color = match kind {
            NoteKind::Ok => "\x1b[92m",
            NoteKind::Warn => "\x1b[1;93m",
            NoteKind::Err => "\x1b[1;91m",
        };
        let _ = writeln!(body, "{color}» {msg}\x1b[0m");
    }
    // Move cursor home, clear trailing chars on each line, and clear below the frame in a single write.
    let mut frame = String::with_capacity(body.len() + 128);
    frame.push_str("\x1b[H");
    for line in body.split_inclusive('\n') {
        if let Some(stripped) = line.strip_suffix('\n') {
            frame.push_str(stripped);
            frame.push_str("\x1b[K\n");
        } else {
            frame.push_str(line);
        }
    }
    frame.push_str("\x1b[J\x1b[1;96magm>\x1b[0m ");
    frame
}

pub fn run(
    store: &Store,
    db_path: &Path,
    cfg: &Config,
    refresh: u64,
    mut scan: impl FnMut(&Store) -> Result<()>,
    backfill: impl FnOnce(&Store, i64) -> Result<()> + Send + 'static,
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let hc = HcCli::default();
    // Identify caller once, fail-closed. Agents (or piped stdin/stdout) cannot kill/interrupt via the TUI.
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let who = actions::detect_caller(
        store,
        &hc,
        crate::ops::hc_terminal().as_deref(),
        tty,
        false,
    );
    if !matches!(who, Caller::User { .. }) {
        anyhow::bail!("agm tui is only available to a human at an interactive terminal (TTY); agents should use standard agm commands");
    }

    // Backfill the configured recent calendar months on a background thread (separate Store since Connection is !Sync).
    let now0 = crate::now_ms();
    let bounds0 = {
        let mut off = sqlite_offset_at(store.conn());
        compute_local_bounds(now0, cfg.policy.stats_months, &mut off)
    };
    if matches!(coverage_status(store, bounds0.four_months_start_ms), Ok((_, true))) {
        let path_owned = db_path.to_path_buf();
        let since = bounds0.four_months_start_ms;
        std::thread::spawn(move || {
            if let Ok(bg_store) = Store::open(&path_owned) {
                let _ = backfill(&bg_store, since);
            }
        });
    }

    let mut note: Option<(NoteKind, String)> = None;
    let mut pending: Option<TuiCmd> = None;
    let mut cleanup_list: Option<Vec<CleanupCandidate>> = None;
    let mut cleanup_selected: Option<Vec<CleanupCandidate>> = None;

    let mut out = std::io::stdout();
    // Initial full clear before the first home-positioned frame.
    let _ = out.write_all(b"\x1b[2J");

    loop {
        let _ = scan(store);
        let now = crate::now_ms();
        let text = match snapshot(store, cfg, now) {
            Ok(s) => render(&s, width()),
            Err(e) => format!("\x1b[1;91mfailed to read database: {e:#}\x1b[0m\n"),
        };
        let frame = render_frame(
            &text,
            cleanup_list.as_deref(),
            note.as_ref().map(|(k, s)| (*k, s.as_str())),
        );
        out.write_all(frame.as_bytes())?;
        out.flush()?;

        let line = match rx.recv_timeout(Duration::from_secs(refresh.max(1))) {
            Ok(l) => l,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        let now = crate::now_ms();
        note = None;

        if let Some(sel) = cleanup_selected.take() {
            if line.trim() == "y" {
                let user = match &who {
                    Caller::User { .. } => Caller::User { confirmed: true },
                    agent => agent.clone(),
                };
                note = Some(match execute_cleanup_batch(store, &hc, &user, &sel, now) {
                    Ok(rep) => {
                        cleanup_list = None;
                        let kind = if !rep.errors.is_empty() {
                            NoteKind::Err
                        } else if !rep.skipped.is_empty() {
                            NoteKind::Warn
                        } else {
                            NoteKind::Ok
                        };
                        (
                            kind,
                            format!("cleaned {}, skipped {}, errors {}", rep.done, rep.skipped.len(), rep.errors.len()),
                        )
                    }
                    Err(e) => (NoteKind::Err, format!("cleanup error: {e:#}")),
                });
            } else {
                cleanup_list = None;
                note = Some((NoteKind::Warn, "cleanup cancelled".into()));
            }
            continue;
        }

        // Kill/interrupt require confirmation: the prior command set `pending`, this line must be "y".
        if let Some(cmd) = pending.take() {
            if line.trim() == "y" {
                let user = match &who {
                    Caller::User { .. } => Caller::User { confirmed: true },
                    agent => agent.clone(),
                };
                let res = match &cmd {
                    TuiCmd::Kill(t) => actions::kill(store, &hc, &user, t, now).map(|()| format!("killed {t}")),
                    TuiCmd::Interrupt(t) => actions::interrupt(store, &hc, &user, t, now).map(|()| format!("interrupted {t}")),
                    _ => Ok(String::new()),
                };
                note = Some(match res {
                    Ok(msg) => (NoteKind::Ok, msg),
                    Err(e) => (NoteKind::Err, format!("error: {e:#}")),
                });
            } else {
                note = Some((NoteKind::Warn, "cancelled".into()));
            }
            continue;
        }

        match parse_command(&line) {
            Ok(TuiCmd::Quit) => return Ok(()),
            Ok(TuiCmd::Refresh) => {}
            Ok(TuiCmd::Cleanup(arg)) => {
                if let Some(sel_expr) = arg {
                    let list = match cleanup_list.take() {
                        Some(l) => l,
                        None => eligible_cleanups(store).unwrap_or_default(),
                    };
                    if list.is_empty() {
                        note = Some((NoteKind::Warn, "no handed-off sessions ready for cleanup".into()));
                    } else {
                        match crate::ops::parse_selection(&sel_expr, list.len()) {
                            Ok(indices) if !indices.is_empty() => {
                                let chosen: Vec<CleanupCandidate> = indices.into_iter().map(|i| list[i].clone()).collect();
                                let items: Vec<String> = chosen
                                    .iter()
                                    .map(|c| format!("{}→{}", c.old_terminal, c.new_terminal.as_deref().unwrap_or("-")))
                                    .collect();
                                note = Some((
                                    NoteKind::Warn,
                                    format!(
                                        "confirm cleanup of {} session(s) ({})? type y + Enter (anything else cancels)",
                                        chosen.len(),
                                        items.join(", ")
                                    ),
                                ));
                                cleanup_selected = Some(chosen);
                            }
                            Ok(_) => note = Some((NoteKind::Warn, "no items selected".into())),
                            Err(e) => {
                                cleanup_list = Some(list);
                                note = Some((NoteKind::Err, format!("selection error: {e:#}")));
                            }
                        }
                    }
                } else {
                    match eligible_cleanups(store) {
                        Ok(list) if list.is_empty() => {
                            cleanup_list = None;
                            note = Some((NoteKind::Warn, "no handed-off sessions ready for cleanup".into()));
                        }
                        Ok(list) => cleanup_list = Some(list),
                        Err(e) => note = Some((NoteKind::Err, format!("error: {e:#}"))),
                    }
                }
            }
            Ok(cmd @ (TuiCmd::Kill(_) | TuiCmd::Interrupt(_))) => {
                let prompt_desc = match &cmd {
                    TuiCmd::Kill(t) => format!("kill {t}"),
                    TuiCmd::Interrupt(t) => format!("interrupt {t}"),
                    _ => unreachable!(),
                };
                note = Some((NoteKind::Warn, format!("confirm {prompt_desc}? type y + Enter (anything else cancels)")));
                pending = Some(cmd);
            }
            Ok(TuiCmd::Reply(id, text)) => {
                note = Some(match actions::reply(store, &hc, id, &text, now) {
                    Ok(()) => (NoteKind::Ok, format!("replied to #{id}")),
                    Err(e) => (NoteKind::Err, format!("error: {e:#}")),
                });
            }
            Ok(TuiCmd::Send(to, kind, text)) => {
                note = Some(match actions::send(store, &hc, "user", &to, &kind, None, &text, now) {
                    Ok(m) => (NoteKind::Ok, format!("msg #{} → {}: {}", m.id, m.to, m.status)),
                    Err(e) => (NoteKind::Err, format!("error: {e:#}")),
                });
            }
            Ok(TuiCmd::Done(id)) => {
                note = Some(match actions::dismiss(store, id, now) {
                    Ok(()) => (NoteKind::Ok, format!("dismissed #{id}")),
                    Err(e) => (NoteKind::Err, format!("error: {e:#}")),
                });
            }
            Ok(TuiCmd::Policy(on)) => {
                // Never swallow errors: the user must know whether policy actually toggled.
                note = Some(match store.set_setting("policy", if on { "on" } else { "off" }) {
                    Ok(()) => (if on { NoteKind::Ok } else { NoteKind::Warn }, format!("policy {}", if on { "ON" } else { "OFF" })),
                    Err(e) => (NoteKind::Err, format!("ERROR — policy NOT changed: {e:#}")),
                });
            }
            Err(e) => note = Some((NoteKind::Err, e)),
        }
    }
}
