# inbox-done-bulk log

## 2026-11-21 start
- Branch: feat/inbox-done-bulk (HEAD a46d8a1 inbox-done-bulk: brief)
- Task: .agents/tasks/inbox-done-bulk.md
- Plan: implement bulk dismiss for CLI and TUI with TDD, commit per step.

## Steps
- [x] 0. prep log
- [x] 1. Add ID selector parser (`actions::parse_done_selectors`, `DoneSelectorParse`) with TDD (red compile -> green). Tests: `tests/actions_test.rs` (`parse_done_id_selectors`, `parse_done_id_selectors_rejects_bad_input`). `cargo test --offline --test actions_test parse_done_id_selectors` => 2 passed. Formatted edited files.
- [x] 2. Store bulk resolve + actions bulk dismiss core. Added `Store::resolve_inbox_many` (single SAVEPOINT transaction) in `src/store/v2.rs`; added `DoneFilters/DoneSkip/DoneReport`, kind/source normalization, and `actions::dismiss_done` in `src/actions.rs` (kept legacy `dismiss`). Added action tests for range/skip reasons, kind alias+Vietnamese equivalence, terminal+kind AND, explicit `--source blocked`, and legacy single closing blocked. `cargo test --offline --test actions_test -q` => 23 passed. Formatted edited files.

- [x] 3. CLI/TUI wiring, parser integration, docs, changelog, and verification.
  - src/main.rs: Cmd::Done now accepts IDS... plus --kind/--terminal/--source; validates args/filters, preserves legacy single behavior/output/JSON, bulk text/JSON output, Nothing to dismiss. (exit 0).
  - src/dashboard.rs: TuiCmd::Done carries ids/filters/flags; TUI_COMMANDS done syntax <#|a-b|kind:K|term:T>; added tokenized parsing for kind:, 	erm: (and source:/src:), supports quoted kind values and combinations with IDs.
  - src/tui.rs: executes ctions::dismiss_done, reports dismissed N (skipped M) or Nothing to dismiss..
  - Tests updated/added: 	ests/dashboard_test.rs (parse_commands for ranges/lists/filters/combined, usage/empty cases), 	ests/cli_test.rs (cli_done_bulk_and_legacy_cases: no args exit!=0, single missing exit!=0, legacy single JSON {"done":id}, bulk range JSON).
  - Docs: README.md, README_vi.md done command syntax/description updated. CHANGELOG.md [Unreleased] -> ### Thêm mới entry added (kept LF).
  - Verification: cargo test --offline -q => all test targets passed, 0 failed. cargo clippy --offline --all-targets -- -D warnings => clean (fixed collapsible_if in ctions::done_item_matches_filters). cargo fmt --check => clean. Existing cli_done_dismisses_inbox_item and cli_help_is_english_for_every_command remain green.
  - Note: ran with temp AGM_HOME in tests; did not run gm done on real inbox.
