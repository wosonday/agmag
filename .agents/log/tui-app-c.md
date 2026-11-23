# tui-app-c log — Task C: Global state and keys

## Scope
- Owned: `src/board/global.rs`
- Read: plan/spec Task 0 interfaces, `src/board/cmd.rs`, `src/board/state.rs`, `src/dashboard.rs`, `src/board/theme.rs`

## Steps
1. Read task/plan/spec and existing scaffold. Confirmed Task 0 frozen interfaces in `global.rs` (`GlobalState`, `CmdLine`, `Popup`, `GEffect`, `GlobalData`, `Focus`, `new/on_key/sync`).
2. Wrote 15 named failing tests in `src/board/global.rs` covering the Task C behavior matrix (release ignored, ctrl+c everywhere, cmdline typing/enter/blank-enter/history/cursor-editing, confirm popup y/esc, tab cycling, inbox reply/show/done, terminal open/send, unbound printable in panel, sync neighbour/hidden-focus/empty).
3. Ran targeted tests: `cargo test -p agmag --lib board::global` — failed to compile due test-only imports (`QuotaRow/UsageBucket/UsageSummary` imported from private dashboard re-exports; `UsageSummary` missing fields). Fixed imports to `crate::report::*` and added missing `UsageSummary` fields.
4. Implemented `GlobalState::on_key`, `sync`, and helpers in `src/board/global.rs`.
5. Re-ran targeted tests: all 15 passed.
6. Ran gates and fixed issues: `cargo fmt --check` flagged formatting in tests/helper; fixed. `cargo clippy --all-targets -- -D warnings` flagged `collapsible_if` in `sync`; collapsed to let-chain. Re-ran gates clean.

## Tests (red → green)
- `release_ignored` — red (todo scaffold) → green
- `ctrl_c_quits_everywhere` — red → green
- `typing_and_enter_runs_command` — red → green
- `blank_enter_does_nothing` — red → green
- `history_up_down_and_dedupe` — red → green
- `cursor_editing_utf8` — red → green
- `confirm_popup_y_enter_runs_y` — red → green
- `confirm_popup_esc_runs_empty` — red → green
- `tab_cycles_visible_panels_and_back` — red → green
- `inbox_r_prefills_reply` — red → green
- `inbox_enter_runs_show` — red → green
- `terminal_enter_opens_board` — red → green
- `terminal_s_prefills_send_with_name` — red → green
- `unbound_printable_in_panel_goes_to_cmdline` — red → green
- `sync_moves_to_neighbour_and_drops_hidden_focus` — red → green

Targeted: `cargo test -p agmag --lib board::global -- --nocapture` → **15 passed, 0 failed**.

## Gates
- `cargo test` → **PASS** (no FAILED/failures in output; doc-tests 0 passed; full workspace test run completed cleanly)
- `cargo fmt --check` → **PASS**
- `cargo clippy --all-targets -- -D warnings` → **PASS**

## Open questions / notes
- `sync` "nearest neighbour when gone": `GlobalState` is frozen (Task 0) and has no stored previous index, so when a selected terminal/inbox item disappears the implementation falls back to a clamped last existing item. The named test accepts either adjacent neighbour; true geometric nearest-neighbour for removals in the middle of long lists may need a stored index if Task 0 types are extended later.
- `visible_panels` treats `data.view` entries (excluding Messages) in given order and always appends `Messages` as the pinned panel; this matches "view + Messages pinned/never counted".
- Printable char insertion in CmdLine/popup ignores CONTROL/ALT-modified chars to avoid inserting control glyphs; plain printable chars insert at cursor.
