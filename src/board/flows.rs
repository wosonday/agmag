//! Flow edges between a parent agent and its children, from the latest message per pair.

use crate::project::AgentView;
use crate::store::MessageRow;

/// A message younger than this makes an edge "live".
pub const LIVE_MS: i64 = 300_000;

/// Latest message between `agents[parent]` and `agents[child]` (either direction).
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub parent: usize,
    pub child: usize,
    /// English kind (`ASSIGN`, `QUESTION`, ...).
    pub kind: String,
    pub ts_ms: i64,
}

/// Parent index per agent. `agents` is in display (pre-order) order, so the parent of an agent at
/// depth d > 0 is the nearest preceding agent at depth d − 1.
pub fn parents(agents: &[AgentView]) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    let mut out = Vec::with_capacity(agents.len());
    for (i, a) in agents.iter().enumerate() {
        stack.truncate(a.depth);
        let parent = if a.depth == 0 {
            None
        } else {
            stack.last().copied()
        };
        out.push(parent);
        // Keep the stack indexed by depth even when depths skip a level.
        if let Some(p) = parent {
            while stack.len() < a.depth {
                stack.push(p);
            }
        }
        stack.push(i);
    }
    out
}

/// Names a message may use for this agent: its address, plus its label when different.
pub fn agent_keys(agent: &AgentView) -> Vec<String> {
    let mut keys = vec![agent.address.clone()];
    if !agent.label.is_empty() && agent.label != agent.address {
        keys.push(agent.label.clone());
    }
    keys
}

/// One edge per (parent, child) pair that exchanged at least one message; newest message wins.
pub fn edges(agents: &[AgentView], pair_latest: &[MessageRow]) -> Vec<Edge> {
    let _ = (agents, pair_latest);
    todo!("Task 1")
}

/// Message under `LIVE_MS` old; a timestamp in the future (clock skew) counts as live.
pub fn is_live(edge: &Edge, now_ms: i64) -> bool {
    now_ms.saturating_sub(edge.ts_ms) < LIVE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn parents_follow_depth_preorder() {
        let agents: Vec<AgentView> = [0, 1, 2, 1, 0, 1]
            .iter()
            .enumerate()
            .map(|(i, &d)| agent(&format!("t{i}"), &format!("t{i}"), d))
            .collect();
        assert_eq!(
            parents(&agents),
            vec![None, Some(0), Some(1), Some(0), None, Some(4)]
        );
    }

    #[test]
    fn agent_keys_dedupes_label() {
        assert_eq!(agent_keys(&agent("t1", "t1", 0)), vec!["t1".to_string()]);
        assert_eq!(
            agent_keys(&agent("t1", "ceo", 0)),
            vec!["t1".to_string(), "ceo".to_string()]
        );
    }

    #[test]
    fn is_live_boundary() {
        let e = |ts_ms| Edge {
            parent: 0,
            child: 1,
            kind: "ASSIGN".into(),
            ts_ms,
        };
        assert!(is_live(&e(1_000), 1_000 + 299_999));
        assert!(!is_live(&e(1_000), 1_000 + 300_000));
        assert!(is_live(&e(5_000), 1_000));
    }
}
