# TUI `cd` into a project — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **This plan runs in parallel.** Task 0 lands first on branch `feat/tui-project`. Tasks 1–4 then run at the same time, each in its own worktree branched from Task 0's commit, each touching only its own files. Task 5 merges them and wires the TUI. Task 6 is the review. **Implement only the task you were assigned.** Do not edit files owned by another task. If you need a signature changed, stop and report.

**Goal:** `cd <project>` in `agm tui` shows one project's agent tree (status, what each agent is doing, context, token sparkline, branch stage), its inbox in full and its recent messages; answering commands work unchanged.

**Architecture:** New pure module `src/project.rs` (project identity, membership, tree, `cd` matching, data types), new read-only store queries in `src/store/project.rs`, new renderer `src/project_render.rs` reusing the ANSI helpers in `src/dashboard.rs`, two new commands in `parse_command`, and scope state in the `src/tui.rs` loop. The global view, `Snapshot`, `TermView` and existing queries do not change.

**Tech Stack:** Rust 2024, rusqlite, unicode-width, raw ANSI (no TUI framework).

**Spec:** `docs/superpowers/specs/2026-11-23-tui-project-scope-design.md`

## Global Constraints

- No new crate dependencies.
- Unknown values render as `-`, never `0` or a guessed state.
- Every untrusted string (turn summary, message text, inbox text, BRANCHES.md value, terminal name/cwd) passes `sanitize_one_line` before it is painted.
- No rendered line wider than the pane: every line goes through `bounded_styled_line(line, width - 1)`.
- Wrap caps: inbox item ≤ 4 lines, message ≤ 3 lines; longer → last line ends with `…`.
- Scoped messages: newest **20**. Sparkline: last **60 min**, **12** buckets of **5 min**; tokens per bucket = `input_uncached + cache_write + output + reasoning` (no `cache_read`).
- Frame is printed in full and scrolls (no height fitting).
- Code comments may be Vietnamese or English (match the file); user-visible TUI text is English.
- Gates for every task: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` all green. Commit after every green step (free models get capped mid-task; the next worker resumes from the last commit).

## Review Focus

1. A cwd with mixed case / forward slashes / trailing `\` and its worktree must land in the same project as the main checkout (Windows paths): pinned in Task 1 (`roots_are_normalized_and_worktrees_join_main`).
2. Messages and inbox rows that name a terminal by **name** instead of address must still be counted in the project: pinned in Task 1 (`membership_matches_address_or_name`) and Task 2 (`messages_involving_matches_any_listed_party`).
3. A `parent` cycle or a parent pointing outside the project must not hang or drop agents: pinned in Task 1 (`tree_survives_cycles_and_foreign_parents`).
4. Escape sequences inside a turn summary or message (`\x1b]0;pwn\x07`, `\x1b[2J`) must render inert: pinned in Task 3 (`untrusted_text_is_inert`).
5. Very narrow panes (40 cols) and wide CJK/emoji text must not overflow: pinned in Task 3 (`no_line_exceeds_width_at_40_60_110`).

---

### Task 0: Skeleton and shared types (orchestrator — lands before the parallel tasks)

**Files:**
- Create: `src/project.rs` (types + stub functions with `todo!()` bodies), `src/project_render.rs` (stub), `src/store/project.rs` (stub `impl Store`)
- Modify: `src/lib.rs` (`pub mod project; pub mod project_render;`), `src/store/mod.rs` (`mod project;`), `src/dashboard.rs` (make `paint`, `section`, `msg_kind_badge`, `inbox_source_badge`, `sanitize_one_line`, `status_color`, `render_header`, `render_footer`, the `SPARK` const and the ANSI color consts (`RESET`, `GRAY`, `WHITE`, `CYAN`, `BRIGHT_GREEN`, `BRIGHT_RED`, `BRIGHT_YELLOW`, …) `pub(crate)`; no behavior change)

**Interfaces (produced; all later tasks consume exactly these):**

```rust
// src/project.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectKey { pub root: String, pub name: String }          // root = norm_cwd(repo root)
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary { pub key: ProjectKey, pub live_agents: usize, pub open_inbox: usize }
#[derive(Debug, Clone, PartialEq)]
pub enum CdMatch { Found(ProjectKey), Ambiguous(Vec<String>), NotFound }
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRow { pub ts_ms: i64, pub role: String, pub summary: String }
#[derive(Debug, Clone, PartialEq)]
pub struct AgentView {
    pub address: String, pub label: String,          // label = name, else address
    pub app: String, pub model: String, pub effort: String,   // "" when unknown
    pub status: String, pub state: String,           // herdr status ("-" unknown); "live" | "exited"
    pub depth: usize, pub last_sibling: bool,
    pub doing: Option<TurnRow>, pub last_msg: Option<(String, bool)>,
    pub context_pct: Option<f64>, pub tokens: Option<u64>,
    pub spark: Option<Vec<u64>>,                     // 12 buckets, oldest first; None = no linked session
    pub stage: Option<String>,                       // BRANCHES.md status
}
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectView { pub key: ProjectKey, pub agents: Vec<AgentView>,
    pub inbox: Vec<crate::store::InboxRow>, pub messages: Vec<crate::store::MessageRow> }

pub struct RootCache { /* HashMap<String, String> */ }
impl RootCache {
    pub fn new() -> Self;
    /// norm_cwd(cwd) → norm_cwd(root); calls `resolve` at most once per normalized cwd (failures cached too).
    pub fn root_of(&mut self, cwd: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String;
}
pub fn git_common_root(cwd: &str) -> Option<String>;  // git -C cwd rev-parse --path-format=absolute --git-common-dir → parent dir
pub fn project_name(root: &str) -> String;            // last path component, original case not required
/// address → root for every terminal with a cwd.
pub type Roots = std::collections::HashMap<String, String>;
pub fn list_projects(terms: &[crate::store::TerminalRow], inbox: &[crate::store::InboxRow], roots: &Roots)
    -> (Vec<ProjectSummary>, usize);                  // sorted by name; usize = open inbox items with no project
pub fn match_project(list: &[ProjectSummary], query: &str) -> CdMatch;
pub fn members<'a>(terms: &'a [crate::store::TerminalRow], roots: &Roots, key: &ProjectKey)
    -> Vec<&'a crate::store::TerminalRow>;
/// Addresses and names of `members`, for message/inbox filtering.
pub fn parties(members: &[&crate::store::TerminalRow]) -> Vec<String>;
/// (index into members, depth, last_sibling) in display order.
pub fn tree_order(members: &[&crate::store::TerminalRow]) -> Vec<(usize, usize, bool)>;
/// `<lane>--<slug>` (cwd last component, else task) → `<lane>/<slug>` if branches::validate_entry accepts it.
pub fn branch_entry_for(cwd: Option<&str>, task: Option<&str>) -> Option<String>;

// src/store/project.rs  (impl crate::store::Store)
pub fn messages_involving(&self, parties: &[String], limit: usize) -> Result<Vec<MessageRow>>;   // newest first
pub fn latest_turns(&self, sessions: &[i64]) -> Result<HashMap<i64, crate::project::TurnRow>>;  // role in ('assistant','tool')
pub fn usage_buckets(&self, sessions: &[i64], since_ms: i64, bucket_ms: i64, n: usize)
    -> Result<HashMap<i64, Vec<u64>>>;               // every requested session present, n entries, oldest first

// src/project_render.rs
pub fn render_project_body(pv: &ProjectView, width: usize, now_ms: i64) -> String;   // AGENTS + INBOX + MESSAGES
pub fn render_project_list(list: &[ProjectSummary], no_project: usize, width: usize) -> String;
pub fn render_inbox_item(item: &crate::store::InboxRow, width: usize) -> String;     // full text, wrapped, no cap

// src/dashboard.rs (Task 4)
TuiCmd::Cd(Option<String>)   // None = list; Some("..") / Some("/") = leave
TuiCmd::Show(i64)
```

- [ ] **Step 1:** Create the files with the types above and `todo!()` bodies; register modules; widen the listed helpers to `pub(crate)`.
- [ ] **Step 2:** Run `cargo build && cargo clippy --all-targets -- -D warnings` (allow `dead_code` only inside the three stub files via a module-level `#![allow(dead_code)]` that Task 5 removes). Expected: clean; `cargo test` still 408 passed.
- [ ] **Step 3:** Commit on `feat/tui-project`: `chore(tui): skeleton for project scope`.

---

### Task 1: Project identity, membership, tree, `cd` matching — worker A

**Files:** Modify `src/project.rs` (bodies only; no signature changes). Test: `tests/project_test.rs`.

**Interfaces:** Consumes `TerminalRow`, `InboxRow` (`src/store/v2.rs`), `correlate::norm_cwd`, `branches::validate_entry`. Produces the bodies of every `src/project.rs` function in Task 0.

- [ ] **Step 1: Write the failing tests** (build `TerminalRow { address, name, cwd, parent, state, created_ms, ..Default::default() }`):
  - `roots_are_normalized_and_worktrees_join_main`: resolver maps `D:/Project/shop.wt/a--b` and `d:\project\shop\` both to `D:\Project\shop`; `root_of` returns the same string for both; a counting resolver is called once per normalized cwd, also when it returns `None` (then root = `norm_cwd(cwd)`).
  - `list_projects_counts_live_agents_and_open_inbox`: two projects + one exited terminal (not counted live) + inbox rows for an address, a name, an unknown terminal and `None` → unknown + `None` counted in the returned `usize` (= 2); result sorted by name.
  - `membership_matches_address_or_name`: `parties` contains both address and name; inbox row whose `terminal` is the name counts in `list_projects`.
  - `match_project_rules`: exact name case-insensitive wins over prefix (`shop` vs `shop2`); unique prefix → `Found`; two prefixes → `Ambiguous(["shop2","shopx"])`; exact root path → `Found`; duplicate names → `Ambiguous` entries formatted `name (root)`; nothing → `NotFound`.
  - `tree_survives_cycles_and_foreign_parents`: t1→t2→t1 cycle and t3 with parent outside members: all three appear exactly once; roots ordered by `created_ms`; depth/last_sibling correct for a 1-root-2-children tree.
  - `branch_entry_from_worktree_dir_or_task`: `D:\x\.wt\tap-hoa--low-stock` → `tap-hoa/low-stock`; cwd `D:\x` + task `a--b` → `a/b`; `Bad--Case` → `None`; no `--` → `None`.
  - `git_common_root_on_real_repo_and_worktree` (uses `git init` + `git worktree add` in a tempdir like `tests/git_test.rs`): both return the main repo dir (compare via `norm_cwd`); non-repo dir → `None`.
- [ ] **Step 2:** `cargo test --test project_test` → FAIL (`not yet implemented`).
- [ ] **Step 3:** Implement the bodies. `tree_order`: parent resolves by address or name within members only; walk from roots with a visited set; anything unvisited after that (cycles) becomes a root in `created_ms` order.
- [ ] **Step 4:** `cargo test --test project_test` → PASS; full gates green.
- [ ] **Step 5:** Commit `feat(project): project identity, membership, tree and cd matching`.

---

### Task 2: Scoped store queries — worker B

**Files:** Modify `src/store/project.rs`. Test: `tests/project_store_test.rs`.

**Interfaces:** Consumes `Store` internals (`self.conn`), tables `messages`, `turns`, `usage`; `TurnRow` from Task 0. Produces the three query bodies.

- [ ] **Step 1: Write the failing tests** (seed with `Store::open_in_memory()`, `add_message(&MessageRow{..Default::default()})`, `session_id_for_path(Source::Codex, "p1")` for session ids, `insert_turn(sid, &TurnSummary{..})`, `upsert_usage(sid, &UsageRecord{..})`):
  - `messages_involving_matches_any_listed_party`: 30 messages among `t1`, `t2`, `ceo-name`, `x9`; parties `["t1","ceo-name"]`, limit 20 → 20 rows, newest first, none only between `x9` and `t2`; empty parties → empty vec (no SQL error).
  - `latest_turns_picks_newest_assistant_or_tool`: session 1 has user (newest), assistant, tool rows → returns the newest of assistant/tool; session without turns absent from map; equal `ts_ms` → higher `byte_offset` wins.
  - `usage_buckets_boundaries`: `since=0, bucket=300_000, n=12`; rows at 0, 299_999, 300_000, 3_599_999, 3_600_000 (outside) → bucket0 = two rows, bucket1 = one, bucket11 = one, out-of-window ignored; `cache_read` excluded; session with no usage → 12 zeros.
- [ ] **Step 2:** `cargo test --test project_store_test` → FAIL.
- [ ] **Step 3:** Implement with bound parameters only (build `IN (?, ?, …)` placeholders from the slice length; never format values into SQL). `usage_buckets` groups `(ts_ms - since_ms) / bucket_ms` in SQL and fills a zeroed `Vec` per session.
- [ ] **Step 4:** Test PASS; full gates green.
- [ ] **Step 5:** Commit `feat(store): scoped messages, latest turns, usage buckets`.

---

### Task 3: Project renderer — worker C

**Files:** Modify `src/project_render.rs`. Test: `tests/project_render_test.rs` (copy the `strip_ansi` helper from `tests/dashboard_test.rs`).

**Interfaces:** Consumes `ProjectView`, `AgentView`, `ProjectSummary`, `InboxRow`, `MessageRow`; `pub(crate)` helpers from `src/dashboard.rs` (`paint`, `section`, `msg_kind_badge`, `inbox_source_badge`, `sanitize_one_line`, `status_color`, `SPARK`, `one_line`, `bounded_styled_line`, `format_age_s`). Produces the three render functions.

Layout per agent (two lines; see spec mock): line 1 = tree prefix (`├ ` / `└ ` / `│ ` by depth+last_sibling) + label + app + model[/effort] + status symbol+word + `ctx` bar (8 cells, `-` when None) + sparkline (12 glyphs scaled to the max bucket; all-zero → `▁`×12; None → `-`) + total tokens (`1.2M`/`380k`, `-` when None) + `stage: X` when Some. Line 2 = `doing: <summary> · <age> ago`, or `doing: -` plus `· last msg: KIND sent|received` when `last_msg` is Some. Status symbol: `●` working/running, `◐` BLOCKED*, `○` idle, `✓` done, `×` exited, `?` other — always followed by the word. Narrow pane (< 70): drop sparkline, then ctx bar.

- [ ] **Step 1: Write the failing tests** using hand-built `ProjectView` fixtures:
  - `agent_lines_show_tree_status_doing_and_unknowns`: 1 root + 2 children; child without session shows `ctx -`, `-` tokens, `doing: -  · last msg: REPORT sent`; root shows `doing: [tool] Bash · 12s ago` for `now_ms - ts = 12_000`; status words present next to symbols.
  - `sparkline_scales_to_max_and_zero_is_flat`: buckets `[0,…,0,10]` → last glyph `█`, others `▁`; all zero → `▁`×12.
  - `inbox_wraps_to_four_lines_messages_to_three`: 1000-char texts → inbox block has exactly 4 text lines, last ends with `…`; message exactly 3; message shows age `5m` for 300 s.
  - `untrusted_text_is_inert`: summary/message/inbox containing `\x1b]0;pwn\x07`, `\x1b[2J`, `\r\n` → output (before strip) contains no `\x1b]`, no `\x1b[2J`, no raw `\r`.
  - `no_line_exceeds_width_at_40_60_110`: fixture with CJK/emoji labels and long texts; every line of `strip_ansi(out)` has `UnicodeWidthStr::width() <= width - 1` for 40, 60, 110; at 60 no sparkline glyph row.
  - `project_list_and_show`: `render_project_list` lists names with `N live · M inbox` and a `(no project)` line when `no_project > 0`; `render_inbox_item` wraps a 1000-char text fully (no `…`).
- [ ] **Step 2:** `cargo test --test project_render_test` → FAIL.
- [ ] **Step 3:** Implement. Section titles: `AGENTS · <name> (<n>)`, `INBOX · <name> (<n>)`, `MESSAGES · <name>`; empty → `(empty)` / `(no messages)` in gray like the global view.
- [ ] **Step 4:** Test PASS; full gates green.
- [ ] **Step 5:** Commit `feat(tui): project view renderer`.

---

### Task 4: `cd` and `show` commands — worker D

**Files:** Modify `src/dashboard.rs` (only `TuiCmd`, `TUI_COMMANDS`, `parse_command`). Test: add to `tests/dashboard_test.rs`.

**Interfaces:** Produces `TuiCmd::Cd(Option<String>)`, `TuiCmd::Show(i64)`; `TUI_COMMANDS` grows to 11 entries with `("cd", "", "[project|..]")` and `("show", "", "<#>")` placed after `view`. The empty short alias must render without a stray separator in the footer.

- [ ] **Step 1: Write the failing tests:** `parse_cd_and_show`: `cd` → `Cd(None)`; `cd  shop ` → `Cd(Some("shop"))`; `cd ..` → `Cd(Some(".."))`; `show 43` → `Show(43)`; `show #43` → `Show(43)`; `show`, `show x` → `Err` mentioning `show <#>`. `footer_lists_cd_and_show`: `render_view` footer text contains `cd [project|..]` and `show <#>`; existing footer tests still pass.
- [ ] **Step 2:** `cargo test --test dashboard_test` → FAIL.
- [ ] **Step 3:** Implement in `parse_command`; adjust footer rendering only if the empty alias breaks it.
- [ ] **Step 4:** Test PASS; full gates green.
- [ ] **Step 5:** Commit `feat(tui): cd and show commands`.

---

### Task 5: Integration — loader and TUI wiring (after Tasks 1–4 merge into `feat/tui-project`)

**Files:** Modify `src/project.rs` (add `load_project_view`), `src/tui.rs`, `src/dashboard.rs` (`render_scoped`), `README.md`, `README_vi.md` (TUI section), `CHANGELOG.md` (`[Unreleased]` → Thêm mới). Remove the `#![allow(dead_code)]` lines. Test: `tests/project_test.rs` (loader), `src/tui.rs` unit tests.

**Interfaces:**
```rust
// src/project.rs
pub fn load_project_view(store: &Store, snap: &crate::dashboard::Snapshot, key: &ProjectKey,
    roots: &Roots, now_ms: i64) -> anyhow::Result<ProjectView>;
pub fn compute_roots(terms: &[TerminalRow], snap: &crate::dashboard::Snapshot, cache: &mut RootCache,
    resolve: &dyn Fn(&str) -> Option<String>) -> Roots;   // cwd from TerminalRow, else live TermView.cwd
// src/dashboard.rs
pub fn render_scoped(s: &Snapshot, body: &str, width: usize) -> String;   // header + body + footer, bounded
```

- [ ] **Step 1: Write the failing tests:**
  - `load_project_view_joins_snapshot_and_store`: in-memory store with 3 terminals in one project (one exited with an open inbox item, one exited without → excluded), linked sessions with turns and usage, a `BRANCHES.md` in a tempdir root with entry `a/b` `status: wip` and a terminal cwd `<root>\.wt\a--b` → `AgentView.stage == Some("wip")`, `doing`, `spark` (12), `model/context_pct/tokens` copied from the snapshot's `TermView` for live agents and `None` for exited, inbox filtered to the project, messages ≤ 20.
  - `tui` unit test `prompt_shows_scope`: `render_frame` prompt is `agm[shop]> ` when scoped, `agm> ` otherwise (add a `scope: Option<&str>` parameter).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement. TUI loop keeps `scope: Option<ProjectKey>`, `roots_cache: RootCache`, `shown: Option<InboxRow>`. Each frame: build `Snapshot` as today; if scoped → `terminals(false)`, `compute_roots`, `load_project_view`, `render_project_body`, `render_scoped`; `show` block (from `render_inbox_item`) is appended above the note until the next command. `Cd(None)` → note-block with `render_project_list`; `Cd(Some(".."|"/"))` → leave; `Cd(Some(q))` → `match_project` → enter / note listing candidates / `no project matching 'q'`. A project that disappears (no members) keeps the scope and renders empty sections. BRANCHES.md read with `branches::get` per agent entry, errors → `stage: None`.
- [ ] **Step 4:** Full gates green; manual check: `cargo run -- tui` with `COLUMNS=60` and `110`, `cd`, `cd <name>`, `show <#>`, `reply`, `cd ..`.
- [ ] **Step 5:** Commit `feat(tui): cd into a project (agents, inbox, messages)`.

---

### Task 6: Independent review — agy

Reviewer: `agy` (Codex quota exhausted), fresh session, different family from the workers. Reads spec + plan + `git diff main...feat/tui-project` + worker logs; writes `.agents/reviews/tui-project-scope-1.md` with `Đối tượng: HEAD <sha>`, verdict `ĐẠT | CHƯA ĐẠT`, blockers as `file:line` + consequence + minimal fix; checks the Review Focus list explicitly; runs the gates. Up to 2 rounds of `CHƯA ĐẠT` → back to the owning worker; then escalate to the orchestrator.

---

## Execution (orchestrator notes)

| Task | Runs | Model (pinned at launch) | Worktree / branch |
|---|---|---|---|
| 0 | first, orchestrator | Claude (small) | `feat/tui-project` in main checkout |
| 1 | parallel | `opencode/deepseek-v4.1-flash-free` | `D:/Project/agmag.wt/tui-project-a` / `feat/tui-project-a` |
| 2 | parallel | `opencode/qwen4-max-free` | `…/tui-project-b` / `feat/tui-project-b` |
| 3 | parallel | `opencode-go/deepseek-v4.1-flash` (hardest task; cheap paid) | `…/tui-project-c` / `feat/tui-project-c` |
| 4 | parallel | `opencode/mimo-v2.6-flash-free` | `…/tui-project-d` / `feat/tui-project-d` |
| 5 | after merge | `opencode-go/deepseek-v4.1-flash` | `feat/tui-project` |
| 6 | after 5 | `agy` (strongest available model, read-only review) | reads `feat/tui-project` |

- Probe each free model first (`opencode run --model <m> "Reply with exactly: OK"`); if one is capped, switch to another free model, not tier.
- Each worker prompt: "Read `docs/superpowers/plans/2026-11-23-tui-project-scope.md` and the spec. Implement **Task N only**, TDD, commit after each green step, log progress to `.agents/log/tui-project-<x>.md`. Do not touch files owned by other tasks."
- Merge order into `feat/tui-project`: 2 → 1 → 4 → 3 (fewest cross-file risks first); each merge followed by the full gates.
