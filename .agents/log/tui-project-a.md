# tui-project-a log — Task 1 (project identity, membership, tree, `cd` matching)

Branch: feat/tui-project-a, base 5e93e85 (Task 0 skeleton).
Scope: implement bodies in `src/project.rs` only; tests in `tests/project_test.rs`.

## Steps
- [x] 1. Wrote 7 failing tests in `tests/project_test.rs`
  (`roots_are_normalized_and_worktrees_join_main`,
  `list_projects_counts_live_agents_and_open_inbox`,
  `membership_matches_address_or_name`, `match_project_rules`,
  `tree_survives_cycles_and_foreign_parents`,
  `branch_entry_from_worktree_dir_or_task`,
  `git_common_root_on_real_repo_and_worktree`).
- [x] 2. `cargo test --test project_test` → FAILED, 0 passed / 7 failed, all
  `not yet implemented: Task 1` (todo!() bodies).
- [x] 3. Implemented the bodies in `src/project.rs` (no signature changes):
  `RootCache::root_of` (normalize + cache hits and failures), `git_common_root`
  (`git rev-parse --path-format=absolute --git-common-dir` → parent),
  `project_name`, `list_projects` (live count, open inbox by address or name,
  `no_project` count, sorted by name), `match_project` (root → exact name
  case-insensitive → unique prefix; ambiguous `name` or `name (root)` when the
  name is shared), `members`, `parties` (deduped address + name), `tree_order`
  (parents by address/name, roots by `created_ms`, visited set, cycles broken at
  the oldest unvisited node), `branch_entry_for` (cwd last component, then task;
  `branches::validate_entry`).
- [x] 4. Green gates: `cargo test --test project_test` → 7 passed;
  `cargo test` → 415 passed, 0 failed (was 408 + 7 new);
  `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean
  (fixed `filter(..).next_back()` → `rfind`, collapsed `if`s with let-chains,
  arrays instead of `vec!` in the tests).
- [x] 5. Commit `feat(project): project identity, membership, tree and cd matching`.

Task 1 is complete. Next worker: Task 5 consumes these functions; do not change
signatures. Note: `tree_order` emits real roots first (by `created_ms`), then
parent-cycle leftovers (by `created_ms`), breaking each cycle at its oldest node.

## Notes for Task 5
- `members` returns every terminal whose `roots` entry equals the project root
  (live and exited); the live/exited filtering for AGENTS belongs to
  `load_project_view` (Task 5 test `load_project_view_joins_snapshot_and_store`).
- `list_projects` counts an open inbox item for a project only when its
  `terminal` matches a terminal that has a root; otherwise it is `no_project`.
