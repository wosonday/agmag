use agmag::config::Config;
use agmag::dashboard::{TuiCmd, parse_command, render, snapshot};
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
            QuotaWindow {
                window_minutes: 300,
                used_percent: 22.0,
                resets_at_s: 2_000,
            },
            QuotaWindow {
                window_minutes: 10080,
                used_percent: 86.0,
                resets_at_s: 900_000,
            },
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
    store.set_terminal_status(
        "t5",
        Some("blocked"),
        Some("question"),
        Some("Chạy test?"),
        1,
        1,
    )?;
    let sid = store.session_id_for_path(Source::OpenCode, "opencode://a")?;
    store.upsert_usage(
        sid,
        &UsageRecord {
            dedupe_key: "k".into(),
            ts_ms: 1,
            model: Some("gpt-6.1-sol".into()),
            usage: Usage {
                output: 340_000,
                ..Default::default()
            },
        },
    )?;
    store.set_context(
        sid,
        &ContextSnapshot {
            ts_ms: 1,
            used_tokens: 237_500,
            window_tokens: Some(475_000),
        },
    )?;
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
fn plan_quota_shows_percent_left() -> anyhow::Result<()> {
    let store = seeded()?;
    // codex: 22% used on 5h, 86% used on 7d → 78% / 14% left.
    let plain = strip_ansi(&render(
        &snapshot(&store, &Config::builtin(), 1_000_000)?,
        110,
    ));
    assert!(plain.contains("PLAN QUOTA (left)"), "{plain}");
    assert!(plain.contains("78% left"), "{plain}");
    assert!(plain.contains("14% left"), "{plain}");
    assert!(
        !plain.contains("22%") && !plain.contains("86%"),
        "used % must not be shown as the quota figure: {plain}"
    );
    Ok(())
}

