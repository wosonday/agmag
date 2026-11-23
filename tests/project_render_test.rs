use agmag::project::{AgentView, ProjectKey, ProjectSummary, ProjectView, TurnRow, tree_order};
use agmag::project_render::{render_inbox_item, render_project_body, render_project_list};
use agmag::store::{InboxRow, MessageRow, TerminalRow};
use unicode_width::UnicodeWidthStr;

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

fn key(name: &str) -> ProjectKey {
    ProjectKey {
        root: format!("D:\\Project\\{name}"),
        name: name.into(),
    }
}

fn agent(address: &str, depth: usize, last_sibling: bool) -> AgentView {
    AgentView {
        address: address.into(),
        label: address.into(),
        app: String::new(),
        model: String::new(),
        effort: String::new(),
        status: "-".into(),
        state: "live".into(),
        depth,
        last_sibling,
        doing: None,
        last_msg: None,
        context_pct: None,
        tokens: None,
        spark: None,
        stage: None,
    }
}

fn terminal(
    address: &str,
    name: Option<&str>,
    parent: Option<&str>,
    created_ms: i64,
) -> TerminalRow {
    TerminalRow {
        address: address.into(),
        name: name.map(Into::into),
        cwd: None,
        parent: parent.map(Into::into),
        state: "live".into(),
        created_ms,
        ..Default::default()
    }
}

fn view(agents: Vec<AgentView>, inbox: Vec<InboxRow>, messages: Vec<MessageRow>) -> ProjectView {
    ProjectView {
        key: key("agmag"),
        agents,
        inbox,
        messages,
    }
}

/// Lines between the line containing `start` and the line containing `end`.
fn section_lines<'a>(out: &'a str, start: &str, end: Option<&str>) -> Vec<&'a str> {
    let mut inside = false;
    let mut lines = Vec::new();
    for line in out.lines() {
        if line.contains(start) {
            inside = true;
            continue;
        }
        if let Some(end) = end
            && line.contains(end)
        {
            break;
        }
        if inside {
            lines.push(line);
        }
    }
    lines
}

#[test]
fn agent_lines_show_tree_status_doing_and_unknowns() {
    let now = 1_700_000_000_000i64;

    let mut root = agent("ceo", 0, false);
    root.app = "claude-code".into();
    root.model = "opus".into();
    root.effort = "high".into();
    root.status = "working".into();
    root.doing = Some(TurnRow {
        ts_ms: now - 12_000,
        role: "tool".into(),
        summary: "[tool] Bash".into(),
    });
    root.context_pct = Some(72.0);
    root.tokens = Some(1_200_000);
    root.spark = Some(vec![1; 12]);

    let mut mid = agent("t7", 1, false);
    mid.app = "codex".into();
    mid.model = "gpt-6.1".into();
    mid.status = "running".into();

    let mut leaf = agent("t9", 1, true);
    leaf.status = "BLOCKED: needs a decision".into();
    leaf.state = "exited".into();
    leaf.last_msg = Some(("REPORT".into(), true));

    let out = render_project_body(&view(vec![root, mid, leaf], vec![], vec![]), 110, now);
    let plain = strip_ansi(&out);

    assert!(plain.contains("● working"), "{plain}");
    assert!(plain.contains("● running"), "{plain}");
    assert!(plain.contains("◐ blocked"), "{plain}");
    assert!(plain.contains("doing: [tool] Bash · 12s ago"), "{plain}");
    assert!(
        plain.contains("doing: -  · last msg: REPORT sent"),
        "{plain}"
    );
    // Root `last_sibling` is false, exactly as `tree_order` produces at depth 0: the tree
    // must not paint a dangling `│` column for the root.
    assert!(plain.contains("\n ├ t7"), "{plain}");
    assert!(plain.contains("\n └ t9"), "{plain}");
    let root_doing = plain
        .lines()
        .find(|line| line.contains("doing: [tool] Bash"))
        .expect("root doing line");
    assert!(
        !root_doing.contains('│'),
        "root doing line has a dangling column: {root_doing:?}"
    );
    assert!(plain.contains("AGENTS · agmag (3)"), "{plain}");

    let leaf_line = plain
        .lines()
        .find(|line| line.contains("t9") && line.contains("ctx"))
        .expect("leaf agent line");
    assert!(leaf_line.contains("ctx -"), "{leaf_line}");
    // Unknown sparkline and unknown token total both render as `-`.
    assert!(
        leaf_line.matches("  -").count() >= 2,
        "leaf line: {leaf_line}"
    );
}

