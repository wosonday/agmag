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
  - src/dashboard.rs: TuiCmd::Done carries ids/filters/flags; TUI_COMMANDS done syntax <#|a-b|kind:K|term:T>; added tokenized parsing for kind:, term: (and source:/src:), supports quoted kind values and combinations with IDs.
  - src/tui.rs: executes actions::dismiss_done, reports dismissed N (skipped M) or Nothing to dismiss..
  - Tests updated/added: tests/dashboard_test.rs (parse_commands for ranges/lists/filters/combined, usage/empty cases), tests/cli_test.rs (cli_done_bulk_and_legacy_cases: no args exit!=0, single missing exit!=0, legacy single JSON {"done":id}, bulk range JSON).
  - Docs: README.md, README_vi.md done command syntax/description updated. CHANGELOG.md [Unreleased] -> ### Thêm mới entry added (kept LF).
  - Verification: cargo test --offline -q => all test targets passed, 0 failed. cargo clippy --offline --all-targets -- -D warnings => clean (fixed collapsible_if in actions::done_item_matches_filters). cargo fmt --check => clean. Existing cli_done_dismisses_inbox_item and cli_help_is_english_for_every_command remain green.
  - Note: ran with temp AGM_HOME in tests; did not run agm done on real inbox.

## Lệch so với tệp tác vụ (bổ sung sau review, CEO)
- Lỗi selector không hợp lệ trong TUI (`done x`, `9-3`, `1-`, khoảng quá lớn): trả thông báo lỗi cụ thể từ parser thay vì luôn `usage: done <#|a-b|kind:K|term:T>`; `usage` vẫn dùng khi trống/thiếu đối số.
- TUI nhận thêm token `source:`/`src:` (ngoài `kind:`/`term:`) để khớp `--source` của CLI và quy tắc an toàn `blocked`.
- Test CLI bulk tạo mục inbox trực tiếp qua `Store` (AGM_HOME tạm) thay vì `agm send`, để kiểm soát kind/ID.

## Vòng sửa sau review (CEO, theo .agents/reviews/inbox-done-bulk-review.md)
- F1 (chặn): `parse_selector_arg` dùng `end - start >= 10_000` thay cho `end - start + 1 > 10_000` (tràn i64 ở `0-9223372036854775807`); test biên 10000/10001, i64::MAX.
- F2: `README_vi.md` dòng `cmd:` của TUI dùng cú pháp mới.
- F3/F5: log này chuẩn hóa LF; ký tự TAB/BEL (do `` `t ``/`` `a `` trong PowerShell) trả lại thành chữ `t`/`a`.
- F4: test CLI `cli_done_range_and_filters_never_close_blocked_items` và test TUI `source:`/`src:blocked`.
