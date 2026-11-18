//! `agm tui`: bảng điều khiển vẽ lại theo chu kỳ (mã ANSI) + dòng lệnh ngắn.
//! Không dùng thư viện TUI (máy đang offline); phím được thay bằng lệnh một chữ: r s i k p q.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;

use agmag::actions::{self, Caller};
use agmag::config::Config;
use agmag::dashboard::{parse_command, render, snapshot, TuiCmd};
use agmag::hc::HcCli;
use agmag::store::Store;

fn width() -> usize {
    std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(110).clamp(60, 200)
}

pub fn run(store: &Store, cfg: &Config, refresh: u64, mut scan: impl FnMut(&Store) -> Result<()>) -> Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let hc = HcCli::default();
    // Ai đang dùng TUI: xác định một lần, fail-closed. Agent (hoặc stdin bị pipe) không được kill/ngắt qua TUI.
    let who = actions::detect_caller(
        store,
        &hc,
        crate::ops::hc_terminal().as_deref(),
        std::io::stdin().is_terminal(),
        false,
    );
    if !matches!(who, Caller::User { .. }) {
        anyhow::bail!("agm tui chỉ dành cho người dùng ở terminal (TTY, không phải agent); agent hãy dùng các lệnh agm thường");
    }
    let mut note = String::new();
    let mut pending: Option<TuiCmd> = None;
    loop {
        let _ = scan(store);
        let now = crate::now_ms();
        let text = match snapshot(store, cfg, now) {
            Ok(s) => render(&s, width()),
            Err(e) => format!("không đọc được CSDL: {e:#}\n"),
        };
        // Xóa màn hình + về góc trên; vẽ; in ghi chú lần trước và dấu nhắc.
        print!("\x1b[2J\x1b[H{text}");
        if !note.is_empty() {
            println!("» {note}");
        }
        print!("> ");
        std::io::stdout().flush()?;
        let line = match rx.recv_timeout(Duration::from_secs(refresh.max(1))) {
            Ok(l) => l,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        note.clear();
        // Kill/ngắt cần xác nhận: lệnh trước đặt `pending`, dòng này phải là "y".
        if let Some(cmd) = pending.take() {
            if line.trim() == "y" {
                let user = match &who {
                    Caller::User { .. } => Caller::User { confirmed: true },
                    agent => agent.clone(),
                };
                let res = match &cmd {
                    TuiCmd::Kill(t) => actions::kill(store, &hc, &user, t, now),
                    TuiCmd::Interrupt(t) => actions::interrupt(store, &hc, &user, t, now),
                    _ => Ok(()),
                };
                note = match res {
                    Ok(()) => format!("đã thực hiện {cmd:?}"),
                    Err(e) => format!("lỗi: {e:#}"),
                };
            } else {
                note = "đã hủy".into();
            }
            continue;
        }
        match parse_command(&line) {
            Ok(TuiCmd::Quit) => return Ok(()),
            Ok(TuiCmd::Refresh) => {}
            Ok(cmd @ (TuiCmd::Kill(_) | TuiCmd::Interrupt(_))) => {
                note = format!("xác nhận {cmd:?}? gõ y rồi Enter (khác: hủy)");
                pending = Some(cmd);
            }
            Ok(TuiCmd::Reply(id, text)) => {
                note = match actions::reply(store, &hc, id, &text, now) {
                    Ok(()) => format!("đã trả lời #{id}"),
                    Err(e) => format!("lỗi: {e:#}"),
                };
            }
            Ok(TuiCmd::Send(to, kind, text)) => {
                note = match actions::send(store, &hc, "user", &to, &kind, None, &text, now) {
                    Ok(m) => format!("tin #{} → {}: {}", m.id, m.to, m.status),
                    Err(e) => format!("lỗi: {e:#}"),
                };
            }
            Ok(TuiCmd::Done(id)) => {
                note = match actions::dismiss(store, id, now) {
                    Ok(()) => format!("đã đóng #{id}"),
                    Err(e) => format!("lỗi: {e:#}"),
                };
            }
            Ok(TuiCmd::Policy(on)) => {
                // Không nuốt lỗi: người dùng phải biết policy có thật sự tắt hay không.
                note = match store.set_setting("policy", if on { "on" } else { "off" }) {
                    Ok(()) => format!("policy {}", if on { "bật" } else { "TẮT" }),
                    Err(e) => format!("LỖI — policy CHƯA đổi: {e:#}"),
                };
            }
            Err(e) => note = e,
        }
    }
}
