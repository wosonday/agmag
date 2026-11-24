# tui-layout log

Task: `.agents/tasks/tui-layout.md` — T1 content-fitted overview, compact Terminals, hit map (OVERVIEW only).

## Step 1 — `fit_heights`

- RED: added 5 unit tests in `src/board/global_view.rs` (`fit_heights_empty_input_is_empty`, `fit_heights_surplus_goes_to_flex`, `fit_heights_deficit_shrinks_largest_first`, `fit_heights_never_below_min`, `fit_heights_sums_to_total`); `cargo test --lib fit_heights` failed with `E0432 unresolved import`.
- CODE: added pure `pub fn fit_heights(desired, flex, total, min) -> Vec<u16>`: clamp each desired to `min`, give surplus to the flexible index, take a deficit from the largest section first (lowest index wins ties), never shrink below `min`. Empty input → empty vec.
- GREEN: `cargo test --lib fit_heights` — 5 passed. `cargo fmt --check` clean.
- Corrected two mistaken test expectations while red/green (surplus is `total - sum`, not `total`).

## Step 2 — content-fitted overview heights

- RED: added `panel_rect`/`blank_inner_rows` helpers and two render tests in `tests/global_view_test.rs` (`wide_overview_fits_content_and_inbox_is_three_rows` at 140×40 and 200×50, `narrow_overview_stacks_content_fitted_panels` at 100×40) with 11 terminals / 0 inbox: Inbox was 12–15 rows instead of 3.
- CODE: removed fixed `MESSAGES_H = 8`; added `MESSAGES_MIN = 4`, `MIN_SECTION_H = 3`, `PAIR_CAP_PERCENT = 60`. Added `section_desired_rows` (Quota lines+2, Terminals header+rows+2, Inbox items+header+2 min 3, bridged body lines+2) and `section_layout`, which fits *groups* in one pass — the top pair is one group = max of the pair's desired capped at 60 % of the sections area; Messages is the last, flexible group. `split_messages`/`draw_sections`/`draw_stacked` now consume the fitted heights; Messages is no longer fixed at 8.
- GREEN: both new tests plus the whole suite pass. Updated `hostile_text_inert` bound (a taller Messages panel legitimately renders more of the hostile string) while keeping the per-row screen-width clipping check.
- Gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all clean.

## Step 3 — compact Terminals

- RED: added unit tests in `src/board/global.rs` (`ordered_terminals_puts_plain_shells_last`, `down_follows_the_visual_terminal_order`) and render tests in `tests/global_view_test.rs` (`terminals_title_counts_agents_and_shells`, `terminals_hides_columns_that_are_all_dashes`, `terminals_agents_first_and_shells_dimmed`): compile error for the missing shared ordering function.
- CODE: `global.rs` now owns `is_plain_shell` (shell app set with no linked model/effort) and `ordered_terminals(snap)` — the single visual order (agents in original order, then plain shells). `sync`, `panel_up` and `panel_down` all use it, so selection stays by address while arrows follow the rows. `draw_terminals` uses the same order, hides every all-`-` column (with `task` only when it has data), dims shell rows with `theme.muted`, and titles the panel `Terminals (N agents · M shells)` (omitting zero parts). Name/model get their content width before `task` takes the remainder.
- GREEN: all unit + render tests pass; corrected three mistaken test expectations (input order, agent/shell y-order) while red/green.
- Gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all clean.

## Step 4 — hit map

