# Task D log — global screen rendering

Worktree `D:/Project/agmag.wt/tui-app-d`, branch `feat/tui-app-d` (base 65d3bca).

## Steps

1. Read the plan (Global Constraints, Review Focus, Task 0 interfaces, Task D), the design
   (including the binding Builder-check answers) and the existing board code
   (`global.rs`, `view.rs`, `theme.rs`, `dashboard.rs`).
2. Wrote the failing tests in `tests/global_view_test.rs` **before** touching the renderer.
   `cargo test --test global_view_test`: 0 passed, 11 failed, 1 ignored (every test hit
   `todo!("Task D")`).
3. Implemented `src/board/global_view.rs`:
   - layout: footer 1 row, command bar 1 row, Messages pinned 6 rows, sections above;
     ≥ 120 columns puts the first two chosen sections side by side and the rest full width,
     < 120 stacks them, tiny sizes give every panel a (possibly 1-row) slot;
   - native Quota bar with the ok/warn/error thresholds (> 50 / 20–50 / < 20 % left),
     window label, reset text, stale / limited badges, `-` for non-finite values;
   - native Terminals `Table` (status symbol, name/address, app, model/effort, status,
     context bar, task, tokens, last kind+direction, place; columns dropped as width shrinks)
     with `TableState` selection auto-scrolled to the selected row;
   - native Inbox `Table` (#id, English kind, terminal, age, text) with selection;
   - Messages list (agent names resolved from `Snapshot::terminals`, newest first, body wrapped);
   - bridged Usage/Budgets/System via `ansi_to_text(render_section_body(..))` scrolled by
     `state.scroll[sec]`;
   - Output overlay on the lower half of the section area, `Clear`ed, scrolled by
     `state.output_scroll`;
   - Confirm popup centred with `Clear` and a frame cursor;
   - command bar `agm> ` with horizontal cell scrolling and the cursor kept visible; footer note
     coloured by `NoteKind` (else key hints);
   - ASCII mode: `+ - |` borders, `#`/`-` bars, `* ~ v x`, `->`, ASCII help text.
4. Re-ran the test file: 11 passed, 1 ignored. `cargo fmt`, then fixed one clippy lint
   (`manual_is_multiple_of`).

## Tests red → green (`tests/global_view_test.rs`)

- `default_view_titles`
- `quota_gauge_colours`
- `terminals_selected_row_visible`
- `inbox_kind_english_and_selected`
- `unknown_values_dash`
- `hostile_text_inert`
- `output_panel_overlays_and_scrolls`
- `confirm_popup_visible`
- `cmdline_long_input_scrolls_cursor_visible`
- `narrow_and_tiny_no_panic`
- `ascii_mode_no_box_drawing`

Ignored (by design, see below): `bridged_sections_render_titles_and_body`
(`#[ignore = "needs Task B (ansi_to_text stub panics)"]`).

## Gates

- `cargo test`: 533 passed, 0 failed, 1 ignored (the bridged test above).
- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.

## Open questions

- **Bridged sections until Task B merges.** `board::ansi::ansi_to_text` is still
  `todo!("Task B")`, so any draw of Usage/Budgets/System panics in this worktree. The renderer
  already calls it exactly as specified; the one test that would exercise those panels is
  `#[ignore]`d with the reason above. Task F should remove the attribute after merging B and the
  test should pass unchanged (it asserts the three titles only).
- **Quota "Gauge".** The design allows "a `Gauge`/bar"; I used the dashboard-style 10-cell bar
  (`█`/`░`, `#`/`-` in ASCII) instead of the `ratatui::widgets::Gauge` widget. It keeps the
  per-row reset/stale/limited text on one line and renders identically at every width; the
  colours are still the specified ok/warn/error thresholds.
- **Messages scrolling.** `GlobalState::scroll` is shared per section, so Messages honours
  `scroll[Section::Messages]` if Task C ever fills it; Task C's spec does not require it.
