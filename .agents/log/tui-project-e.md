# tui-project-e log (Task 5, loader part only)

Branch: `feat/tui-project-e` · worktree `D:/Project/agmag.wt/tui-project-e`
Scope: only `src/project.rs` (add `compute_roots` + `load_project_view`), `tests/project_test.rs`, this log.
TUI wiring (`src/tui.rs`, `render_scoped`, README/CHANGELOG, allow(dead_code) removal) belongs to the other worker.

## 2026-11-23 · step 1 (red)
- Read plan (Task 5 interfaces + Global Constraints) and design spec; Tasks 0–3 already merged here
  (`src/project.rs`, `src/store/project.rs`, `src/project_render.rs` implemented).
- Confirmed consumed APIs: `Store::terminals(false)`, `inbox(true)`, `messages_involving`, `latest_turns`,
  `usage_buckets`, `branches::get` (missing file → Ok(None)), `branch_entry_for`, `members`, `parties`,
  `tree_order`, `RootCache::root_of`; `dashboard::snapshot(&store, &Config::builtin(), now)` (Config has no
  Default; dashboard_test uses builtin). `upsert_terminal` ignores `status` → use `set_terminal_status`.
- Wrote `load_project_view_joins_snapshot_and_store` + `compute_roots_falls_back_to_live_termview_cwd`
  in `tests/project_test.rs` (in-memory store, tempdir BRANCHES.md `### a/b` / `- status: wip`,
  worktree cwd `<root>\.wt\a--b`, fake resolver, 2 live + exited-with-inbox + exited-without terminals).
- `cargo test --test project_test` → RED: E0432, no `compute_roots` / `load_project_view` in `project`.

## 2026-11-23 · step 2–4 (green)
- Implemented both functions at the end of `src/project.rs` (no existing signature touched, no new imports
  needed — `crate::dashboard::Snapshot` / `crate::store::Store` / `anyhow::Result` fully qualified):
  - `compute_roots`: row cwd (non-empty) else the live `TermView.cwd` by address (non-empty), else skip;
    root via `RootCache::root_of` (resolver called once per normalized cwd, failures cached).
  - `load_project_view`: `terminals(false)` → `members` → keep live always, exited only with an open inbox
    item matched by address or name; `tree_order` for depth/last_sibling; label = name else address,
    app/status/state from the row (status `-` when unknown); model/effort/context_pct/tokens/last_msg copied
    from the snapshot `TermView` (live only, exited → ""/None); doing = `latest_turns`, spark =
    `usage_buckets(sessions, now-3_600_000, 300_000, 12)` (None without a session); stage = `status` field of
    `branches::get(<root>/BRANCHES.md, branch_entry_for(cwd, task))`, any error/missing → None;
    inbox = open items whose terminal is a party; messages = `messages_involving(parties, 20)`.
- `cargo test --test project_test` → GREEN: 9 passed (7 existing + 2 new).
- Gates: `cargo test` → 426 passed / 0 failed; `cargo fmt --check` clean (after `cargo fmt` on the two
  touched files); `cargo clippy --all-targets -- -D warnings` clean.
- Files touched: `src/project.rs`, `tests/project_test.rs`, this log. No deps added, no existing test
  weakened. `#![allow(dead_code, unused_variables)]` left in place — its removal belongs to the TUI-wiring
  half of Task 5 (the other worker), same for `src/tui.rs` / `render_scoped` / README / CHANGELOG.
- Loader part of Task 5 complete. Commit: `feat(project): project view loader (compute_roots, load_project_view)`.
