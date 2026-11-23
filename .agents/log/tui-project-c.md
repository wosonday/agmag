# tui-project-c log (Task 3: project renderer)

## 2026-11-23

### State before
- Worktree: D:\Project\agmag.wt\tui-project-c, branch feat/tui-project-c (base 5e93e85 chore(tui): skeleton for project scope).
- Files to touch: src/project_render.rs, tests/project_render_test.rs, this log.
- Task 0 interfaces confirmed: render_project_body / render_project_list / render_inbox_item; dashboard helpers are pub(crate).

### Step 1: failing tests (RED)
- Added tests/project_render_test.rs: agent_lines_show_tree_status_doing_and_unknowns,
  sparkline_scales_to_max_and_zero_is_flat, inbox_wraps_to_four_lines_messages_to_three,
  untrusted_text_is_inert, no_line_exceeds_width_at_40_60_110, project_list_and_show.
- Next: run `cargo test --test project_render_test`, expect todo!() failures.
