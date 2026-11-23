//! Task 1: project identity, membership, tree and `cd` matching (pure functions).

use std::fs;
use std::process::Command;

use agmag::correlate::norm_cwd;
use agmag::project::{
    CdMatch, ProjectKey, ProjectSummary, RootCache, Roots, branch_entry_for, git_common_root,
    list_projects, match_project, members, parties, project_name, tree_order,
};
use agmag::store::{InboxRow, TerminalRow};

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
        term("t3", Some("t9"), Some(r"D:\Project\shop"), "exited", None, 3),
        term("t4", Some("agmag"), Some(r"D:\Project\agmag"), "live", None, 4),
    ];
    let roots = roots(&[
        ("t1", r"D:\Project\shop"),
        ("t2", r"D:\Project\shop"),
        ("t3", r"D:\Project\shop"),
        ("t4", r"D:\Project\agmag"),
    ]);
    let inbox = vec![
        inbox_item(1, Some("t1")),      // address → shop
        inbox_item(2, Some("agmag")),   // name → agmag
        inbox_item(3, Some("x9")),      // unknown terminal → no project
        inbox_item(4, None),            // no terminal → no project
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
        term("t3", Some("other"), Some(r"D:\Project\agmag"), "live", None, 3),
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
    assert_eq!(
        match_project(&list, "sho2"),
        CdMatch::Found(ProjectKey {
            root: norm_cwd(r"D:\Project\shop2"),
            name: "shop2".into()
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
    // t1 ↔ t2 cycle; t3's parent is outside the member set.
    let cyc = vec![
        term("t1", None, None, "live", Some("t2"), 10),
        term("t2", None, None, "live", Some("t1"), 20),
        term("t3", None, None, "live", Some("x9"), 30),
    ];
    let refs: Vec<&TerminalRow> = cyc.iter().collect();
    let order = tree_order(&refs);
    let mut seen: Vec<usize> = order.iter().map(|(i, _, _)| *i).collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2], "every member exactly once: {order:?}");
    // Roots ordered by created_ms: t1 (cycle broken at created order) then t3.
    assert_eq!(order[0], (0, 0, false));
    assert_eq!(order[1], (1, 1, true));
    assert_eq!(order[2], (2, 0, false));

    // A normal 1-root / 2-children tree.
    let tree = vec![
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
    let named = vec![
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
    git(d, &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "b1"]);

    let main = git_common_root(d.to_str().unwrap()).expect("main root");
    let from_wt = git_common_root(wt.to_str().unwrap()).expect("worktree root");
    assert_eq!(norm_cwd(&main), norm_cwd(&from_wt));
    assert_eq!(norm_cwd(&main), norm_cwd(d.to_str().unwrap()));

    let outside = tempfile::tempdir()?;
    assert_eq!(git_common_root(outside.path().to_str().unwrap()), None);
    Ok(())
}