- RED: added `hit_map_prefers_borders_then_rows_then_panels` in `src/board/global.rs` and `hit_map_records_what_the_overview_drew` / `hit_map_is_rebuilt_every_frame` in `tests/global_view_test.rs`: unresolved imports and no `hit_map()` on `GlobalState`.
- CODE: `global.rs` now defines `HitMap { panels, rows, borders }`, `RowTarget` (terminal/inbox/message/agent/project), `BorderId` (top-pair vertical, horizontal-by-section-index), `Hit` and the pure `hit(map, x, y)` with border → row → panel priority. `GlobalState` holds a `RefCell<HitMap>` with `hit_map()` / `clear_hit_map()` / `push_*`; `draw_global` clears and refills it every frame without changing any `app.rs` call site. Renderers record panel rects, the top-pair divider, horizontal boundaries, terminal/inbox table rows (with the table's scroll offset) and message-row rects. No mouse events are handled.
- GREEN: unit + render hit-map tests pass, including a stale-row check after a second draw.
- Gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all clean.

## Step 5 — CHANGELOG + final gates

- Added a `[Unreleased]` section above `## [0.5.0]` in `CHANGELOG.md` with one Vietnamese bullet under `### Thay đổi` covering the content-fitted overview and compact Terminals. CRLF preserved (`git ls-files --eol CHANGELOG.md` → `i/crlf w/crlf`).
- Final gates (all clean, recorded): `cargo fmt --check` → ok; `cargo clippy --all-targets -- -D warnings` → Finished, zero warnings; `cargo test` → all 42 test binaries report `ok`, 0 failed.
- No mouse handling, no resize, no edits to `src/board/mod.rs`, `state.rs` or `view.rs`; app.rs call sites unchanged. No push/merge/branch switch.

## Fix round 1

Review: `.agents/reviews/tui-layout-1.md` (CHƯA ĐẠT). TDD per item (RED → CODE → GREEN → log → commit).

### Item 1 — top-pair floor/cap

- RED: `top_pair_floor_is_one_panel_and_cap_does_not_squeeze_messages` in `tests/global_view_test.rs`: at 140×40 with 1 quota line + 1 terminal, Quota had 3 blank inner rows; at 140×10 (content height 8) the pair was 4 rows and Messages only 4.
- CODE: `section_layout` computes `cap = 60% of (main.height - MESSAGES_MIN)` without inflating it to `2 * MIN_SECTION_H`; `pair = max(desired, MIN_SECTION_H)` after applying the cap, so the floor is one panel's minimum (3), not two.
- GREEN: new test passes; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` clean.
- EOL: `git ls-files --eol` unchanged (`i/lf w/lf` for all touched files).

### Item 2 — clamp every rect to `main`

- RED: `every_rect_stays_inside_main_at_tiny_heights` in `tests/global_view_test.rs` over the requested matrix (100×1…8, 140×3…10): a `Panel(Terminals)` rect and `BorderId::Horizontal` rect reached `y = 3` while `main.bottom() = 2` at 100×4.
- CODE: `split_messages` clamps `layout.messages` to `main.height` before deriving `sections_h`; `draw_global` only records the Messages panel/border when its height is non-zero; the top-pair divider strip is clamped to `pair.right()`.
- GREEN: matrix test passes; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` clean.

### Item 3 — drop stacked sections that do not fit

- RED: `no_panels_overlap_at_tiny_heights` (same matrix) — at 100×6 a 1-row `Terminals` panel drawn at `y = 1` overlapped the 3-row Messages panel at `y = 1..4`.
- CODE: `draw_stacked` now reduces a height all the way to 0 when the area cannot fit the minimum (previously `saturating_sub(1)` stopped at 1); zero-height sections are skipped entirely — not drawn and not pushed to the hit map — so the remaining panels stay packed and non-overlapping.
- GREEN: overlap matrix passes with the bounds matrix; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` clean.

### Item 4 — message rows use the real wrapped height

- RED: `message_row_rects_follow_wrapped_height`: at width 100 a 250-cell message wraps to header + 3 text rows, but the Message(0) rect was the hard-coded 2 rows, so the Message(1) rect did not cover its drawn text.
- CODE: `wrapped_line_rows` counts rows with ratatui's wrap rule (greedy word packing, trimmed leading whitespace, long words broken at the row width) using grapheme widths. `draw_messages` records one rect per message from the summed header/text row counts, clipped to the visible/scrolled window.
- GREEN: new test passes; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` clean.

### Item 5 - Output overlay hit map

- RED: `output_overlay_hides_covered_hit_entries` (140x40, 11 terminals, Output open): no `HitPanel::Output` existed and compile failed on the missing type/border variant.
- CODE: `HitPanel { Section, Output }` replaces `Section` in `HitMap.panels`/`Hit::Panel` (`From<Section>` keeps call sites simple); added `BorderId::OutputTop`. Extracted `output_rect` so drawing and hit mapping agree; `draw_global` clips away entries under the overlay, then records the Output panel and its top boundary.
- GREEN: new test passes (covered panels/rows/borders clipped, Output and its border hit-testable); `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` clean.

### Item 6 - desired rows for empty Terminals and top-pair columns

- RED: `empty_terminals_panel_is_three_rows` (empty Terminals was 4 rows) and `top_pair_bridged_sections_use_their_column_width` (Usage in a 60-column top-pair column was sized from the 120-column window: 7 rows instead of ≥8). Re-verified the second test fails with the old width argument before restoring the fix.
- CODE: `section_desired_rows` counts one `-` line (3 rows with borders) for an empty Terminals; `pair_column_widths` mirrors `draw_sections`' two columns and is used both to measure bridged bodies in `section_layout` and to build the same constraints in `draw_sections`.
- GREEN: both tests pass; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` clean.
