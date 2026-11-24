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
