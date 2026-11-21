# inbox-done-bulk log

## 2026-11-21 start
- Branch: feat/inbox-done-bulk (HEAD a46d8a1 inbox-done-bulk: brief)
- Task: .agents/tasks/inbox-done-bulk.md
- Plan: implement bulk dismiss for CLI and TUI with TDD, commit per step.

## Steps
- [x] 0. prep log
- [x] 1. Add ID selector parser (`actions::parse_done_selectors`, `DoneSelectorParse`) with TDD (red compile -> green). Tests: `tests/actions_test.rs` (`parse_done_id_selectors`, `parse_done_id_selectors_rejects_bad_input`). `cargo test --offline --test actions_test parse_done_id_selectors` => 2 passed. Formatted edited files.
- [x] 2. Store bulk resolve + actions bulk dismiss core. Added `Store::resolve_inbox_many` (single SAVEPOINT transaction) in `src/store/v2.rs`; added `DoneFilters/DoneSkip/DoneReport`, kind/source normalization, and `actions::dismiss_done` in `src/actions.rs` (kept legacy `dismiss`). Added action tests for range/skip reasons, kind alias+Vietnamese equivalence, terminal+kind AND, explicit `--source blocked`, and legacy single closing blocked. `cargo test --offline --test actions_test -q` => 23 passed. Formatted edited files.
