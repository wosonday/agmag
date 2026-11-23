//! Task 1: project identity, membership, tree and `cd` matching (pure functions).

use std::fs;
use std::process::Command;

use agmag::config::Config;
use agmag::correlate::norm_cwd;
use agmag::dashboard::snapshot;
use agmag::model::{ContextSnapshot, Role, Source, TurnSummary, Usage, UsageRecord};
use agmag::project::{
    CdMatch, ProjectKey, ProjectSummary, RootCache, Roots, TurnRow, branch_entry_for,
    compute_roots, git_common_root, list_projects, load_project_view, match_project, members,
    parties, project_name, tree_order,
};
use agmag::store::{InboxRow, MessageRow, Store, TerminalRow};

fn term(
    address: &str,
    name: Option<&str>,
    cwd: Option<&str>,
    state: &str,
    parent: Option<&str>,
    created_ms: i64,
) -> TerminalRow {
    TerminalRow {
        address: address.into(),
        name: name.map(Into::into),
        cwd: cwd.map(Into::into),
        state: state.into(),
        parent: parent.map(Into::into),
        created_ms,
        ..Default::default()
    }
}

fn inbox_item(id: i64, terminal: Option<&str>) -> InboxRow {
    InboxRow {
        id,
        terminal: terminal.map(Into::into),
        source: "hook".into(),
        text: format!("item {id}"),
        ..Default::default()
    }
}

fn roots(pairs: &[(&str, &str)]) -> Roots {
    pairs
        .iter()
        .map(|(a, r)| ((*a).to_string(), norm_cwd(r)))
        .collect()
}

