use agmag::project::tree_order;
use agmag::store::TerminalRow;

fn terminal(
    address: &str,
    name: Option<&str>,
    parent: Option<&str>,
    state: &str,
    created_ms: i64,
) -> TerminalRow {
    TerminalRow {
        address: address.into(),
        name: name.map(Into::into),
        parent: parent.map(Into::into),
        state: state.into(),
        created_ms,
        ..Default::default()
    }
}

#[test]
fn tree_order_prefers_live_then_newest_on_name_collision() {
    // Two terminals share the name "worker": t1 is exited/older, t2 is live/newer.
    // A child references "worker" by name and should attach to the live one (t2).
    let members = [
        terminal("t1", Some("worker"), None, "exited", 10),
        terminal("t2", Some("worker"), None, "live", 20),
        terminal("c1", Some("child"), Some("worker"), "live", 30),
    ];
    let refs: Vec<&TerminalRow> = members.iter().collect();
    let order = tree_order(&refs);

    // Roots walked separately get last_sibling=false; c1 should be child of t2.
    assert_eq!(order, vec![(0, 0, false), (1, 0, false), (2, 1, true)]);
}

#[test]
fn tree_order_prefers_newest_when_both_live_on_name_collision() {
    let members = [
        terminal("t1", Some("worker"), None, "live", 10),
        terminal("t2", Some("worker"), None, "live", 20),
        terminal("c1", Some("child"), Some("worker"), "live", 30),
    ];
    let refs: Vec<&TerminalRow> = members.iter().collect();
    let order = tree_order(&refs);

    // Both live: newest created_ms wins, so c1 attaches to t2.
    assert_eq!(order, vec![(0, 0, false), (1, 0, false), (2, 1, true)]);
}
