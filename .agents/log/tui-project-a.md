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
- [ ] 3. Implement the bodies.
- [ ] 4. Green: `cargo test --test project_test`, then full gates.
- [ ] 5. Commit `feat(project): project identity, membership, tree and cd matching`.

Next step: implement `RootCache::root_of`, `git_common_root`, `project_name`,
`list_projects`, `match_project`, `members`, `parties`, `tree_order`,
`branch_entry_for`.