fn summary(root: &str, name: &str) -> ProjectSummary {
    ProjectSummary {
        key: ProjectKey {
            root: norm_cwd(root),
            name: name.into(),
        },
        live_agents: 0,
        open_inbox: 0,
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn roots_are_normalized_and_worktrees_join_main() {
    let calls = std::cell::Cell::new(0usize);
    let resolve = |cwd: &str| -> Option<String> {
        calls.set(calls.get() + 1);
        let n = norm_cwd(cwd);
        if n.ends_with(r"shop.wt\a--b") || n == norm_cwd(r"d:\project\shop") {
            Some(r"D:\Project\shop".to_string())
        } else {
            None
        }
    };

    let mut cache = RootCache::new();
    let worktree = cache.root_of("D:/Project/shop.wt/a--b", &resolve);
    let main = cache.root_of(r"d:\project\shop\", &resolve);
    assert_eq!(worktree, main);
    assert_eq!(main, norm_cwd(r"D:\Project\shop"));
    assert_eq!(calls.get(), 2);

    // Same normalized cwd (case / trailing slash) → cached, no second resolve.
    assert_eq!(cache.root_of(r"D:\PROJECT\SHOP", &resolve), main);
    assert_eq!(calls.get(), 2);

    // Failures are cached too; the root falls back to the normalized cwd.
    let other = cache.root_of(r"D:\other", &resolve);
    assert_eq!(other, norm_cwd(r"D:\other"));
    assert_eq!(calls.get(), 3);
    assert_eq!(cache.root_of(r"D:\other\", &resolve), other);
    assert_eq!(calls.get(), 3);
}

#[test]
fn list_projects_counts_live_agents_and_open_inbox() {
    let terms = vec![
        term("t1", Some("ceo"), Some(r"D:\Project\shop"), "live", None, 1),
        term("t2", Some("t7"), Some(r"D:\Project\shop"), "live", None, 2),
        term(
            "t3",
            Some("t9"),
            Some(r"D:\Project\shop"),
            "exited",
            None,
            3,
        ),
        term(
            "t4",
            Some("agmag"),
            Some(r"D:\Project\agmag"),
            "live",
            None,
            4,
        ),
    ];
    let roots = roots(&[
        ("t1", r"D:\Project\shop"),
        ("t2", r"D:\Project\shop"),
        ("t3", r"D:\Project\shop"),
        ("t4", r"D:\Project\agmag"),
    ]);
    let inbox = vec![
        inbox_item(1, Some("t1")),    // address → shop
        inbox_item(2, Some("agmag")), // name → agmag
        inbox_item(3, Some("x9")),    // unknown terminal → no project
        inbox_item(4, None),          // no terminal → no project
        InboxRow {
            resolved_ms: Some(10),
            ..inbox_item(5, Some("t1"))
        }, // closed → not counted
    ];

    let (list, no_project) = list_projects(&terms, &inbox, &roots);
    assert_eq!(no_project, 2);
    assert_eq!(list.len(), 2);
    // sorted by name
    assert_eq!(list[0].key.name, "agmag");
    assert_eq!(list[0].live_agents, 1);
    assert_eq!(list[0].open_inbox, 1);
    assert_eq!(list[1].key.name, "shop");
    assert_eq!(list[1].live_agents, 2); // exited t3 is not live
    assert_eq!(list[1].open_inbox, 1);
    assert_eq!(list[1].key.root, norm_cwd(r"D:\Project\shop"));
}

#[test]
fn membership_matches_address_or_name() {
    let terms = vec![
        term("t1", Some("ceo"), Some(r"D:\Project\shop"), "live", None, 1),
        term("t2", None, Some(r"D:\Project\shop"), "live", None, 2),
        term(
            "t3",
            Some("other"),
            Some(r"D:\Project\agmag"),
            "live",
            None,
            3,
        ),
    ];
    let roots = roots(&[
        ("t1", r"D:\Project\shop"),
        ("t2", r"D:\Project\shop"),
        ("t3", r"D:\Project\agmag"),
    ]);
    let key = ProjectKey {
        root: norm_cwd(r"D:\Project\shop"),
        name: "shop".into(),
    };
    let got = members(&terms, &roots, &key);
    assert_eq!(got.len(), 2);
    assert_eq!(
        got.iter().map(|t| t.address.as_str()).collect::<Vec<_>>(),
        vec!["t1", "t2"]
    );

    let ps = parties(&got);
    assert!(ps.contains(&"t1".to_string()), "address missing: {ps:?}");
    assert!(ps.contains(&"ceo".to_string()), "name missing: {ps:?}");

    // An inbox row naming the terminal by name still counts for the project.
    let inbox = vec![inbox_item(1, Some("ceo"))];
    let (list, no_project) = list_projects(&terms, &inbox, &roots);
    assert_eq!(no_project, 0);
    let shop = list.iter().find(|p| p.key.name == "shop").unwrap();
    assert_eq!(shop.open_inbox, 1);
}

#[test]
fn match_project_rules() {
    // Exact name (case-insensitive) wins over a longer prefix.
    let list = vec![
        summary(r"D:\Project\shop", "shop"),
        summary(r"D:\Project\shop2", "shop2"),
    ];
    assert_eq!(
        match_project(&list, "SHOP"),
        CdMatch::Found(ProjectKey {
            root: norm_cwd(r"D:\Project\shop"),
            name: "shop".into()
        })
    );

    // Unique prefix.
    let unique = vec![
        summary(r"D:\Project\shop", "shop"),
        summary(r"D:\Project\agmag", "agmag"),
    ];
    assert_eq!(
        match_project(&unique, "sho"),
        CdMatch::Found(ProjectKey {
            root: norm_cwd(r"D:\Project\shop"),
            name: "shop".into()
        })
    );

    // Two prefixes → ambiguous, plain names.
    let prefixes = vec![
        summary(r"D:\Project\shop2", "shop2"),
        summary(r"D:\Project\shopx", "shopx"),
    ];
    assert_eq!(
        match_project(&prefixes, "shop"),
        CdMatch::Ambiguous(vec!["shop2".into(), "shopx".into()])
    );

    // Exact root path.
    assert_eq!(
        match_project(&list, r"D:\Project\shop2"),
        CdMatch::Found(ProjectKey {
            root: norm_cwd(r"D:\Project\shop2"),
            name: "shop2".into()
        })
    );

    // Duplicate names → ambiguous entries `name (root)`.
    let dups = vec![
        summary(r"D:\Project\shop", "shop"),
        summary(r"D:\Project\other", "shop"),
    ];
    let mut expected = vec![
        format!("shop ({})", norm_cwd(r"D:\Project\other")),
        format!("shop ({})", norm_cwd(r"D:\Project\shop")),
    ];
    expected.sort();
    assert_eq!(match_project(&dups, "shop"), CdMatch::Ambiguous(expected));

    assert_eq!(match_project(&list, "zzz"), CdMatch::NotFound);
    assert_eq!(match_project(&list, "  "), CdMatch::NotFound);
}

#[test]
fn tree_survives_cycles_and_foreign_parents() {
    // t1 ↔ t2 cycle; t3's parent is outside the member set. The real root (t3) comes
    // first; the cycle is then broken at its oldest node (t1).
    let cyc = [
        term("t1", None, None, "live", Some("t2"), 10),
        term("t2", None, None, "live", Some("t1"), 20),
        term("t3", None, None, "live", Some("x9"), 30),
    ];
    let refs: Vec<&TerminalRow> = cyc.iter().collect();
    let order = tree_order(&refs);
    let mut seen: Vec<usize> = order.iter().map(|(i, _, _)| *i).collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2], "every member exactly once: {order:?}");
    assert_eq!(order[0], (2, 0, false));
    assert_eq!(order[1], (0, 0, false));
    assert_eq!(order[2], (1, 1, true));

    // Roots ordered by created_ms, not by input order.
    let two_roots = [
        term("late", None, None, "live", None, 90),
        term("early", None, None, "live", None, 5),
    ];
    let refs: Vec<&TerminalRow> = two_roots.iter().collect();
    assert_eq!(tree_order(&refs), vec![(1, 0, false), (0, 0, false)]);

    // A normal 1-root / 2-children tree.
    let tree = [
        term("r", None, None, "live", None, 1),
        term("c1", None, None, "live", Some("r"), 2),
        term("c2", None, None, "live", Some("r"), 3),
    ];
    let tree_refs: Vec<&TerminalRow> = tree.iter().collect();
    assert_eq!(
        tree_order(&tree_refs),
        vec![(0, 0, false), (1, 1, false), (2, 1, true)]
    );

    // Parents resolve by name too.
    let named = [
        term("t1", Some("ceo"), None, "live", None, 1),
        term("t2", None, None, "live", Some("ceo"), 2),
    ];
    let named_refs: Vec<&TerminalRow> = named.iter().collect();
    assert_eq!(tree_order(&named_refs), vec![(0, 0, false), (1, 1, true)]);
}

#[test]
fn branch_entry_from_worktree_dir_or_task() {
    assert_eq!(
        branch_entry_for(Some(r"D:\x\.wt\tap-hoa--low-stock"), None),
        Some("tap-hoa/low-stock".into())
    );
    assert_eq!(
        branch_entry_for(Some(r"D:\x"), Some("a--b")),
        Some("a/b".into())
    );
    assert_eq!(branch_entry_for(Some(r"D:\x\Bad--Case"), None), None);
    assert_eq!(branch_entry_for(Some(r"D:\x"), None), None);
    assert_eq!(
        project_name(&norm_cwd(r"D:\Project\shop")),
        "shop".to_string()
    );
}

#[test]
fn git_common_root_on_real_repo_and_worktree() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.invalid"]);
    git(d, &["config", "user.name", "t"]);
    fs::write(d.join("a.txt"), "1")?;
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "init"]);

    let base = tempfile::tempdir()?;
    let wt = base.path().join("wt");
    git(
        d,
        &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "b1"],
    );

    let main = git_common_root(d.to_str().unwrap()).expect("main root");
    let from_wt = git_common_root(wt.to_str().unwrap()).expect("worktree root");
    assert_eq!(norm_cwd(&main), norm_cwd(&from_wt));
    assert_eq!(norm_cwd(&main), norm_cwd(d.to_str().unwrap()));

    let outside = tempfile::tempdir()?;
    assert_eq!(git_common_root(outside.path().to_str().unwrap()), None);
    Ok(())
}

