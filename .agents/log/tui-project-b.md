# tui-project-b log (Task 2: scoped store queries)

Branch: `feat/tui-project-b` · worktree `D:/Project/agmag.wt/tui-project-b`
Scope: only `src/store/project.rs`, `tests/project_store_test.rs`, this log.

## 2026-11-23 · step 1 (red)
- Read plan Global Constraints / Review Focus / Task 0 Interfaces + design spec.
- Task 2 = bodies of `messages_involving`, `latest_turns`, `usage_buckets` in `src/store/project.rs`.
- Confirmed stubs are `todo!("Task 2")`; schema tables `messages`, `turns`, `usage` present;
  rusqlite 0.43 bundled (window functions OK). Seed helpers: `add_message`, `session_id_for_path`,
  `insert_turn`, `upsert_usage`.
- Wrote `tests/project_store_test.rs` with the 3 named tests.
- `cargo test --test project_store_test` → RED: 3 failed, all `not yet implemented: Task 2`
  (`src/store/project.rs:16/22/34`). Test count: 3 new. Next: implement the three bodies (bound params only).

## 2026-11-23 · step 2–4 (green)
- Implemented the three bodies in `src/store/project.rs` (signatures unchanged from Task 0):
  - `messages_involving`: numbered `IN (?1…)` placeholders from slice length, `from_t`/`to_t` match,
    `ORDER BY id DESC LIMIT ?`; empty parties → empty vec.
  - `latest_turns`: `ROW_NUMBER() OVER (PARTITION BY session ORDER BY ts_ms DESC, byte_offset DESC)`,
    `role IN ('assistant','tool')`, `rn = 1`; sessions without such turns absent.
  - `usage_buckets`: SQL `(ts_ms - since)/bucket` grouping, window `[since, since + n*bucket)`,
    sums `input_uncached + cache_write + output + reasoning` (no `cache_read`); zeroed vec per session.
  - All values bound (never formatted into SQL); only fixed role literals inline.
- `cargo test --test project_store_test` → GREEN: 3 passed.
- Gates: `cargo test` → 411 passed / 0 failed (408 baseline + 3 new); `cargo fmt --check` clean;
  `cargo clippy --all-targets -- -D warnings` clean (collapsed a nested `if` into a let-chain).
- Files touched: `src/store/project.rs`, `tests/project_store_test.rs`, this log. No deps added.
- Task 2 complete. Commit: `feat(store): scoped messages, latest turns, usage buckets`.