#[test]
fn tree_prefix_follows_tree_order_output() {
    let now = 1_700_000_000_000i64;
    let terms = [
        terminal("ceo", Some("ceo"), None, 1),
        terminal("t7", Some("t7"), Some("ceo"), 2),
        terminal("t9", Some("t9"), Some("ceo"), 3),
    ];
    let refs: Vec<&TerminalRow> = terms.iter().collect();
    let order = tree_order(&refs);
    assert_eq!(order, vec![(0, 0, false), (1, 1, false), (2, 1, true)]);

    let agents: Vec<AgentView> = order
        .iter()
        .map(|&(index, depth, last_sibling)| {
            let mut a = agent(&refs[index].address, depth, last_sibling);
            a.label = refs[index]
                .name
                .clone()
                .unwrap_or_else(|| refs[index].address.clone());
            if depth == 0 {
                a.doing = Some(TurnRow {
                    ts_ms: now,
                    role: "assistant".into(),
                    summary: "root work".into(),
                });
            }
            a
        })
        .collect();

    let plain = strip_ansi(&render_project_body(
        &view(agents, vec![], vec![]),
        110,
        now,
    ));
    assert!(plain.contains("\n ├ t7"), "{plain}");
    assert!(plain.contains("\n └ t9"), "{plain}");
    let root_doing = plain
        .lines()
        .find(|line| line.contains("doing: root work"))
        .expect("root doing line");
    assert!(
        !root_doing.contains('│'),
        "root doing line has a dangling column: {root_doing:?}"
    );
}

#[test]
fn sparkline_scales_to_max_and_zero_is_flat() {
    let mut peak = agent("peak", 0, true);
    peak.tokens = Some(0);
    peak.spark = Some(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10]);
    let plain = strip_ansi(&render_project_body(
        &view(vec![peak], vec![], vec![]),
        110,
        0,
    ));
    assert!(plain.contains("▁▁▁▁▁▁▁▁▁▁▁█"), "{plain}");

    let mut flat = agent("flat", 0, true);
    flat.tokens = Some(0);
    flat.spark = Some(vec![0; 12]);
    let plain = strip_ansi(&render_project_body(
        &view(vec![flat], vec![], vec![]),
        110,
        0,
    ));
    assert!(plain.contains("▁▁▁▁▁▁▁▁▁▁▁▁"), "{plain}");
}

#[test]
fn inbox_wraps_to_four_lines_messages_to_three() {
    let now = 1_700_000_000_000i64;
    let inbox = vec![InboxRow {
        id: 43,
        ts_ms: now,
        terminal: Some("t9".into()),
        source: "blocked".into(),
        text: "x".repeat(1000),
        ..Default::default()
    }];
    let messages = vec![MessageRow {
        id: 1,
        ts_ms: now - 300_000,
        from: "ceo".into(),
        to: "t7".into(),
        kind: "BÁO CÁO".into(),
        text: "y".repeat(1000),
        ..Default::default()
    }];
    let plain = strip_ansi(&render_project_body(
        &view(vec![], inbox, messages),
        110,
        now,
    ));

    let inbox_block = section_lines(&plain, "INBOX ·", Some("MESSAGES ·"));
    assert_eq!(inbox_block.len(), 4, "{inbox_block:#?}");
    assert!(inbox_block.last().unwrap().trim_end().ends_with('…'));

    let message_block = section_lines(&plain, "MESSAGES ·", None);
    assert_eq!(message_block.len(), 3, "{message_block:#?}");
    assert!(message_block.last().unwrap().ends_with('…'));
    assert!(plain.contains("5m"), "{plain}");
}

#[test]
fn untrusted_text_is_inert() {
    let evil = "pwn\x1b]0;pwn\x07\x1b[2J\r\nsecond";
    let mut agent = agent("a", 0, true);
    agent.doing = Some(TurnRow {
        ts_ms: 0,
        role: "assistant".into(),
        summary: evil.into(),
    });
    let inbox = vec![InboxRow {
        id: 1,
        text: evil.into(),
        ..Default::default()
    }];
    let messages = vec![MessageRow {
        from: "a".into(),
        to: "b".into(),
        kind: "REPORT".into(),
        text: evil.into(),
        ..Default::default()
    }];
    let out = render_project_body(&view(vec![agent], inbox, messages), 110, 0);

    assert!(!out.contains("\x1b]"), "OSC survived: {out:?}");
    assert!(!out.contains("\x1b[2J"), "CSI clear survived: {out:?}");
    assert!(!out.contains('\r'), "raw CR survived: {out:?}");
    assert!(out.contains("pwn"), "{out:?}");
}

