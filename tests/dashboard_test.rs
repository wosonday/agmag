use agmag::config::Config;
use agmag::dashboard::{parse_command, render, snapshot, TuiCmd};
use agmag::model::{ContextSnapshot, QuotaSnapshot, QuotaWindow, Source, Usage, UsageRecord};
use agmag::report::QuotaRow;
use agmag::store::{BudgetRow, Store, TerminalRow};

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn seeded() -> anyhow::Result<Store> {
    let store = Store::open_in_memory()?;
    store.insert_quota(&QuotaSnapshot {
        provider: "codex".into(),
        ts_ms: 1,
        windows: vec![
            QuotaWindow { window_minutes: 300, used_percent: 22.0, resets_at_s: 2_000 },
            QuotaWindow { window_minutes: 10080, used_percent: 86.0, resets_at_s: 900_000 },
        ],
    })?;
    store.upsert_terminal(&TerminalRow {
        address: "t5".into(),
        name: Some("w-login-7-1".into()),
        app: Some("opencode".into()),
        task: Some("login-7".into()),
        role: Some("worker".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_status("t5", Some("blocked"), Some("question"), Some("Chạy test?"), 1, 1)?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://a")?;
    store.upsert_usage(
        sid,
        &UsageRecord {
            dedupe_key: "k".into(),
            ts_ms: 1,
            model: Some("gpt-6.1-sol".into()),
            usage: Usage { output: 340_000, ..Default::default() },
        },
    )?;
    store.set_context(sid, &ContextSnapshot { ts_ms: 1, used_tokens: 237_500, window_tokens: Some(475_000) })?;
    store.link_terminal("t5", sid, "auto")?;
    store.set_budget(&BudgetRow {
        scope: "task:login-7".into(),
        unit: "tokens".into(),
        soft: 300_000.0,
        hard: 3_000_000.0,
        action: "notify".into(),
    })?;
    store.open_inbox(Some("t5"), "blocked", Some("question"), "Chạy test?", 2)?;
    store.add_message(&agmag::store::MessageRow {
        id: 0,
        ts_ms: 3,
        from: "codex-mgr".into(),
        to: "t5".into(),
        kind: "GIAO".into(),
        task: Some("login-7".into()),
        text: "run unit tests".into(),
        status: "delivered".into(),
        detail: None,
    })?;
    Ok(store)
}

#[test]
fn snapshot_sections() -> anyhow::Result<()> {
    let store = seeded()?;
    let s = snapshot(&store, &Config::builtin(), 1_000_000)?;
    assert_eq!(s.quota.len(), 2);
    let t = &s.terminals[0];
    assert_eq!((t.address.as_str(), t.tokens, t.context_pct), ("t5", Some(340_000), Some(50.0)));
    assert_eq!(s.budgets[0].value, Some(340_000.0));
    assert_eq!(s.inbox.len(), 1);
    assert!(s.policy_on);
    Ok(())
}

#[test]
fn render_contains_all_english_sections_and_colors() -> anyhow::Result<()> {
    let store = seeded()?;
    let raw = render(&snapshot(&store, &Config::builtin(), 1_000_000)?, 110);
    // Must contain ANSI SGR colors for lively terminal display.
    assert!(raw.contains("\x1b["), "expected ANSI colors in render output");
    let plain = strip_ansi(&raw);
    for needle in [
        "agmag",
        "POLICY ON",
        "1 terminal (1 blocked)",
        "inbox 1",
        "PLAN QUOTA",
        "codex",
        "86%",
        "USAGE (",
        "24h",
        "Today",
        "This month",
        "TERMINALS",
        "w-login-7-1",
        "BLOCKED",
        "50%",
        "BUDGETS",
        "task:login-7",
        "soft",
        "INBOX",
        "#1",
        "Chạy test?",
        "RECENT MESSAGES",
        "codex-mgr → t5",
        "[GIAO]",
        "(delivered)",
        "cmd:",
        "cleanup",
    ] {
        assert!(plain.contains(needle), "missing '{needle}' in:\n{plain}");
    }
    // Verify no Vietnamese TUI chrome remains.
    for old_vi in ["HẠN MỨC GÓI", "SỬ DỤNG", "Hôm nay", "Tháng này", "NGÂN SÁCH", "HỘP THƯ", "TIN GẦN ĐÂY", "lệnh:"] {
        assert!(!plain.contains(old_vi), "unexpected Vietnamese label '{old_vi}' in:\n{plain}");
    }
    Ok(())
}

#[test]
fn render_empty_state_and_narrow_width() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    store.set_setting("policy", "off")?;
    let raw = render(&snapshot(&store, &Config::builtin(), 1_000_000)?, 80);
    let plain = strip_ansi(&raw);
    for needle in [
        "POLICY OFF",
        "0 terminals",
        "inbox 0",
        "(no quota data",
        "(no active terminals)",
        "(none set",
        "(empty)",
        "(no messages)",
    ] {
        assert!(plain.contains(needle), "missing '{needle}' in:\n{plain}");
    }
    Ok(())
}

#[test]
fn render_shows_red_warning_for_live_rotation_pair() -> anyhow::Result<()> {
    let store = seeded()?;
    store.upsert_terminal(&TerminalRow {
        address: "t6".into(),
        app: Some("claude-code".into()),
        cwd: Some(r"D:\repo".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.upsert_terminal(&TerminalRow {
        address: "t7".into(),
        app: Some("claude-code".into()),
        cwd: Some(r"D:\repo".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    store.set_terminal_handoff_state("t6", "handed_off")?;
    store.conn().execute(
        "INSERT INTO rotations (old_terminal, new_terminal, app, cwd, state, updated_ms) VALUES ('t6', 't7', 'claude-code', 'D:\\repo', 'done', 10)",
        [],
    )?;
    let raw = render(&snapshot(&store, &Config::builtin(), 1_000_000)?, 120);
    assert!(raw.contains("\x1b[1;91m  ! WARNING:"), "expected red warning banner in:\n{raw}");
    let plain = strip_ansi(&raw);
    assert!(plain.contains("WARNING: t6 (handed off) and t7 are both running in D:\\repo"), "{plain}");
    assert!(plain.contains("[HANDED OFF]"), "{plain}");
    Ok(())
}

#[test]
fn parse_commands() {
    assert_eq!(parse_command("r 3 có, chạy đi"), Ok(TuiCmd::Reply(3, "có, chạy đi".into())));
    assert_eq!(parse_command("s t5 GIAO làm tiếp"), Ok(TuiCmd::Send("t5".into(), "GIAO".into(), "làm tiếp".into())));
    assert_eq!(parse_command("s t5 BÁO CÁO xong"), Ok(TuiCmd::Send("t5".into(), "BÁO CÁO".into(), "xong".into())));
    assert_eq!(parse_command("k t5"), Ok(TuiCmd::Kill("t5".into())));
    assert_eq!(parse_command("i t5"), Ok(TuiCmd::Interrupt("t5".into())));
    assert_eq!(parse_command("p off"), Ok(TuiCmd::Policy(false)));
    assert_eq!(parse_command("q"), Ok(TuiCmd::Quit));
    assert_eq!(parse_command(""), Ok(TuiCmd::Refresh));
    assert_eq!(parse_command("r x có").unwrap_err(), "usage: r <id> <reply>");
    assert_eq!(parse_command("zzz").unwrap_err(), "unknown command 'zzz'");
    assert_eq!(parse_command("s t5  GIAO  làm"), Ok(TuiCmd::Send("t5".into(), "GIAO".into(), "làm".into())));
    assert!(parse_command("s t5 GIAOX làm").is_err());
    assert_eq!(parse_command("d 4"), Ok(TuiCmd::Done(4)));
    assert_eq!(parse_command("c"), Ok(TuiCmd::Cleanup(None)));
    assert_eq!(parse_command("c 1,3"), Ok(TuiCmd::Cleanup(Some("1,3".into()))));
    assert_eq!(parse_command("c all"), Ok(TuiCmd::Cleanup(Some("all".into()))));
}

#[test]
fn render_edge_widths_cross_year_and_extreme_values() -> anyhow::Result<()> {
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.usage_summary.recent_months = ["2025-11", "2025-12", "2026-01", "2026-02", "2026-03"]
        .into_iter()
        .map(|k| agmag::report::CostRow {
            key: k.into(),
            usage: Usage { output: 50_000, ..Default::default() },
            billing: None,
            cost_usd: None,
        })
        .collect();
    snap.quota.push(QuotaRow {
        provider: "claude".into(),
        window_minutes: 300,
        used_percent: f64::NAN,
        resets_at_s: 2_000,
        observed_ms: 1,
        reset_passed: true,
    });
    snap.budgets.push(agmag::dashboard::BudgetView {
        scope: "task:over".into(),
        unit: "tokens".into(),
        value: Some(9_000_000.0),
        soft: 100_000.0,
        hard: 200_000.0,
    });

    for w in [60usize, 200usize] {
        let plain = strip_ansi(&render(&snap, w));
        assert!(plain.contains("Nov'25"), "missing cross-year label at width {w}:\n{plain}");
        assert!(plain.contains("Mar'26"), "missing 5th month label at width {w}:\n{plain}");
        assert!(plain.contains("reset ready"), "missing reset ready at width {w}:\n{plain}");
        assert!(plain.contains("HARD"), "missing HARD badge at width {w}:\n{plain}");
        assert!(plain.contains(">999%"), "missing >999% cap for extreme budget at width {w}:\n{plain}");
        assert!(!plain.contains("NaN%"), "NaN leaked into output at width {w}:\n{plain}");
        for line in plain.lines() {
            assert!(
                line.chars().count() < w,
                "line exceeded width-1 ({w}): {} chars: {line}",
                line.chars().count()
            );
        }
    }
    assert_eq!(agmag::dashboard::one_line("\x1b[2~LEAK \x1b]52;c;YQ==\x07OK \x1b(BHI", 50), "LEAK OK HI");
    Ok(())
}

#[test]
fn one_line_bounds_wide_unicode_by_terminal_cells() {
    let text = agmag::dashboard::one_line(&"界".repeat(40), 12);
    let cells: usize = text.chars().map(|c| if c == '界' { 2 } else { 1 }).sum();
    assert!(cells <= 12, "{cells} cells: {text}");
}

#[test]
fn render_wraps_unbroken_warning_paths_within_width() -> anyhow::Result<()> {
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.live_rotation_pairs.push(("t1".into(), "t2".into(), format!("D:/{}", "long-path".repeat(30))));
    for width in [60, 80, 110] {
        let plain = strip_ansi(&render(&snap, width));
        for line in plain.lines() {
            assert!(line.chars().count() < width, "{width}: {}: {line}", line.chars().count());
        }
    }
    Ok(())
}

#[test]
fn render_bounds_untrusted_message_and_inbox_prefixes() -> anyhow::Result<()> {
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.inbox[0].source = "long-source".repeat(30);
    snap.inbox[0].terminal = Some("long-terminal".repeat(30));
    snap.messages[0].from = "long-from".repeat(30);
    snap.messages[0].to = "long-to".repeat(30);
    snap.messages[0].kind = "long-kind".repeat(30);
    snap.messages[0].status = "long-status".repeat(30);
    for width in [60, 80, 110] {
        let plain = strip_ansi(&render(&snap, width));
        for line in plain.lines() {
            assert!(line.chars().count() < width, "{width}: {}: {line}", line.chars().count());
        }
    }
    Ok(())
}

#[test]
fn styled_clipping_preserves_graphemes_and_resets_color() {
    use unicode_width::UnicodeWidthStr;
    use agmag::dashboard::{bounded_styled_line, one_line};
    assert_eq!(one_line("👩‍💻abc", 3), "👩‍💻…");
    assert_eq!(one_line("e\u{301}e\u{301}x", 2), "e\u{301}…");
    let clipped = bounded_styled_line("\x1b[31m👩‍💻界X\x1b[0m", 4);
    assert_eq!(strip_ansi(&clipped), "👩‍💻界");
    assert_eq!(strip_ansi(&clipped).width(), 4);
    assert!(clipped.ends_with("\x1b[0m"));
    let escaped = bounded_styled_line("\x1b]52;c;c2VjcmV0\x07safe", 4);
    assert_eq!(strip_ansi(&escaped), "safe");
}

#[test]
fn render_wide_untrusted_content_stays_inside_terminal_cells() -> anyhow::Result<()> {
    use unicode_width::UnicodeWidthStr;
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    for t in &mut snap.terminals {
        t.name = "👩‍💻界".repeat(20);
        t.task = "界".repeat(30);
        t.context_pct = Some(f64::NAN);
    }
    snap.inbox[0].text = "界👩‍💻e\u{301}".repeat(40);
    snap.messages[0].text = "界".repeat(100);
    snap.budgets[0].scope = "界".repeat(40);
    snap.usage_summary.recent_months[0].key = "年20-01".into();
    for width in [60, 80, 110, 200] {
        let plain = strip_ansi(&render(&snap, width));
        for line in plain.lines() {
            assert!(line.width() < width, "{width}: {} cells: {line}", line.width());
        }
    }
    Ok(())
}
