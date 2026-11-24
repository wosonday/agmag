# T3 — overview mouse + drag-resize (round 2)

Branch: `feat/tui-mouse-overview` (worktree `D:/Project/agmag.wt/tui-mouse-overview`).
Brief: `.agents/tasks/tui-mouse-overview.md` (round 2 rules included).
Spec: `docs/superpowers/specs/2026-11-24-tui-layout-mouse-design.md`.

## Baseline

- `cargo test` before any change: **745 passed, 0 failed** (41 test binaries + 0 doctests).
  Counts per binary are recorded in the final gate entry.
- EOL: `src/board/*` and `Cargo.toml` LF; `CHANGELOG.md`, `src/main.rs`, `src/tui.rs` CRLF.
  README files LF. Verified with `git ls-files --eol` before every commit.

## Plan (one commit per step)

1. `GlobalState::on_mouse` (pure): focus, row selection, wheel, double-click, popup/output guards.
2. `LayoutOverride` (fractions, `to_string`/`parse`) applied to `section_layout`/`draw_sections`.
3. Drag-resize + `GEffect::SaveLayout`, border double-click reset, `layout reset`, `tui_layout` load/save.
4. Wire `handle_mouse` for `Screen::Global` + app-level tests + anti-goal matrix.
5. Docs: overview `?` help (Output panel), README/README_vi, CHANGELOG.

## Step 1 — mouse on the overview (pure)

New tests (written first, seen failing because `on_mouse` does not exist):

- `mouse_click_focuses_panel_rows_and_cmdline`
- `mouse_wheel_moves_selection_and_scrolls`
- `mouse_double_click_opens_board_and_prefills_reply`
- `mouse_double_click_window_edges`
- `mouse_ignored_while_popup_or_output_overlay`
- `mouse_ignores_non_left_and_moved_events`

Red: the module failed to compile with "no method named `on_mouse`" and no `HitPanel::CmdLine`
variant. Green: all 6 pass; `cargo test` = **751 passed, 0 failed** (lib 115, was 109).

Supporting changes in this step: `HitPanel::CmdLine` (the command bar is a hit target recorded by
`draw_global`) and two existing integration tests updated for it — `tests/global_view_test.rs` now
skips the command line in the "every rect inside main" check (it is below main by definition) and
its exhaustive `HitPanel` match lists `CmdLine` with `Output` as "not a section".

