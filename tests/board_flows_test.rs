//! Task 1: parent/child flow edges and the bounded pair-latest store query.

use std::collections::HashSet;

use agmag::board::flows::{self, Edge};
use agmag::project::AgentView;
use agmag::store::{MessageRow, Store};

fn agent(address: &str, label: &str, depth: usize) -> AgentView {
    AgentView {
        address: address.into(),
        label: label.into(),
        app: String::new(),
        model: String::new(),
        effort: String::new(),
        status: "-".into(),
        state: "live".into(),
        depth,
        last_sibling: false,
        doing: None,
        last_msg: None,
        context_pct: None,
        tokens: None,
        spark: None,
        stage: None,
        task: None,
        role: None,
    }
}

fn msg(id: i64, ts_ms: i64, from: &str, to: &str, kind: &str) -> MessageRow {
    MessageRow {
        id,
        ts_ms,
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        text: format!("m{id}"),
        status: "sent".into(),
        ..Default::default()
    }
}

#[test]
fn edges_direction_agnostic_and_newest_wins() {
    let agents = vec![agent("p1", "ceo", 0), agent("c1", "c1", 1)];
    let rows = vec![
        msg(1, 10, "ceo", "c1", "GIAO"),
        msg(2, 20, "c1", "p1", "HỎI"),
    ];
    assert_eq!(
        flows::edges(&agents, &rows),
        vec![Edge {
            parent: 0,
            child: 1,
            kind: "QUESTION".into(),
            ts_ms: 20,
        }]
    );
}

#[test]
fn edges_skip_pairs_without_messages() {
    let agents = vec![
        agent("p1", "p1", 0),
        agent("c1", "c1", 1),
        agent("c2", "c2", 1),
    ];
    // Only p1 <-> c1 exchanged a message; c2 has none.
    let rows = vec![msg(1, 10, "p1", "c1", "GIAO")];
    assert_eq!(
        flows::edges(&agents, &rows),
        vec![Edge {
            parent: 0,
            child: 1,
            kind: "ASSIGN".into(),
            ts_ms: 10,
        }]
    );
}

#[test]
fn edges_ignore_messages_to_non_parent() {
    let agents = vec![
        agent("p1", "p1", 0),
        agent("c1", "c1", 1),
        agent("g1", "g1", 2),
    ];
    // p1 <-> g1 is grandparent/grandchild, not a direct parent-child pair;
    // c1 <-> g1 is the real parent-child pair.
    let rows = vec![
        msg(1, 5, "p1", "g1", "GIAO"),
        msg(2, 15, "c1", "g1", "BÁO CÁO"),
    ];
    assert_eq!(
        flows::edges(&agents, &rows),
        vec![Edge {
            parent: 1,
            child: 2,
            kind: "REPORT".into(),
            ts_ms: 15,
        }]
    );
}

#[test]
fn pair_latest_one_row_per_unordered_pair() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    // Ids are auto-assigned 1..=4 in insertion order.
    store.add_message(&msg(0, 10, "a", "b", "GIAO"))?;
    store.add_message(&msg(0, 20, "b", "a", "HỎI"))?;
    store.add_message(&msg(0, 30, "a", "c", "BÁO CÁO"))?;
    store.add_message(&msg(0, 40, "a", "x", "LỖI"))?;

    let parties = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let rows = store.pair_latest_messages(&parties)?;
    let ids: HashSet<i64> = rows.iter().map(|r| r.id).collect();
    assert_eq!(ids, HashSet::from([2, 3]));

    // The b→a row (highest id) wins for {a,b}; a→c is the only {a,c} row.
    let ab = rows.iter().find(|r| r.id == 2).expect("ab row");
    assert_eq!(ab.from, "b");
    assert_eq!(ab.to, "a");
    assert_eq!(ab.kind, "HỎI");
    let ac = rows.iter().find(|r| r.id == 3).expect("ac row");
    assert_eq!(ac.from, "a");
    assert_eq!(ac.to, "c");
    assert_eq!(ac.kind, "BÁO CÁO");

    // Empty parties → empty result, no SQL error.
    assert!(store.pair_latest_messages(&[])?.is_empty());
    Ok(())
}