/// Each row sums to `1 + 2 + 3 + 4 = 10` spark tokens; the large `cache_read` is excluded there
/// but included in the snapshot's session total (`1 + 1000 + 2 + 3 = 1006`).
fn usage_rec(ts_ms: i64, key: &str) -> UsageRecord {
    UsageRecord {
        dedupe_key: key.into(),
        ts_ms,
        model: None,
        usage: Usage {
            input_uncached: 1,
            cache_read: 1000,
            cache_write: 2,
            output: 3,
            reasoning: 4,
        },
    }
}

#[test]
fn load_project_view_joins_snapshot_and_store() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().to_str().unwrap().to_string();
    let root_norm = norm_cwd(&root);
    fs::write(tmp.path().join("BRANCHES.md"), "### a/b\n- status: wip\n")?;
    let wt = format!(r"{root}\.wt\a--b");

    let store = Store::open_in_memory()?;
    let now: i64 = 4_000_000;

    // Live agent t1: linked session with model/effort/context, a tool turn and usage rows.
    let s1 = store.session_id_for_path(Source::Codex, "p1")?;
    store.set_model(s1, "gpt-6.1")?;
    store.set_effort(s1, "high")?;
    store.set_context(
        s1,
        &ContextSnapshot {
            ts_ms: now - 1_000,
            used_tokens: 50_000,
            window_tokens: Some(200_000),
        },
    )?;
    store.insert_turn(
        s1,
        &TurnSummary {
            ts_ms: now - 12_000,
            role: Role::Tool,
            summary: "[tool] Bash".into(),
            byte_offset: 10,
        },
    )?;
    // One row in the oldest bucket, two in the newest (window = last 60 min, 12 x 5 min).
    store.upsert_usage(s1, &usage_rec(now - 3_600_000 + 1_000, "u0"))?;
    store.upsert_usage(s1, &usage_rec(now - 100_000, "u1"))?;
    store.upsert_usage(s1, &usage_rec(now - 50_000, "u2"))?;

    // Live agent t2: worktree cwd → BRANCHES.md entry a/b; linked session without usage rows.
    let s2 = store.session_id_for_path(Source::Codex, "p2")?;
    store.insert_turn(
        s2,
        &TurnSummary {
            ts_ms: now - 40_000,
            role: Role::Assistant,
            summary: "Implementing the parser".into(),
            byte_offset: 20,
        },
    )?;

    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        name: Some("ceo".into()),
        app: Some("claude-code".into()),
        cwd: Some(root.clone()),
        state: "live".into(),
        created_ms: 1,
        ..Default::default()
    })?;
    store.set_terminal_status("t1", Some("working"), None, None, 1, now)?;
    store.link_terminal("t1", s1, "auto")?;
    store.upsert_terminal(&TerminalRow {
        address: "t2".into(),
        app: Some("codex".into()),
        cwd: Some(wt.clone()),
        state: "live".into(),
        parent: Some("ceo".into()),
        created_ms: 2,
        ..Default::default()
    })?;
    store.link_terminal("t2", s2, "auto")?;
    // Exited t3 stays only because an open inbox item names it (by name); t4 has none → excluded.
    store.upsert_terminal(&TerminalRow {
        address: "t3".into(),
        name: Some("t9".into()),
        app: Some("opencode".into()),
        cwd: Some(root.clone()),
        state: "exited".into(),
        created_ms: 3,
        ..Default::default()
    })?;
    store.upsert_terminal(&TerminalRow {
        address: "t4".into(),
        cwd: Some(root.clone()),
        state: "exited".into(),
        created_ms: 4,
        ..Default::default()
    })?;

    let i1 = store
        .open_inbox(Some("t1"), "hook", Some("question"), "keep API v1?", now)?
        .unwrap();
    let i2 = store
        .open_inbox(Some("t9"), "hook", Some("blocked"), "needs a decision", now)?
        .unwrap();
    store.open_inbox(Some("x9"), "hook", None, "outside the project", now)?;
    let closed = store
        .open_inbox(Some("t1"), "hook", None, "already answered", now - 10)?
        .unwrap();
    store.resolve_inbox(closed, None, now)?;

    // 25 messages inside the project (by name/address), 3 outside, 1 from the exited member.
    for i in 0..25i64 {
        store.add_message(&MessageRow {
            ts_ms: now - (25 - i) * 1_000,
            from: "ceo".into(),
            to: "t2".into(),
            kind: "GIAO".into(),
            text: format!("m{i:02}"),
            status: "sent".into(),
            ..Default::default()
        })?;
    }
    for i in 0..3i64 {
        store.add_message(&MessageRow {
            ts_ms: now - i * 1_000,
            from: "x9".into(),
            to: "y8".into(),
            kind: "GIAO".into(),
            text: format!("out{i}"),
            status: "sent".into(),
            ..Default::default()
        })?;
    }
    store.add_message(&MessageRow {
        ts_ms: now - 500,
        from: "t3".into(),
        to: "ceo".into(),
        kind: "BÁO CÁO".into(),
        text: "done".into(),
        status: "sent".into(),
        ..Default::default()
    })?;

    let snap = snapshot(&store, &Config::builtin(), now)?;
    let terms = store.terminals(false)?;
    let resolve = |cwd: &str| -> Option<String> {
        let n = norm_cwd(cwd);
        if n == root_norm || n == norm_cwd(&wt) {
            Some(root.clone())
        } else {
            None
        }
    };
    let mut cache = RootCache::new();
    let roots = compute_roots(&terms, &snap, &mut cache, &resolve);
    assert_eq!(roots.len(), 4);
    let key = ProjectKey {
        root: root_norm.clone(),
        name: project_name(&root_norm),
    };

    let pv = load_project_view(&store, &snap, &key, &roots, now)?;
    assert_eq!(pv.key, key);

    // t4 excluded; tree order: ceo root, t2 its only child, exited t9 a second root.
    let addrs: Vec<&str> = pv.agents.iter().map(|a| a.address.as_str()).collect();
    assert_eq!(addrs, vec!["t1", "t2", "t3"]);
    let (a1, a2, a3) = (&pv.agents[0], &pv.agents[1], &pv.agents[2]);
    assert_eq!(a1.label, "ceo");
    assert_eq!(a1.app, "claude-code");
    assert_eq!(a1.status, "working");
    assert_eq!(a1.state, "live");
    assert_eq!(a1.depth, 0);
    assert_eq!(a2.label, "t2"); // no name → address
    assert_eq!(a2.depth, 1);
    assert!(a2.last_sibling);
    assert_eq!(a3.depth, 0);
    assert_eq!(a3.state, "exited");
    assert_eq!(a3.status, "-"); // no herdr status → "-", never guessed

    // Live fields copied from the snapshot TermView with the same address.
    let tv1 = snap.terminals.iter().find(|v| v.address == "t1").unwrap();
    assert_eq!(a1.model, tv1.model);
    assert_eq!(a1.effort, tv1.effort);
    assert_eq!(a1.tokens, tv1.tokens);
    assert_eq!(a1.context_pct, tv1.context_pct);
    assert_eq!(a1.last_msg, tv1.last_msg);
    assert_eq!(a1.model, "gpt-6.1");
    assert_eq!(a1.effort, "high");
    assert_eq!(a1.tokens, Some(3 * 1006));
    assert_eq!(a1.context_pct, Some(25.0));
    assert_eq!(a1.last_msg, Some(("REPORT".into(), false)));
    assert_eq!(a2.last_msg, Some(("ASSIGN".into(), false)));

    // Exited agent: no snapshot fields, no linked session.
    assert_eq!(a3.model, "");
    assert_eq!(a3.effort, "");
    assert_eq!(a3.tokens, None);
    assert_eq!(a3.context_pct, None);
    assert_eq!(a3.last_msg, None);
    assert_eq!(a3.doing, None);
    assert_eq!(a3.spark, None);

    // doing = latest assistant/tool turn of the linked session.
    assert_eq!(
        a1.doing,
        Some(TurnRow {
            ts_ms: now - 12_000,
            role: "tool".into(),
            summary: "[tool] Bash".into(),
        })
    );
    assert_eq!(
        a2.doing.as_ref().map(|t| t.summary.as_str()),
        Some("Implementing the parser")
    );

    // spark: 12 buckets, oldest first; linked session without usage → 12 zeros.
    let sp1 = a1.spark.as_ref().unwrap();
    assert_eq!(sp1.len(), 12);
    assert_eq!(sp1[0], 10);
    assert_eq!(sp1[11], 20);
    assert!(sp1[1..11].iter().all(|&b| b == 0));
    assert_eq!(a2.spark, Some(vec![0u64; 12]));

    // stage from <root>/BRANCHES.md via the worktree directory name a--b → a/b.
    assert_eq!(a2.stage.as_deref(), Some("wip"));
    assert_eq!(a1.stage, None);
    assert_eq!(a3.stage, None);

    // inbox: open items of project parties only (address or name), closed/foreign excluded.
    assert_eq!(
        pv.inbox.iter().map(|i| i.id).collect::<Vec<_>>(),
        vec![i1, i2]
    );

    // messages: newest 20 involving a party (exited member included), newest first.
    assert_eq!(pv.messages.len(), 20);
    assert!(pv.messages.windows(2).all(|w| w[0].ts_ms > w[1].ts_ms));
    assert_eq!(pv.messages[0].text, "done");
    assert!(pv.messages.iter().all(|m| m.from != "x9" && m.to != "x9"));
    Ok(())
}

