# Worker h — TUI project scope (tasks 1–3) + review round 1

## Review round 1 fixes

- Read `.agents/reviews/tui-project-scope-1.md` and the plan Global Constraints.
  Reproduced every item with a failing test before fixing (TDD).

### Renderer (`src/project_render.rs`, `tests/project_render_test.rs`)

- Blocker 1: root `last_sibling = false` fixture (also built from real `tree_order`) now pins
  `"\n ├ t7"`, `"\n └ t9"` and no `│` on the root doing line; fixed by skipping stack layer 0
  in the prefix and continuation loops.
- Blocker 2: 17-char project name pins the padded `(no project)` name cell; fixed by padding
  the plain name before `paint`.
- Nên sửa 1: `width = 0` fixture must not panic; fixed `(width - 1)` → `width.saturating_sub(1)`.
- Nên sửa 3: ctx bar pinned present at width 60, absent at 40.
- Gates: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` green.

### Project identity (`src/project.rs`, `tests/project_test.rs`)

- Nên sửa 2: empty terminal names are ignored by `list_projects`, `parties` and `tree_order`
  (an inbox row naming `""` counts as no-project); duplicate names prefer a live terminal,
  then the newest — same order as `Store::terminal`.
- Nhỏ 2: `ambiguous_entries` detects shared names case-insensitively, matching
  `match_project`.
- Gates green; committed.

### Store (`src/store/project.rs`, `tests/project_store_test.rs`)

- Nhỏ 1: `usage_buckets` saturates `since_ms + bucket_ms * n` (extreme-bounds test with
  `i64::MAX` inputs returns the requested sessions with zero buckets instead of panicking).
- Gates green; committed.
