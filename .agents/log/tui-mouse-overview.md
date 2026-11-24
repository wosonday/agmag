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


## Step 2 — layout overrides (pure)

New tests (written first, seen failing: `LayoutOverride`/`apply_height_overrides` missing):

- `layout_override_parse_roundtrip_and_garbage`
- `layout_override_resets_only_the_named_boundary`
- `layout_override_height_keeps_floors_and_messages_flexible`
- `layout_override_tiny_size_ignores_an_impossible_override`
- `layout_override_shrinks_with_the_window`
- `layout_override_ignores_a_stale_boundary_index`
- `layout_override_top_pair_width_clamped_to_twenty_columns`

Code: `LayoutOverride { top_pair_width: Option<f32>, heights: BTreeMap<usize, f32> }` in
`src/board/global.rs` (one field on `GlobalState`: `layout`), compact `w=...;h2=...` text with
`Display`/`parse` (garbage -> `None`). `section_layout` applies `pair_column_widths` and
`apply_height_overrides` AFTER `fit_heights`: key `Horizontal(i)` sizes the group above it
(wide: `Horizontal(2)` = top pair), Messages takes the remainder, floors `MIN_SECTION_H` /
`MESSAGES_MIN`, >= 20 columns per top panel, stale keys ignored, impossible windows ignore the
override. Green: `cargo test` = **758 passed, 0 failed** (lib 122); clippy clean.


## Step 3 — drag-resize, reset, persistence

New tests (written first, seen failing: no `GEffect::SaveLayout`, no drag routing):

- `mouse_drag_horizontal_resizes_and_clamps` (global.rs)
- `mouse_drag_top_pair_vertical_keeps_twenty_columns` (global.rs)
- `mouse_double_click_border_resets_only_that_boundary` (global.rs)
- `layout_reset_command_clears_and_persists` (app.rs)
- `saved_layout_roundtrips_and_ignores_garbage` (app.rs)
- `horizontal_borders_match_the_group_above_them` (tests/global_view_test.rs)
- `mouse_drag_changes_the_next_frames_rects` (tests/global_view_test.rs)
- `mouse_drag_top_pair_divider_changes_drawn_widths` (tests/global_view_test.rs)

Code: `DragState` on `GlobalState` (one drag field next to the one `layout` field). Down on a border
arms the drag from the last frame's `content_area` + group top; Drag recomputes the ratio live,
clamped (>= `MIN_SECTION_H`, Messages >= `MESSAGES_MIN`, top panels >= 20 columns); Up emits
`GEffect::SaveLayout`; double-click a border resets only that boundary. `handle_global_key` handles
`layout reset` (clears + persists `tui_layout` = "") and `SaveLayout`, and `run_app` loads
`tui_layout` through `saved_layout` next to the theme.

The border mapping test proves `Horizontal(2)`/`Horizontal(3)` sit on the pair's and Inbox's bottom
row in a real 140x40 frame, so the group-above mapping is measured, not guessed.
Green: `cargo test` = **766 passed, 0 failed** (lib 127, global_view_test 45); clippy clean.


## Step 4 — wire the overview mouse into the app loop

New tests (written first, seen failing: `Screen::Global => Next::Skip` ignored every click):

- `overview_mouse_redraws_only_when_something_changed`
- `overview_mouse_never_dispatches_or_confirms`
- `overview_mouse_never_confirms_a_popup`
- `overview_mouse_is_modal_under_the_output_overlay`
- `overview_drag_persists_through_handle_mouse`
- `overview_mouse_double_click_opens_the_board`

Code: `handle_mouse` `Screen::Global` calls `state.on_mouse(...)` and maps the effect exactly like
`handle_global_key`: `OpenBoard` resolves through `terminal_project`, `SaveLayout` writes
`tui_layout` + note, `None` is `Skip` unless something changed (`Redraw`). Run/SaveTheme/Quit are
never produced by the mouse and are ignored defensively (the handle has no dispatcher at all).

Anti-goal matrix: every recorded panel/row/border centre + footer x every event kind, plus every
double-click pair, with and without a popup, and under the Output overlay: no `GEffect::Run`, no
dispatch, no `Next::Quit`/`Restart`, popup text/state unchanged, board opens only on the terminal
double-click.
Green: `cargo test` = **772 passed, 0 failed** (lib 133); clippy clean.


## Step 5 — docs

New test (written first, seen failing: `?` still set a one-line footer note, `data.output` was None):

- `overview_help_lists_mouse_layout_and_escape_hatches` (app.rs) — `?` fills the Output panel and
  the text mentions click, wheel, double-click, drag, `layout reset`, `Shift+drag`, `AGM_MOUSE=0`,
  every line <= 100 columns.

Code: `GEffect::ShowHelp` from `?`; `handle_global_key` shows `OVERVIEW_HELP` in the Output panel.
The footer `HELP`/`HELP_ASCII` line is untouched (no diff there). Docs: one overview sentence in
`README.md` and `README_vi.md`; one Vietnamese bullet under `CHANGELOG.md` `[Unreleased]` ->
`### Thêm mới`. CHANGELOG stayed CRLF (`git ls-files --eol` = `i/crlf w/crlf`), READMEs LF.

