# tui-board-b log

## Task
Plan Task 2: State and key handling (`src/board/state.rs`).

## Step 1: Failing tests
Added the 16 named tests in `src/board/state.rs` under `#[cfg(test)]`:
- `release_events_ignored`
- `tab_cycles_panels_both_ways`
- `navigation_clamps_without_wrap`
- `enter_on_project_switches`
- `reply_popup_types_and_sends`
- `reply_blank_does_not_send`
- `send_popup_tab_cycles_kind_and_emits_stored_code`
- `done_requires_y`
- `esc_cancels_popup_without_effect`
- `q_quits_in_normal_mode_only`
- `theme_key_cycles_and_saves`
- `filter_set_and_clear`
- `sync_keeps_selection_by_address_after_reorder`
- `sync_moves_to_neighbor_when_agent_gone`
- `sync_closes_popup_for_closed_item`
- `sync_on_empty_data_is_none`

Initial run: `cargo test board::state --lib` failed to compile due test helper using `KeyEvent::with_kind` (crossterm 0.29 uses `new_with_kind`); fixed helper. After fix, all 16 tests failed with `todo!("Task 2")` as expected.

## Step 2: Confirmed failure
`cargo test board::state --lib` -> 0 passed; 16 failed; 0 ignored; 5 filtered out.

## Step 3: Implement
Implemented `BoardState::{on_key,sync}` and private helpers in `src/board/state.rs`:
- `on_key`: ignores `KeyEventKind::Release`; `Ctrl+C` quits in any mode; dispatches normal/popup modes.
- Normal mode: Tab/BackTab panel cycle, directional navigation with clamp/no wrap (Projects sel_project, Orchestration sel_agent, Detail msg_scroll, Inbox sel_inbox; h/l/Left/Right move agent in Orchestration), Enter actions (project switch sets project+panel and emits `SwitchProject`, orchestration focuses Detail, inbox opens ShowItem), `r` opens Reply or sets "no inbox item selected" note, `d` ConfirmDone, `s` Send with agent address and kind 0, `/` Filter with current filter, `t` cycles theme and emits `SaveTheme`, `p` PickProject, `?` Help, `q`/Esc Quit.
- Popups: printable chars append (ignoring Control/Alt), Backspace pops, Esc cancels to Normal (Filter also clears filter). Reply Enter sends trimmed non-blank text as `Effect::Reply`; blank stays. Send Tab cycles `KINDS_EN`, Enter emits stored code `KINDS_EN[kind].1`. ConfirmDone `y/Y` emits Done; any other key cancels. PickProject moves/selects and emits SwitchProject. ShowItem/Help any key closes.
- `sync`: clamps sel_project; keeps selected agent by address, else moves to nearest remaining by previous index (or first/None), auto-selects first agent when selection is None and agents exist; same nearest-by-index behavior for inbox ids; closes Reply/ConfirmDone/ShowItem popup when referenced inbox id is gone with note `item #N is closed`.

Added private state fields `agent_idx` and `inbox_idx` to track previous selection indices for sync-nearest behavior. This is a private field addition (no public signature change).

## Step 4: Pass + gates
- `cargo test board::state --lib` -> 16 passed; 0 failed.
- `cargo fmt --check` -> clean (after `cargo fmt`).
- `cargo clippy --all-targets -- -D warnings` -> clean after collapsing 3 `collapsible_if` helpers as clippy suggested.
- `cargo test` (full) -> 206 passed; 0 failed across 20 test result lines (includes 1 doc-test line with 0 tests).

## Open questions
- Added private fields `agent_idx: usize` and `inbox_idx: usize` to `BoardState` to implement "nearest remaining ... by previous index" on sync. The plan froze signatures/types; I treated private fields as an internal detail, but it is a struct layout change. If Task 3/4 construct `BoardState` with struct literals, they will need updating (unlikely; they should use `BoardState::new`).
- `d` on a selected inbox item with no selection does nothing (no note), mirroring the plan's minimal spec; `r` explicitly sets the "no inbox item selected" note as specified.
- Detail `msg_scroll` increments without an upper bound because max depends on rendered conversation length not present in state; view should clamp on render. This keeps navigation clamping/no-wrap behavior for the lower bound as specified.
