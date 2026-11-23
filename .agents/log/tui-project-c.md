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
- `cargo test --test project_render_test` -> 0 passed, 6 failed, all `not yet implemented: Task 3`.
- Commit 3689c93 `test(tui): failing project renderer tests`.

### Step 2-3: implementation (GREEN)
- src/project_render.rs: AGENTS/INBOX/MESSAGES body, `cd` project list, full inbox item.
  - Agent line 1: tree prefix (ancestor carry stack) + label + app + model/effort + status symbol/word
    + ctx 8-cell bar (`ctx -` unknown) + 12-glyph spark (`▁`×12 all-zero, `-` unknown) + token total
    (`1.2M`/`380k`, `-` unknown) + `stage:` when Some.
  - Agent line 2: `doing: <summary> · <12s|N m> ago`, or `doing: -  · last msg: KIND sent|received`.
  - Narrow panes: spark dropped < 70 cols, ctx dropped < 50 cols; every line through
    `bounded_styled_line(line, width - 1)`.
  - Untrusted text: all summaries/messages/inbox/labels/stages via `sanitize_one_line`; OSC/CSI/CR inert.
  - Wrap caps: inbox item 4 lines, message 3 lines, last line ends with `…`; `show` item uncapped.
- `cargo test --test project_render_test` -> 6 passed.
- Gates: `cargo test` -> 414 passed (408 baseline + 6 new), 0 failed; `cargo fmt --check` clean;
  `cargo clippy --all-targets -- -D warnings` clean.

### Next
- Task 3 is complete. No signature/type changes; only src/project_render.rs and tests/project_render_test.rs.

