//! Project scope for `agm tui`: which project a terminal belongs to, who is in it, and the
//! data the project view renders. Pure functions except `git_common_root`.
// Skeleton (Task 0): bodies are filled by Task 1; Task 5 removes this allow.
#![allow(dead_code, unused_variables)]

use std::collections::HashMap;
use std::process::Command;

use crate::branches;
use crate::correlate::norm_cwd;
use crate::store::{InboxRow, MessageRow, TerminalRow};

/// A project = repository root (worktrees join their main checkout).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectKey {
    /// `correlate::norm_cwd` of the repository root.
    pub root: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary {
    pub key: ProjectKey,
    pub live_agents: usize,
    pub open_inbox: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CdMatch {
    Found(ProjectKey),
    Ambiguous(Vec<String>),
    NotFound,
}

/// Latest `turns` row of a session (role `assistant` or `tool`).
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRow {
    pub ts_ms: i64,
    pub role: String,
    pub summary: String,
}

/// One agent line pair in the project view. Unknown values are `None` / `""`, never invented.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentView {
    pub address: String,
    /// Terminal name, else address.
    pub label: String,
    pub app: String,
    pub model: String,
    pub effort: String,
    /// herdr status (`-` when unknown).
    pub status: String,
    /// `live` | `exited`.
    pub state: String,
    pub depth: usize,
    pub last_sibling: bool,
    pub doing: Option<TurnRow>,
    /// (English kind, sent_by_this_terminal).
    pub last_msg: Option<(String, bool)>,
    pub context_pct: Option<f64>,
    pub tokens: Option<u64>,
    /// 12 buckets of 5 minutes, oldest first; None = no linked session.
    pub spark: Option<Vec<u64>>,
    /// `status` of the matching BRANCHES.md entry.
    pub stage: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectView {
    pub key: ProjectKey,
    pub agents: Vec<AgentView>,
    pub inbox: Vec<InboxRow>,
    pub messages: Vec<MessageRow>,
}

/// Terminal address → normalized project root.
pub type Roots = HashMap<String, String>;

/// cwd → root, resolved at most once per normalized cwd (failures cached too).
#[derive(Debug, Default)]
pub struct RootCache {
    map: HashMap<String, String>,
}

impl RootCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// `norm_cwd(cwd)` → `norm_cwd(root)`; when `resolve` returns None the root is `norm_cwd(cwd)`.
    pub fn root_of(&mut self, cwd: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
        let norm = norm_cwd(cwd);
        if let Some(root) = self.map.get(&norm) {
            return root.clone();
        }
        let root = resolve(&norm)
            .map(|r| norm_cwd(&r))
            .unwrap_or_else(|| norm.clone());
        self.map.insert(norm, root.clone());
        root
    }
}