#[test]
fn no_line_exceeds_width_at_40_60_110() {
    let now = 1_700_000_000_000i64;

    let mut wide = agent("日本团队🚀", 0, false);
    wide.label = "日本团队🚀".into();
    wide.app = "claude-code".into();
    wide.model = "模型超長名稱".into();
    wide.effort = "high".into();
    wide.status = "working".into();
    wide.doing = Some(TurnRow {
        ts_ms: now - 3_600_000,
        role: "assistant".into(),
        summary: "Đang triển khai bộ phân tích cú pháp cho tệp rất dài và phức tạp 🚀🚀🚀".into(),
    });
    wide.context_pct = Some(99.0);
    wide.tokens = Some(9_999_999);
    wide.spark = Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    wide.stage = Some("wip".into());

    let mut narrow = agent("🚀", 1, true);
    narrow.label = "🚀".into();

    let inbox = vec![InboxRow {
        id: 999,
        terminal: Some("日本团队🚀".into()),
        source: "blocked".into(),
        text: "📬".repeat(400),
        ..Default::default()
    }];
    let messages = vec![MessageRow {
        from: "日本团队🚀".into(),
        to: "🚀".into(),
        kind: "REPORT".into(),
        text: "✨".repeat(400),
        ..Default::default()
    }];
    let pv = view(vec![wide, narrow], inbox, messages);

    for width in [40usize, 60, 110] {
        let plain = strip_ansi(&render_project_body(&pv, width, now));
        for line in plain.lines() {
            let line_width = UnicodeWidthStr::width(line);
            assert!(
                line_width < width,
                "width {width}: {line:?} is {line_width} cells"
            );
        }
        if width == 60 {
            assert!(
                !plain.contains('▂'),
                "at 60 the sparkline must be dropped: {plain}"
            );
            assert!(plain.contains("ctx -"), "at 60 ctx stays: {plain}");
        }
        if width == 40 {
            assert!(
                !plain.contains("ctx"),
                "at 40 the ctx bar must be dropped: {plain}"
            );
        }
        if width == 110 {
            assert!(
                plain.contains('▂'),
                "at 110 the sparkline is shown: {plain}"
            );
        }
    }
}

#[test]
fn zero_width_does_not_panic() {
    let now = 1_700_000_000_000i64;
    let mut a = agent("ceo", 0, false);
    a.doing = Some(TurnRow {
        ts_ms: now,
        role: "tool".into(),
        summary: "Bash".into(),
    });
    let inbox = vec![InboxRow {
        id: 1,
        terminal: Some("ceo".into()),
        text: "x".repeat(100),
        ..Default::default()
    }];
    let messages = vec![MessageRow {
        from: "ceo".into(),
        to: "t7".into(),
        kind: "REPORT".into(),
        text: "y".repeat(100),
        ..Default::default()
    }];
    let out = render_project_body(&view(vec![a], inbox, messages), 0, now);
    for line in strip_ansi(&out).lines() {
        assert_eq!(UnicodeWidthStr::width(line), 0, "{line:?}");
    }

    let _ = render_project_list(&[], 1, 0);
    let _ = render_inbox_item(
        &InboxRow {
            text: "x".repeat(10),
            ..Default::default()
        },
        0,
    );
}

#[test]
fn project_list_and_show() {
    let list = vec![
        ProjectSummary {
            key: key("agmag"),
            live_agents: 4,
            open_inbox: 2,
        },
        ProjectSummary {
            key: key("shop"),
            live_agents: 1,
            open_inbox: 0,
        },
        ProjectSummary {
            key: key("long-project-name"),
            live_agents: 2,
            open_inbox: 1,
        },
    ];

    let plain = strip_ansi(&render_project_list(&list, 3, 60));
    assert!(plain.contains("agmag"), "{plain}");
    assert!(plain.contains("4 live · 2 inbox"), "{plain}");
    assert!(plain.contains("shop"), "{plain}");
    assert!(plain.contains("1 live · 0 inbox"), "{plain}");
    assert!(plain.contains("(no project)"), "{plain}");
    assert!(plain.contains("3 inbox"), "{plain}");
    assert!(plain.contains("long-project-name"), "{plain}");
    // A 17-cell name widens the name column past `(no project)`: the `(no project)` name
    // cell must be padded to that width like every project row (the painted name must not
    // be measured by `pad_cells`).
    let expected_row = format!(
        "  (no project){}  - · 3 inbox",
        " ".repeat("long-project-name".len() - "(no project)".len())
    );
    assert!(
        plain.lines().any(|line| line == expected_row),
        "expected {expected_row:?}:\n{plain}"
    );
    assert_eq!(plain.matches("inbox").count(), 4, "{plain}");
    for line in plain.lines() {
        assert!(UnicodeWidthStr::width(line) <= 59, "{line:?}");
    }

    let without = strip_ansi(&render_project_list(&list, 0, 60));
    assert!(!without.contains("(no project)"), "{without}");

    let item = InboxRow {
        id: 43,
        terminal: Some("t9".into()),
        source: "blocked".into(),
        text: "x".repeat(1000),
        ..Default::default()
    };
    let shown = strip_ansi(&render_inbox_item(&item, 60));
    assert!(!shown.contains('…'), "full item must not be cut: {shown}");
    assert_eq!(shown.chars().filter(|c| *c == 'x').count(), 1000);
    for line in shown.lines() {
        assert!(UnicodeWidthStr::width(line) <= 59, "{line:?}");
    }
}
