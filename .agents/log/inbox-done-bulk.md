# inbox-done-bulk log

## 2026-11-21 start
- Branch: feat/inbox-done-bulk (HEAD a46d8a1 inbox-done-bulk: brief)
- Task: .agents/tasks/inbox-done-bulk.md
- Plan: implement bulk dismiss for CLI and TUI with TDD, commit per step.

## Steps
- [x] 0. prep log
- [x] 1. Add ID selector parser (`actions::parse_done_selectors`, `DoneSelectorParse`) with TDD (red compile -> green). Tests: `tests/actions_test.rs` (`parse_done_id_selectors`, `parse_done_id_selectors_rejects_bad_input`). `cargo test --offline --test actions_test parse_done_id_selectors` => 2 passed. Formatted edited files.
