# tui-layout log

Task: `.agents/tasks/tui-layout.md` — T1 content-fitted overview, compact Terminals, hit map (OVERVIEW only).

## Step 1 — `fit_heights`

- RED: added 5 unit tests in `src/board/global_view.rs` (`fit_heights_empty_input_is_empty`, `fit_heights_surplus_goes_to_flex`, `fit_heights_deficit_shrinks_largest_first`, `fit_heights_never_below_min`, `fit_heights_sums_to_total`); `cargo test --lib fit_heights` failed with `E0432 unresolved import`.
- CODE: added pure `pub fn fit_heights(desired, flex, total, min) -> Vec<u16>`: clamp each desired to `min`, give surplus to the flexible index, take a deficit from the largest section first (lowest index wins ties), never shrink below `min`. Empty input → empty vec.
- GREEN: `cargo test --lib fit_heights` — 5 passed. `cargo fmt --check` clean.
- Corrected two mistaken test expectations while red/green (surplus is `total - sum`, not `total`).