/// `git -C cwd rev-parse --path-format=absolute --git-common-dir` → its parent directory.
pub fn git_common_root(cwd: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let common = String::from_utf8_lossy(&out.stdout);
    let common = common.trim();
    if common.is_empty() {
        return None;
    }
    std::path::Path::new(common)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Last path component of a root.
pub fn project_name(root: &str) -> String {
    root.split(['\\', '/'])
        .rfind(|part| !part.is_empty())
        .unwrap_or(root)
        .to_string()
}

/// Projects sorted by name, plus the number of open inbox items that belong to no project.
pub fn list_projects(
    terms: &[TerminalRow],
    inbox: &[InboxRow],
    roots: &Roots,
) -> (Vec<ProjectSummary>, usize) {
    let mut by_address: HashMap<&str, &str> = HashMap::new();
    // name → (root, live, created_ms); a live terminal wins over an exited one, then the
    // newest wins — the same order `Store::terminal` uses for a name lookup.
    let mut by_name: HashMap<&str, (&str, bool, i64)> = HashMap::new();
    let mut projects: HashMap<&str, ProjectSummary> = HashMap::new();

    for t in terms {
        let Some(root) = roots.get(&t.address).map(String::as_str) else {
            continue;
        };
        by_address.insert(t.address.as_str(), root);
        if let Some(name) = t.name.as_deref().filter(|name| !name.is_empty()) {
            let candidate = (root, t.state == "live", t.created_ms);
            let better = by_name.get(name).is_none_or(|&(_, live, created_ms)| {
                (candidate.1, candidate.2) > (live, created_ms)
            });
            if better {
                by_name.insert(name, candidate);
            }
        }
        let entry = projects.entry(root).or_insert_with(|| ProjectSummary {
            key: ProjectKey {
                root: root.to_string(),
                name: project_name(root),
            },
            live_agents: 0,
            open_inbox: 0,
        });
        if t.state == "live" {
            entry.live_agents += 1;
        }
    }

    let mut no_project = 0usize;
    for item in inbox {
        if item.resolved_ms.is_some() {
            continue;
        }
        let root = item.terminal.as_deref().and_then(|t| {
            by_address
                .get(t)
                .copied()
                .or_else(|| by_name.get(t).map(|(root, _, _)| *root))
        });
        match root.and_then(|r| projects.get_mut(r)) {
            Some(p) => p.open_inbox += 1,
            None => no_project += 1,
        }
    }

    let mut list: Vec<ProjectSummary> = projects.into_values().collect();
    list.sort_by(|a, b| {
        a.key
            .name
            .cmp(&b.key.name)
            .then_with(|| a.key.root.cmp(&b.key.root))
    });
    (list, no_project)
}

pub fn match_project(list: &[ProjectSummary], query: &str) -> CdMatch {
    let query = query.trim();
    if query.is_empty() {
        return CdMatch::NotFound;
    }

    // Exact root path first (worktree cwds resolve to their main root).
    let norm = norm_cwd(query);
    if let Some(found) = list.iter().find(|p| p.key.root == norm) {
        return CdMatch::Found(found.key.clone());
    }

    let needle = query.to_lowercase();
    let exact: Vec<&ProjectSummary> = list
        .iter()
        .filter(|p| p.key.name.to_lowercase() == needle)
        .collect();
    if !exact.is_empty() {
        return if exact.len() == 1 {
            CdMatch::Found(exact[0].key.clone())
        } else {
            CdMatch::Ambiguous(ambiguous_entries(&exact, list))
        };
    }

    let prefixed: Vec<&ProjectSummary> = list
        .iter()
        .filter(|p| p.key.name.to_lowercase().starts_with(&needle))
        .collect();
    match prefixed.len() {
        0 => CdMatch::NotFound,
        1 => CdMatch::Found(prefixed[0].key.clone()),
        _ => CdMatch::Ambiguous(ambiguous_entries(&prefixed, list)),
    }
}

/// `name` normally; `name (root)` when the name is shared, so the choice is unambiguous.
fn ambiguous_entries(candidates: &[&ProjectSummary], all: &[ProjectSummary]) -> Vec<String> {
    let mut out: Vec<String> = candidates
        .iter()
        .map(|p| {
            let shared = all
                .iter()
                .filter(|other| other.key.name.to_lowercase() == p.key.name.to_lowercase())
                .count()
                > 1;
            if shared {
                format!("{} ({})", p.key.name, p.key.root)
            } else {
                p.key.name.clone()
            }
        })
        .collect();
    out.sort();
    out
}

pub fn members<'a>(
    terms: &'a [TerminalRow],
    roots: &Roots,
    key: &ProjectKey,
) -> Vec<&'a TerminalRow> {
    terms
        .iter()
        .filter(|t| roots.get(&t.address).is_some_and(|root| root == &key.root))
        .collect()
}

/// Addresses and names of `members`, for message/inbox filtering.
pub fn parties(members: &[&TerminalRow]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in members {
        if !out.contains(&m.address) {
            out.push(m.address.clone());
        }
        if let Some(name) = m.name.as_deref().filter(|name| !name.is_empty())
            && !out.iter().any(|p| p == name)
        {
            out.push(name.to_string());
        }
    }
    out
}

/// (index into members, depth, last_sibling) in display order.
pub fn tree_order(members: &[&TerminalRow]) -> Vec<(usize, usize, bool)> {
    let n = members.len();
    if n == 0 {
        return Vec::new();
    }

    // Parent resolves by address or name, but only within `members`.
    let mut by_key: HashMap<&str, usize> = HashMap::new();
    for (i, m) in members.iter().enumerate() {
        by_key.entry(m.address.as_str()).or_insert(i);
    }
    for (i, m) in members.iter().enumerate() {
        if let Some(name) = m.name.as_deref().filter(|name| !name.is_empty()) {
            by_key.entry(name).or_insert(i);
        }
    }
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (i, m) in members.iter().enumerate() {
        if let Some(p) = m.parent.as_deref().and_then(|p| by_key.get(p).copied())
            && p != i
        {
            parent[i] = Some(p);
        }
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots: Vec<usize> = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) => children[*p].push(i),
            None => roots.push(i),
        }
    }

    let by_created = |a: &usize, b: &usize| {
        members[*a]
            .created_ms
            .cmp(&members[*b].created_ms)
            .then_with(|| members[*a].address.cmp(&members[*b].address))
    };
    roots.sort_by(by_created);
    for kids in &mut children {
        kids.sort_by(by_created);
    }

    fn walk(
        start: usize,
        children: &[Vec<usize>],
        visited: &mut [bool],
        out: &mut Vec<(usize, usize, bool)>,
    ) {
        let mut stack = vec![(start, 0usize, false)];
        while let Some((i, depth, last)) = stack.pop() {
            if visited[i] {
                continue;
            }
            visited[i] = true;
            out.push((i, depth, last));
            let kids = &children[i];
            for (k, &child) in kids.iter().enumerate().rev() {
                if !visited[child] {
                    stack.push((child, depth + 1, k + 1 == kids.len()));
                }
            }
        }
    }

    let mut out: Vec<(usize, usize, bool)> = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    for &root in &roots {
        walk(root, &children, &mut visited, &mut out);
    }
    // Anything still unvisited sits in a parent cycle: break it at the oldest node.
    let mut leftovers: Vec<usize> = (0..n).filter(|&i| !visited[i]).collect();
    leftovers.sort_by(by_created);
    for &root in &leftovers {
        walk(root, &children, &mut visited, &mut out);
    }
    out
}

