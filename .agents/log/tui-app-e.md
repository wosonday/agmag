# tui-app-e log — Task E (app loop and screens)

Branch `feat/tui-app-e`, base `65d3bca` (phase 2 scaffold). Scope: `src/board/app.rs`,
`src/board/mod.rs`, `tests/tui_app_test.rs`; no frozen Task 0 signature/type changed.

## Steps

- [x] 1. Wrote the failing tests first.
  - `src/board/mod.rs` (in-file): `board_screen_leave_on_q` — `BoardScreen` with a project:
    `↓` → `Stay` and `needs_refresh() == false`, `q` → `Leave`; without a project the board opens
    on the picker, where `esc` cancels the picker (`Stay`), as `agm board` does today.
  - `src/board/app.rs` (in-file): `apply_dispatched_maps_each_outcome` (Note / Shown /
    NeedsConfirm / OpenBoard / Quit / None + `view` replaced every time and the Output panel
    closed by each command), `terminal_without_project_is_warn` (name in the note, address when
    the row is unnamed).
  - `tests/tui_app_test.rs`: `run_app_signature_matches_the_frozen_interface` (coerces `run_app`
    to an explicit `for<'s, 'd> fn(...)` type). The behavioural helpers are `pub(crate)` as the
    plan requires, so an integration test cannot call them — that test lives in-file; the
    integration file pins the public surface Task F wires.
- [x] 2. Red: `cargo test --lib board` failed to compile (`BoardScreen`, `BoardKeyResult`,
  `terminal_project` missing) — the repo's usual red state for tests of new items.
- [x] 3. Implemented.
  - `src/board/mod.rs`: `run_loop`'s body split into `pub(crate) struct BoardScreen { state, data,
    cache, refresh_note, needs_refresh }` with `new(project, theme)`, `theme()`, `refresh`,
    `needs_refresh()`, `draw(frame, palette, ascii)` and `on_key(key, store, hc, now) ->
    BoardKeyResult { Stay, Leave }` (the old key handling verbatim: `SwitchProject` updates the
    state, `Quit` = `Leave`, anything else runs `apply_effect` and asks for a refresh). `run_loop`
    now only draws/refreshes/polls; `board::run` builds one `BoardScreen`. `TermGuard` /
    `TermControl` are `pub(crate)`; the guard + panic hook sequence was extracted to
    `pub(crate) fn with_console(body)` and `run` uses it, so the standalone board behaves exactly
    as before (existing `board_*` tests unchanged and green).
  - `src/board/app.rs`: `run_app` = `with_console` + both screens (`Screen::Global`,
    `Screen::Board(Box<BoardScreen>)`; boxed for `clippy::large_enum_variant`). Every refresh:
    `scan` → `GlobalSide::refresh` (`snapshot`, `compute_roots` → `list_projects` →
    `address → ProjectKey`, `sync`; failed refresh keeps the last good data and shows the error,
    like the board) → board screen refresh when active. First frame's `now_ms` comes from
    `clock_ms()`.
  - `GlobalSide` also keeps the `SysSampler` the plain UI keeps, and `snap.system` is filled from
    it, so the bridged SYSTEM section shows the same readings as `--plain` (global constraint
    "no change to the data a section shows"); a failure to sample stays `None` = unknown.
  - Keys: `ctrl+c` (Press/Repeat only) quits from either screen before the screens see it. Global:
    `GlobalState::on_key` → `Run` dispatches (the popup's line, including `esc` → `""`, goes
    through the same path per builder-check answer 7), `SaveTheme` persists `board.theme` with
    the board's `theme: …` / `error: …` note, `OpenBoard(address)` resolves via
    `terminal_project`, `Quit` leaves. Board: `Leave` → `Screen::Global`, `focus = CmdLine`, and
    the global screen adopts the board's theme (so `t` in the board applies app-wide). Entering a
    board closes the Output panel and refreshes through the outer loop (same cadence as before).
  - `pub(crate) fn apply_dispatched(state, data, d) -> Option<ProjectKey>`: one command replaces
    the previous result as one line does in `--plain` (note and Output panel cleared, then the
    outcome sets note / output / popup), `data.view` always replaced, `OpenBoard` returned.
  - `pub(crate) fn terminal_project(data, address, name) -> Result<ProjectKey, String>`:
    `project_of[address]`, else `<name> has no project` (address when the row has no name).
      The Output panel also closes on `esc` in the global screen (builder-check answer 1);
      `Release` events never act.
- [x] 4. Green gates:
  - `cargo test` → 41 targets, 526 passed, 0 failed (was 522 before this task: +3 in-file,
    +1 `tests/tui_app_test.rs`).
  - `cargo fmt --check` → clean (note: `src/board/mod.rs` is committed with CRLF in this repo;
    the working file was kept CRLF so the diff stays small, rustfmt's `Auto` accepts it).
  - `cargo clippy --all-targets -- -D warnings` → clean.
- [x] 5. Commit `feat(tui): app loop hosting global and board screens` (this log included).

## Test names (new)

`apply_dispatched_maps_each_outcome`, `terminal_without_project_is_warn`,
`board_screen_leave_on_q`, `run_app_signature_matches_the_frozen_interface`.

## Open questions

1. `BoardScreen::new` takes `(project, theme)`; the plan sketch shows `new(key)`. The board must
   open with the app's theme (and `t` in the board then applies app-wide), so the theme is passed
   in. `board::run` passes the saved setting, `run_app` passes `GlobalState.theme`.
2. The plan's refresh list for `run_app` does not mention system sampling. I kept the plain UI's
   `SysSampler` and fill `snap.system` so SYSTEM is not silently "not measured" in the app; it is
   a pure addition to the listed steps.
3. `apply_dispatched` returns only the project to open, so `Outcome::Quit` is detected by the
   caller before applying (the helper clears the previous note/output like any command, which is
   harmless because the loop returns). The named test asserts the map for every outcome.
4. `tests/tui_app_test.rs` can only pin the public `run_app` signature: the plan's pure helpers
   are `pub(crate)`, so their behaviour is covered in-file. Flagging in case the CEO wanted them
   `pub` after all.
5. No manual terminal run was possible here (no interactive TTY in this worktree); review focus
   4's "board → `q` → global with the command bar focused, `ctrl+c` restores the terminal" is
   covered at state level (`board_screen_leave_on_q`, the leave path sets `focus = CmdLine`) and
   still needs the Task F manual preview.