#[test]
fn compute_roots_falls_back_to_live_termview_cwd() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().to_str().unwrap().to_string();
    let store = Store::open_in_memory()?;
    let sid = store.session_id_for_path(Source::Codex, "p1")?;
    store.set_session_identity(sid, "conv-1", Some(&root))?;

    // t1: no row cwd → the live TermView.cwd (session cwd) is used.
    store.upsert_terminal(&TerminalRow {
        address: "t1".into(),
        state: "live".into(),
        created_ms: 1,
        ..Default::default()
    })?;
    store.link_terminal("t1", sid, "auto")?;
    // t2: empty row cwd, exited (no TermView) → skipped.
    store.upsert_terminal(&TerminalRow {
        address: "t2".into(),
        cwd: Some(String::new()),
        state: "exited".into(),
        created_ms: 2,
        ..Default::default()
    })?;
    // t3: exited but has a row cwd → still resolved.
    store.upsert_terminal(&TerminalRow {
        address: "t3".into(),
        cwd: Some(root.clone()),
        state: "exited".into(),
        created_ms: 3,
        ..Default::default()
    })?;

    let snap = snapshot(&store, &Config::builtin(), 1_000_000)?;
    let terms = store.terminals(false)?;
    let calls = std::cell::Cell::new(0usize);
    let resolve = |_: &str| -> Option<String> {
        calls.set(calls.get() + 1);
        Some(root.clone())
    };
    let mut cache = RootCache::new();
    let roots = compute_roots(&terms, &snap, &mut cache, &resolve);
    let rn = norm_cwd(&root);
    assert_eq!(roots.get("t1").map(String::as_str), Some(rn.as_str()));
    assert!(!roots.contains_key("t2"));
    assert_eq!(roots.get("t3").map(String::as_str), Some(rn.as_str()));
    // t1 and t3 share one normalized cwd → the resolver runs once.
    assert_eq!(calls.get(), 1);
    Ok(())
}