/// `<lane>--<slug>` (cwd last component, else task) → `<lane>/<slug>` when `branches::validate_entry` accepts it.
pub fn branch_entry_for(cwd: Option<&str>, task: Option<&str>) -> Option<String> {
    let entry_from = |component: &str| -> Option<String> {
        let (lane, slug) = component.split_once("--")?;
        let entry = format!("{lane}/{slug}");
        branches::validate_entry(&entry).ok().map(|()| entry)
    };

    if let Some(cwd) = cwd {
        let last = cwd
            .split(['\\', '/'])
            .rfind(|part| !part.is_empty())
            .unwrap_or("");
        if let Some(entry) = entry_from(last) {
            return Some(entry);
        }
    }
    task.and_then(entry_from)
}

/// address → root for every terminal with a cwd: the row's cwd, else the live `TermView.cwd`
/// (the linked session's cwd) from the snapshot. Empty cwds are skipped.
pub fn compute_roots(
    terms: &[TerminalRow],
    snap: &crate::dashboard::Snapshot,
    cache: &mut RootCache,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Roots {
    let mut out = Roots::new();
    for t in terms {
        let cwd = t.cwd.as_deref().filter(|c| !c.is_empty()).or_else(|| {
            snap.terminals
                .iter()
                .find(|v| v.address == t.address)
                .map(|v| v.cwd.as_str())
                .filter(|c| !c.is_empty())
        });
        if let Some(cwd) = cwd {
            out.insert(t.address.clone(), cache.root_of(cwd, resolve));
        }
    }
    out
}

/// Everything the project view renders: member agents (live always, exited only while they have
/// an open inbox item), the project's open inbox items and its newest 20 messages.
pub fn load_project_view(
    store: &crate::store::Store,
    snap: &crate::dashboard::Snapshot,
    key: &ProjectKey,
    roots: &Roots,
    now_ms: i64,
) -> anyhow::Result<ProjectView> {
    let terms = store.terminals(false)?;
    let open = store.inbox(true)?;
    let kept: Vec<&TerminalRow> = members(&terms, roots, key)
        .into_iter()
        .filter(|t| {
            t.state == "live"
                || open.iter().any(|item| {
                    item.terminal.as_deref().is_some_and(|term| {
                        term == t.address.as_str() || t.name.as_deref() == Some(term)
                    })
                })
        })
        .collect();

    let ps = parties(&kept);
    let sessions: Vec<i64> = kept.iter().filter_map(|t| t.session).collect();
    let turns = store.latest_turns(&sessions)?;
    let buckets = store.usage_buckets(&sessions, now_ms - 3_600_000, 300_000, 12)?;
    let messages = store.messages_involving(&ps, 20)?;
    let inbox: Vec<InboxRow> = open
        .into_iter()
        .filter(|item| {
            item.terminal
                .as_deref()
                .is_some_and(|term| ps.iter().any(|p| p.as_str() == term))
        })
        .collect();

    let branches_file = std::path::Path::new(&key.root).join("BRANCHES.md");
    let mut agents = Vec::with_capacity(kept.len());
    for (i, depth, last_sibling) in tree_order(&kept) {
        let t = kept[i];
        // Snapshot fields exist for live terminals only; exited → ""/None, never guessed.
        let view = if t.state == "live" {
            snap.terminals.iter().find(|v| v.address == t.address)
        } else {
            None
        };
        let stage = branch_entry_for(t.cwd.as_deref(), t.task.as_deref())
            .and_then(|entry| branches::get(&branches_file, &entry).ok().flatten())
            .and_then(|fields| {
                fields
                    .iter()
                    .rfind(|(k, _)| k == "status")
                    .map(|(_, v)| v.clone())
            });
        agents.push(AgentView {
            address: t.address.clone(),
            label: t
                .name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| t.address.clone()),
            app: t.app.clone().unwrap_or_default(),
            model: view.map(|v| v.model.clone()).unwrap_or_default(),
            effort: view.map(|v| v.effort.clone()).unwrap_or_default(),
            status: t.status.clone().unwrap_or_else(|| "-".into()),
            state: t.state.clone(),
            depth,
            last_sibling,
            doing: t.session.and_then(|sid| turns.get(&sid).cloned()),
            last_msg: view.and_then(|v| v.last_msg.clone()),
            context_pct: view.and_then(|v| v.context_pct),
            tokens: view.and_then(|v| v.tokens),
            spark: t.session.and_then(|sid| buckets.get(&sid).cloned()),
            stage,
        });
    }
    Ok(ProjectView {
        key: key.clone(),
        agents,
        inbox,
        messages,
    })
}