#[test]
fn snapshot_sections() -> anyhow::Result<()> {
    let store = seeded()?;
    let s = snapshot(&store, &Config::builtin(), 1_000_000)?;
    assert_eq!(s.quota.len(), 2);
    let t = &s.terminals[0];
    assert_eq!(
        (t.address.as_str(), t.tokens, t.context_pct),
        ("t5", Some(340_000), Some(50.0))
    );
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
    assert!(
        raw.contains("\x1b["),
        "expected ANSI colors in render output"
    );
    let plain = strip_ansi(&raw);
    for needle in [
        "agmag",
        "POLICY ON",
        "1 terminal (1 blocked)",
        "inbox 1",
        "PLAN QUOTA",
        "codex",
        "14% left",
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
        "[ASSIGN]",
        "(delivered)",
        "cmd:",
        "cleanup",
    ] {
        assert!(plain.contains(needle), "missing '{needle}' in:\n{plain}");
    }
    // Verify no Vietnamese TUI chrome remains.
    for old_vi in [
        "HẠN MỨC GÓI",
        "SỬ DỤNG",
        "Hôm nay",
        "Tháng này",
        "NGÂN SÁCH",
        "HỘP THƯ",
        "TIN GẦN ĐÂY",
        "lệnh:",
    ] {
        assert!(
            !plain.contains(old_vi),
            "unexpected Vietnamese label '{old_vi}' in:\n{plain}"
        );
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
    assert!(
        raw.contains("\x1b[1;91m  ! WARNING:"),
        "expected red warning banner in:\n{raw}"
    );
    let plain = strip_ansi(&raw);
    assert!(
        plain.contains("WARNING: t6 (handed off) and t7 are both running in D:\\repo"),
        "{plain}"
    );
    assert!(plain.contains("[HANDED OFF]"), "{plain}");
    Ok(())
}

#[test]
fn parse_commands() {
    // Full names and single-letter aliases give the same command.
    for (full, alias) in [
        ("reply 3 có, chạy đi", "r 3 có, chạy đi"),
        ("done 4", "d 4"),
        ("kill t5", "k t5"),
        ("interrupt t5", "i t5"),
        ("policy off", "p off"),
        ("quit", "q"),
        ("cleanup 1,3", "c 1,3"),
        ("send t5 ASSIGN làm tiếp", "s t5 ASSIGN làm tiếp"),
    ] {
        assert_eq!(parse_command(full), parse_command(alias), "{full}");
        assert!(parse_command(full).is_ok(), "{full}");
    }
    assert_eq!(
        parse_command("reply 3 có, chạy đi"),
        Ok(TuiCmd::Reply(3, "có, chạy đi".into()))
    );
    assert_eq!(parse_command("kill t5"), Ok(TuiCmd::Kill("t5".into())));
    assert_eq!(
        parse_command("interrupt t5"),
        Ok(TuiCmd::Interrupt("t5".into()))
    );
    assert_eq!(parse_command("policy off"), Ok(TuiCmd::Policy(false)));
    assert_eq!(parse_command("quit"), Ok(TuiCmd::Quit));
    assert_eq!(parse_command(""), Ok(TuiCmd::Refresh));
    assert_eq!(
        parse_command("done 4"),
        Ok(TuiCmd::Done {
            ids: vec![4],
            filters: agmag::actions::DoneFilters::default(),
            explicit_source_blocked: false,
            legacy_single: true,
        })
    );
    assert_eq!(
        parse_command("done 150-179"),
        Ok(TuiCmd::Done {
            ids: (150..=179).collect(),
            filters: agmag::actions::DoneFilters::default(),
            explicit_source_blocked: false,
            legacy_single: false,
        })
    );
    assert_eq!(
        parse_command("done 150,151 160-162"),
        Ok(TuiCmd::Done {
            ids: vec![150, 151, 160, 161, 162],
            filters: agmag::actions::DoneFilters::default(),
            explicit_source_blocked: false,
            legacy_single: false,
        })
    );
    assert_eq!(
        parse_command("done kind:REPORT"),
        Ok(TuiCmd::Done {
            ids: vec![],
            filters: agmag::actions::DoneFilters {
                kind: agmag::actions::normalize_done_kind("REPORT"),
                terminal: None,
                source: None,
            },
            explicit_source_blocked: false,
            legacy_single: false,
        })
    );
    assert_eq!(
        parse_command("done kind:\"BÁO CÁO\""),
        Ok(TuiCmd::Done {
            ids: vec![],
            filters: agmag::actions::DoneFilters {
                kind: agmag::actions::normalize_done_kind("BÁO CÁO"),
                terminal: None,
                source: None,
            },
            explicit_source_blocked: false,
            legacy_single: false,
        })
    );
    assert_eq!(
        parse_command("done term:t4w"),
        Ok(TuiCmd::Done {
            ids: vec![],
            filters: agmag::actions::DoneFilters {
                kind: None,
                terminal: Some("t4w".into()),
                source: None,
            },
            explicit_source_blocked: false,
            legacy_single: false,
        })
    );
    assert_eq!(
        parse_command("done 5 term:t4w kind:REPORT"),
        Ok(TuiCmd::Done {
            ids: vec![5],
            filters: agmag::actions::DoneFilters {
                kind: agmag::actions::normalize_done_kind("REPORT"),
                terminal: Some("t4w".into()),
                source: None,
            },
            explicit_source_blocked: false,
            legacy_single: false,
        })
    );
    // Only an explicit `source:blocked` (or alias `src:`) unlocks blocked items; ranges never do.
    for cmd in ["done source:blocked", "done src:blocked 1-3"] {
        let Ok(TuiCmd::Done {
            filters,
            explicit_source_blocked,
            ..
        }) = parse_command(cmd)
        else {
            panic!("{cmd}: not parsed as Done");
        };
        assert!(explicit_source_blocked, "{cmd}");
        assert_eq!(filters.source.as_deref(), Some("blocked"), "{cmd}");
    }
    let Ok(TuiCmd::Done {
        explicit_source_blocked,
        ..
    }) = parse_command("done 1-3 source:message")
    else {
        panic!("done 1-3 source:message: not parsed as Done");
    };
    assert!(!explicit_source_blocked);
    assert_eq!(parse_command("cleanup"), Ok(TuiCmd::Cleanup(None)));
    assert_eq!(
        parse_command("cleanup all"),
        Ok(TuiCmd::Cleanup(Some("all".into())))
    );
    assert_eq!(
        parse_command("r x có").unwrap_err(),
        "usage: reply <#> <text>"
    );
    assert!(parse_command("done x").is_err());
    assert!(parse_command("done 9-3").is_err());
    assert!(parse_command("done 1-").is_err());
    assert_eq!(
        parse_command("done").unwrap_err(),
        "usage: done <#|a-b|kind:K|term:T>"
    );
    let unknown = parse_command("zzz").unwrap_err();
    assert!(
        unknown.starts_with("unknown command 'zzz'") && unknown.contains("reply, done, send"),
        "{unknown}"
    );
}
#[test]
fn send_accepts_english_and_legacy_vietnamese_types() {
    let send = |t: &str, k: &str, m: &str| -> Result<TuiCmd, String> {
        Ok(TuiCmd::Send(t.into(), k.into(), m.into()))
    };
    for (en, vi) in [
        ("ASSIGN", "GIAO"),
        ("REPORT", "BÁO CÁO"),
        ("ERROR", "LỖI"),
        ("QUESTION", "HỎI"),
        ("BLOCKED", "CHẶN"),
        ("PASS", "ĐẠT"),
        ("FAIL", "CHƯA ĐẠT"),
    ] {
        let expected = send("t5", vi, "Nội dung");
        assert_eq!(
            parse_command(&format!("send t5 {en} Nội dung")),
            expected,
            "{en}"
        );
        assert_eq!(
            parse_command(&format!("send t5 {vi} Nội dung")),
            expected,
            "{vi}"
        );
    }
    assert_eq!(
        parse_command("s user QUESTION Cần xác nhận?"),
        parse_command("s user HỎI Cần xác nhận?")
    );
    assert_eq!(
        parse_command("s t5 report Đã chạy test"),
        send("t5", "BÁO CÁO", "Đã chạy test")
    );
    assert_eq!(parse_command("send t5 Pass OK"), send("t5", "ĐẠT", "OK"));
    // Message text keeps its case and Unicode; extra spaces between fields are ignored.
    assert_eq!(
        parse_command("send t5  REPORT  Đã XONG 界 👩‍💻"),
        send("t5", "BÁO CÁO", "Đã XONG 界 👩‍💻")
    );
}

#[test]
fn send_rejects_bad_type_target_or_message() {
    for (line, needle) in [
        ("send t5 REPORTING done", "unknown message type"),
        ("send t5 GIAOX làm", "unknown message type"),
        ("send t5 NOPE x", "unknown message type"),
        ("send", "missing target terminal"),
        ("send t5 REPORT", "missing message text"),
        ("send t5 BÁO CÁO", "missing message text"),
        ("send t5 REPORT    ", "missing message text"),
    ] {
        let err = parse_command(line).unwrap_err();
        assert!(err.contains(needle), "{line}: {err}");
        assert!(
            err.contains("ASSIGN|REPORT|ERROR|QUESTION|BLOCKED|PASS|FAIL"),
            "{line}: {err}"
        );
    }
}

#[test]
fn command_bar_lists_full_names_and_all_english_types() -> anyhow::Result<()> {
    let store = seeded()?;
    let snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    for width in [60, 110] {
        let plain = strip_ansi(&render(&snap, width));
        let bar = &plain[plain.rfind("cmd:").expect("cmd bar")..];
        for name in [
            "reply",
            "done",
            "send",
            "cleanup",
            "interrupt",
            "kill",
            "policy",
            "view",
            "quit",
            "ASSIGN",
            "REPORT",
            "ERROR",
            "QUESTION",
            "BLOCKED",
            "PASS",
            "FAIL",
        ] {
            assert!(
                bar.contains(name),
                "width {width}: missing '{name}' in:
{bar}"
            );
        }
        assert!(
            bar.lines().all(|l| l.chars().count() < width),
            "width {width} overflow:
{bar}"
        );
    }
    Ok(())
}

/// Dòng quota stale hiện `stale · seen … ago` màu xám, không tràn khung ở cả 60 lẫn 110 cột.
#[test]
fn render_stale_quota_line_fits_60_and_110() -> anyhow::Result<()> {
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.quota.push(QuotaRow {
        provider: "codex".into(),
        window_minutes: 300,
        used_percent: 22.0,
        left_percent: 78.0,
        resets_at_s: 900_000,
        observed_ms: 1,
        reset_passed: false,
        stale: true,
        observed_age_s: 90_000, // 1d01h
        limited: false,
    });
    for w in [60usize, 110usize] {
        let plain = strip_ansi(&render(&snap, w));
        let line = plain
            .lines()
            .find(|l| l.contains("stale"))
            .unwrap_or_else(|| panic!("width {w}: missing stale line:\n{plain}"));
        assert!(
            line.contains("stale · seen 1d01h ago"),
            "width {w}: wrong stale text: {line}"
        );
        assert!(
            !line.contains("% left"),
            "stale row must not show a fresh percent: {line}"
        );
        // Màu xám, không phải màu xanh của số tươi.
        let raw = render(&snap, w);
        let raw_line = raw.lines().find(|l| l.contains("stale · seen")).unwrap();
        assert!(
            raw_line.contains("\x1b[90m"),
            "width {w}: stale text must be gray: {raw_line:?}"
        );
        for l in plain.lines() {
            assert!(
                l.chars().count() < w,
                "width {w}: line overflow ({} chars): {l}",
                l.chars().count()
            );
        }
    }
    Ok(())
}

#[test]
fn render_edge_widths_cross_year_and_extreme_values() -> anyhow::Result<()> {
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.usage_summary.recent_months = ["2025-11", "2025-12", "2026-01", "2026-02", "2026-03"]
        .into_iter()
        .map(|k| agmag::report::CostRow {
            key: k.into(),
            usage: Usage {
                output: 50_000,
                ..Default::default()
            },
            billing: None,
            cost_usd: None,
            value_usd: None,
            unpriced_tokens: 0,
        })
        .collect();
    snap.quota.push(QuotaRow {
        provider: "claude".into(),
        window_minutes: 300,
        used_percent: f64::NAN,
        left_percent: f64::NAN,
        resets_at_s: 2_000,
        observed_ms: 1,
        reset_passed: true,
        stale: false,
        observed_age_s: 0,
        limited: false,
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
        assert!(
            plain.contains("Nov'25"),
            "missing cross-year label at width {w}:\n{plain}"
        );
        assert!(
            plain.contains("Mar'26"),
            "missing 5th month label at width {w}:\n{plain}"
        );
        assert!(
            plain.contains("reset ready"),
            "missing reset ready at width {w}:\n{plain}"
        );
        assert!(
            plain.contains("HARD"),
            "missing HARD badge at width {w}:\n{plain}"
        );
        assert!(
            plain.contains(">999%"),
            "missing >999% cap for extreme budget at width {w}:\n{plain}"
        );
        assert!(
            !plain.contains("NaN%"),
            "NaN leaked into output at width {w}:\n{plain}"
        );
        for line in plain.lines() {
            assert!(
                line.chars().count() < w,
                "line exceeded width-1 ({w}): {} chars: {line}",
                line.chars().count()
            );
        }
    }
    assert_eq!(
        agmag::dashboard::one_line("\x1b[2~LEAK \x1b]52;c;YQ==\x07OK \x1b(BHI", 50),
        "LEAK OK HI"
    );
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
    snap.live_rotation_pairs.push((
        "t1".into(),
        "t2".into(),
        format!("D:/{}", "long-path".repeat(30)),
    ));
    for width in [60, 80, 110] {
        let plain = strip_ansi(&render(&snap, width));
        for line in plain.lines() {
            assert!(
                line.chars().count() < width,
                "{width}: {}: {line}",
                line.chars().count()
            );
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
            assert!(
                line.chars().count() < width,
                "{width}: {}: {line}",
                line.chars().count()
            );
        }
    }
    Ok(())
}

#[test]
fn styled_clipping_preserves_graphemes_and_resets_color() {
    use agmag::dashboard::{bounded_styled_line, one_line};
    use unicode_width::UnicodeWidthStr;
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
            assert!(
                line.width() < width,
                "{width}: {} cells: {line}",
                line.width()
            );
        }
    }
    Ok(())
}

