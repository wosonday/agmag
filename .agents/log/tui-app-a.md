# tui-app-a — Task A: shared command dispatcher (binary crate)

Worktree `D:/Project/agmag.wt/tui-app-a`, branch `feat/tui-app-a`, base `65d3bca`.

## What was done

- New `src/tui_cmd.rs` (binary crate):
  - `Ctx { store, cfg, hc, who, now_ms }`, `CmdState` (view, scope, pending, cleanup_list,
    cleanup_selected, roots_cache — moved out of `tui::run`), `Front { Plain, App }`.
  - `CmdState::{new, view, scope, cleanup_list}` plus `pub(crate) roots_cache_mut()` so the plain frame
    reuses the same cwd→root cache as the dispatcher (as before).
  - `dispatch(line, st, ctx, front) -> Dispatched` with the whole per-line handling moved verbatim:
    pending cleanup confirmation, pending kill/interrupt confirmation, then the `match parse_command`.
    `NeedsConfirm(prompt)` replaces the `(Warn, confirmation_note(..))` note; `Caller::User { confirmed:
    true }` is built only after the pending line is `y`.
  - `cleanup_block` / `is_cleanup_listing` shared with `tui::render_frame` so the candidate list is
    drawn exactly as before (byte-identical), and the plain loop never prints it twice.
  - `load_view`, `view_names`, `lookup_project` moved here.
- `src/tui.rs`: keeps stdin thread + resume hand-shake, scan/snapshot/frame rendering/`scoped_text`; it now
  builds a `Ctx` per line, calls `dispatch(.., Front::Plain)` and maps `Dispatched.outcome` to its old
  `note` / `shown` / `board::run` / `quit` paths. `confirmation_note` is now `pub(crate)`.
  `render_frame` still takes `cleanup_list` (`CmdState::cleanup_list()`).
- `src/main.rs`: only `mod tui_cmd;`.

## TDD (red → green)

Red: `cargo test --bin agm tui_cmd` → 11 tests, 0 passed, all panicking at `todo!("Task A")`.

Green (now all pass):
- `kill_needs_confirm_then_y_kills`
- `kill_confirm_other_input_cancels`
- `agent_caller_never_confirms`
- `cleanup_two_step_and_stale_list` (candidate list frozen before `cleanup 1`; prompt names only `t1→t2`)
- `view_sets_and_returns_view`
- `policy_toggles`
- `cd_found_opens_board_both_fronts`
- `cd_t_plain_scopes_app_warns`
- `cd_up_notes_all_projects` (extra, closes the `cd ..`/`cd /` wording gap)
- `unknown_command_is_err_note`
- `quit_is_quit`

Existing `tui.rs` tests and all board/tui tests stayed green without edits.

## Gates

- `cargo test`: 40 suites, all `ok` (`--bin agm`: 18 tests, incl. the 11 new; lib: 31; 0 failed anywhere).
- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.

## Open questions / decisions

1. **Width for `Outcome::Shown`.** `Ctx` (frozen) has no width, but `cd` list / `show` / cleanup list were
   width-dependent. `dispatch` uses `crate::tui::pane_width()` so `--plain` output stays byte-identical;
   the app (Task D) may re-wrap the text in its panel. One extra `pane_width()` call per line, not per frame.
2. **Cleanup listing is returned both ways.** The plan asks for `Shown(text)` *and* for `render_frame` to keep
   drawing `cleanup_list`. `dispatch` returns the block for the app; the plain loop suppresses its `Shown`
   copy when `is_cleanup_listing(&text)` and a list is present, so nothing prints twice and `show`-while-list
   still prints. Decision recorded here because the plan does not name a discriminator.
3. **`cd ..` / `cd /` wording.** App with no scope → `already at all projects`; plain keeps today's exact
   `back to all projects` (even at the top level) to stay behaviour-neutral.
4. **`cd -t` in the app** warns (`text project view: use \`agm tui --plain\``) for every argument, including
   `..`/`/`, because the app has no text scope at all.
5. **Snapshots for `cd` lookups.** `Ctx` has no `Snapshot`, so `dispatch` builds one with
   `dashboard::snapshot` for `cd` / `cd -t` instead of reusing the frame's. Same data, one extra read per
   `cd` line; the plain loop keeps its per-frame snapshot for rendering.