#[test]
fn wide_budget_scope_preserves_soft_and_hard_badges() -> anyhow::Result<()> {
    use unicode_width::UnicodeWidthStr;
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.budgets[0].scope = "界".repeat(40);
    for (value, badge) in [(340_000.0, "soft"), (9_000_000.0, "HARD")] {
        snap.budgets[0].value = Some(value);
        let plain = strip_ansi(&render(&snap, 60));
        assert!(
            plain.contains(&format!(" {badge}")),
            "{badge} missing:\n{plain}"
        );
        assert!(plain.lines().all(|line| line.width() < 60));
    }
    Ok(())
}

#[test]
fn terminals_show_name_dot_agent_model_effort_and_where() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&TerminalRow {
        address: "t11".into(),
        name: Some("w-login-1".into()),
        app: Some("codex".into()),
        cwd: Some(r"D:\Project\agmag\.wt\login-1".into()),
        state: "live".into(),
        status: Some("working".into()),
        ..Default::default()
    })?;
    store.upsert_terminal(&TerminalRow {
        address: "t12".into(),
        app: Some("antigravity".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    let codex = store.session_id_for_path(Source::Codex, "codex://a")?;
    store.set_model(codex, "gpt-6.1-sol")?;
    store.set_effort(codex, "high")?;
    store.link_terminal("t11", codex, "manual")?;
    let agy = store.session_id_for_path(Source::Agy, "agy://b")?;
    store.set_model(agy, "gemini-4-argon-medium")?;
    store.link_terminal("t12", agy, "manual")?;
    let plain = strip_ansi(&render(
        &snapshot(&store, &Config::builtin(), 1_000_000)?,
        160,
    ));
    for needle in [
        "name",
        "agent",
        "model",
        "effort",
        "where",
        "● w-login-1",
        "● t12",
        "gpt-6.1-sol",
        "high",
        "gemini-4-argon",
        "medium",
        "login-1",
    ] {
        assert!(plain.contains(needle), "missing '{needle}' in:\n{plain}");
    }
    assert!(
        !plain.contains("gemini-4-argon-medium"),
        "agy effort suffix should move to the effort column:\n{plain}"
    );
    Ok(())
}

#[test]
fn terminals_columns_order_short_name_and_folder_only() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    store.upsert_terminal(&TerminalRow {
        address: "t20".into(),
        name: Some("worker-with-a-really-long-name-1".into()),
        app: Some("opencode".into()),
        cwd: Some(r"D:\Project\agmag\.wt\login-1\".into()),
        state: "live".into(),
        ..Default::default()
    })?;
    let plain = strip_ansi(&render(
        &snapshot(&store, &Config::builtin(), 1_000_000)?,
        160,
    ));
    let header = plain
        .lines()
        .find(|l| l.contains("name") && l.contains("agent"))
        .expect("terminals header");
    let order = [
        "name", "agent", "model", "effort", "where", "task", "tokens", "ctx", "last msg", "status",
    ];
    let pos: Vec<usize> = order
        .iter()
        .map(|h| header.find(h).unwrap_or_else(|| panic!("{h} in {header}")))
        .collect();
    assert!(
        pos.windows(2).all(|w| w[0] < w[1]),
        "column order wrong: {header}"
    );
    let row = plain
        .lines()
        .find(|l| l.contains("● worker"))
        .expect("terminal row");
    assert!(
        row.contains("● worker-with…") && !row.contains("long-name-1"),
        "name not shortened: {row}"
    );
    assert!(
        row.contains(" login-1 ") && !row.contains("Project") && !row.contains(".wt"),
        "where should be the folder only: {row}"
    );
    Ok(())
}

#[test]
fn split_effort_and_folder_name() {
    use agmag::dashboard::{folder_name, split_effort};
    assert_eq!(
        split_effort("gemini-4-argon-high", None),
        ("gemini-4-argon".into(), "high".into())
    );
    assert_eq!(
        split_effort("claude-opus-5-5", Some("max")),
        ("claude-opus-5-5".into(), "max".into())
    );
    assert_eq!(
        split_effort("claude-opus-5-5", None),
        ("claude-opus-5-5".into(), String::new())
    );
    assert_eq!(
        split_effort("opencode-go/deepseek-v4-pro", Some("high")),
        ("opencode-go/deepseek-v4-pro".into(), "high".into())
    );
    assert_eq!(folder_name(r"D:\Project\agmag\.wt\login-1"), "login-1");
    assert_eq!(folder_name(r"D:\Project\agmag\"), "agmag");
    assert_eq!(folder_name("/home/u/repo/"), "repo");
    assert_eq!(folder_name("agmag"), "agmag");
    assert_eq!(folder_name(r"D:\"), "D:");
    assert_eq!(folder_name(""), "");
}

#[test]
fn terminals_show_last_message_kind_in_english_with_direction() -> anyhow::Result<()> {
    use agmag::store::MessageRow;
    let store = Store::open_in_memory()?;
    for (address, name) in [("t3p", Some("ceo")), ("t3v", None), ("t9", None)] {
        store.upsert_terminal(&TerminalRow {
            address: address.into(),
            name: name.map(String::from),
            app: Some("codex".into()),
            state: "live".into(),
            ..Default::default()
        })?;
    }
    let msg = |ts: i64, from: &str, to: &str, kind: &str| MessageRow {
        ts_ms: ts,
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        text: "x".into(),
        status: "delivered".into(),
        ..Default::default()
    };
    // Messages may name a terminal by address or by name.
    store.add_message(&msg(1, "ceo", "t3v", "GIAO"))?;
    store.add_message(&msg(2, "t3v", "t3p", "BÁO CÁO"))?;
    let snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    let last = |addr: &str| {
        snap.terminals
            .iter()
            .find(|t| t.address == addr)
            .unwrap()
            .last_msg
            .clone()
    };
    assert_eq!(last("t3v"), Some(("REPORT".to_string(), true)));
    assert_eq!(last("t3p"), Some(("REPORT".to_string(), false)));
    assert_eq!(last("t9"), None);
    let plain = strip_ansi(&render(&snap, 160));
    for needle in ["last msg", "→ REPORT", "← REPORT", "[REPORT]", "[ASSIGN]"] {
        assert!(plain.contains(needle), "missing '{needle}' in:\n{plain}");
    }
    assert!(
        !plain.contains("[BÁO CÁO]") && !plain.contains("[GIAO]"),
        "{plain}"
    );
    Ok(())
}

#[test]
fn view_command_picks_up_to_three_sections() {
    use agmag::dashboard::{Section, parse_view};
    assert_eq!(parse_command("view"), Ok(TuiCmd::View(None)));
    // Any order and case; shown in display order; duplicates collapse.
    assert_eq!(
        parse_command("v inbox QUOTA,terminals"),
        Ok(TuiCmd::View(Some(vec![
            Section::Quota,
            Section::Terminals,
            Section::Inbox
        ])))
    );
    assert_eq!(parse_view("inbox inbox"), Ok(vec![Section::Inbox]));
    // RECENT MESSAGES is pinned, so it is not a choice (and never uses one of the 3 slots).
    assert!(parse_view("messages").unwrap_err().contains("always shown"));
    assert!(!Section::names().contains("messages"));
    assert_eq!(parse_view("default"), Ok(Section::DEFAULT.to_vec()));
    assert!(
        parse_view("quota usage terminals inbox")
            .unwrap_err()
            .starts_with("usage: view <1-3 of")
    );
    assert!(
        parse_view("quota nope")
            .unwrap_err()
            .contains("unknown section 'nope'")
    );
    assert!(parse_command("view").is_ok() && parse_command("view a b c d").is_err());
}

#[test]
fn parse_cd_and_show() {
    // `cd` alone lists projects; arguments enter a project (`..` and `/` leave it).
    assert_eq!(parse_command("cd"), Ok(TuiCmd::Cd(None)));
    assert_eq!(
        parse_command("cd  shop "),
        Ok(TuiCmd::Cd(Some("shop".into())))
    );
    assert_eq!(parse_command("cd .."), Ok(TuiCmd::Cd(Some("..".into()))));
    assert_eq!(parse_command("cd /"), Ok(TuiCmd::Cd(Some("/".into()))));
    assert_eq!(parse_command("show 43"), Ok(TuiCmd::Show(43)));
    assert_eq!(parse_command("show #43"), Ok(TuiCmd::Show(43)));
    for line in ["show", "show x"] {
        let err = parse_command(line).unwrap_err();
        assert!(err.contains("show <#>"), "{line}: {err}");
    }
}

#[test]
fn footer_lists_cd_and_show() -> anyhow::Result<()> {
    use agmag::dashboard::{Section, render_view};
    let store = seeded()?;
    let snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    let plain = strip_ansi(&render_view(&snap, 110, &[Section::Quota], None));
    let bar = &plain[plain.rfind("cmd:").expect("cmd bar")..];
    assert!(bar.contains("cd [project|..]"), "{bar}");
    assert!(bar.contains("show <#>"), "{bar}");
    // cd/show carry no short alias: an empty entry must not show as a stray separator.
    assert!(!bar.contains("·  ·"), "{bar}");
    Ok(())
}

#[test]
fn render_view_shows_only_chosen_sections_and_fits_height() -> anyhow::Result<()> {
    use agmag::dashboard::{Section, render_view};
    let store = seeded()?;
    for i in 0..30 {
        store.upsert_terminal(&TerminalRow {
            address: format!("x{i}"),
            app: Some("opencode".into()),
            state: "live".into(),
            ..Default::default()
        })?;
    }
    let snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    let plain = strip_ansi(&render_view(&snap, 110, &[Section::Terminals], None));
    assert!(
        plain.contains("TERMINALS") && plain.contains("cmd:"),
        "{plain}"
    );
    assert!(
        plain.contains("RECENT MESSAGES") && plain.contains("[ASSIGN]"),
        "messages are always shown:\n{plain}"
    );
    assert!(
        plain.find("RECENT MESSAGES") < plain.find("cmd:"),
        "messages sit just above the command bar:\n{plain}"
    );
    for hidden in ["PLAN QUOTA", "USAGE (", "BUDGETS", "INBOX"] {
        assert!(
            !plain.contains(hidden),
            "'{hidden}' should be hidden:\n{plain}"
        );
    }
    for height in [24, 40] {
        let plain = strip_ansi(&render_view(&snap, 110, &Section::DEFAULT, Some(height)));
        // Two lines stay free for the note and the `agm>` prompt, so the pane never scrolls.
        assert!(
            plain.lines().count() <= height - 2,
            "height {height}: {} lines\n{plain}",
            plain.lines().count()
        );
        for needle in [
            "agmag",
            "PLAN QUOTA",
            "TERMINALS",
            "INBOX",
            "RECENT MESSAGES",
            "more lines",
            "cmd:",
            "TYPE:",
        ] {
            assert!(
                plain.contains(needle),
                "height {height}: missing '{needle}':\n{plain}"
            );
        }
    }
    // Enough room → nothing is cut.
    let full = strip_ansi(&render_view(&snap, 110, &Section::DEFAULT, Some(500)));
    assert!(!full.contains("more lines"), "{full}");
    Ok(())
}

#[test]
fn where_shows_herdr_workspace_and_tab() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("where".to_string(), "MATHY-AI / opus-high".to_string());
    store.upsert_terminal(&TerminalRow {
        address: "t35".into(),
        app: Some("antigravity".into()),
        cwd: Some(r"D:\Project\MATHY-AI".into()),
        labels,
        state: "live".into(),
        ..Default::default()
    })?;
    let plain = strip_ansi(&render(
        &snapshot(&store, &Config::builtin(), 1_000_000)?,
        160,
    ));
    let row = plain.lines().find(|l| l.contains("● t35")).expect("row");
    assert!(row.contains("MATHY-AI / opus-high"), "{row}");
    // No herdr location → working folder name.
    assert_eq!(agmag::dashboard::where_label("", r"D:\x\agmag"), "agmag");
    assert_eq!(agmag::dashboard::where_label("  ", "/r/repo"), "repo");
    Ok(())
}

#[test]
fn render_opencode_go_quota_labels_limited_and_stale_fit_60_110() -> anyhow::Result<()> {
    use agmag::dashboard::render_view;
    let store = seeded()?;
    let mut snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    snap.quota = vec![
        QuotaRow {
            provider: "opencode-go".into(),
            window_minutes: 300,
            used_percent: 100.0,
            left_percent: 0.0,
            resets_at_s: 2_000_000,
            observed_ms: 900_000_000,
            reset_passed: false,
            stale: false,
            observed_age_s: 100,
            limited: true,
        },
        QuotaRow {
            provider: "opencode-go".into(),
            window_minutes: 10080,
            used_percent: 40.0,
            left_percent: 60.0,
            resets_at_s: 2_000_000,
            observed_ms: 900_000_000,
            reset_passed: false,
            stale: false,
            observed_age_s: 100,
            limited: false,
        },
        QuotaRow {
            provider: "opencode-go".into(),
            window_minutes: 43200,
            used_percent: 20.0,
            left_percent: 80.0,
            resets_at_s: 2_000_000,
            observed_ms: 900_000_000,
            reset_passed: false,
            stale: true,
            observed_age_s: 90_000,
            limited: false,
        },
    ];
    for w in [60usize, 110usize] {
        let raw = render_view(&snap, w, &[agmag::dashboard::Section::Quota], None);
        let plain = strip_ansi(&raw);
        for needle in ["opencode-go", "5h", "7d", "mo", "limited", "stale"] {
            assert!(
                plain.contains(needle),
                "missing '{needle}' at width {w} in:\n{plain}"
            );
        }
        assert!(
            raw.contains("\x1b[91m"),
            "limited label must be red at width {w}:\n{raw}"
        );
        for (i, line) in plain.lines().enumerate() {
            assert!(
                line.chars().count() <= w,
                "width {w} overflow at line {i} ({} chars):\n{line}\nfull:\n{plain}",
                line.chars().count()
            );
        }
    }
    Ok(())
}
