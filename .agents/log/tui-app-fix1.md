# tui-app-fix1 — phase 2 review round 1 fixes

Worktree `tui-app-fix1`, branch `feat/tui-app-fix1`, base `d8714df` (review `05cb047`).
Every item was written test-first (failing test → fix → green).

## Commits

| Commit | Items |
|---|---|
| `564fa87` fix(tui): command bar underflow and hostile text | 1, 2 |
| `dfea59e` fix(tui): refresh after commands, keep selection neighbours, skip board-tick global refresh | 3, 5, 8 |
| `3d39ed2` fix(tui): scroll Output from the command bar; share the visible panel list | 4, 7 |
| `a54d02f` fix(tui): output colours, content-sized terminal columns, one-line messages | 6, 9, 10 |
| `f823b3d` fix(tui): keep terminal column headers readable | 9 |
| `9a74e94` fix(tui): six message rows in the Messages panel | 10 |

## Bắt buộc sửa

1. **Cmdline underflow at tiny widths** — `src/board/global_view.rs` (`draw_cmdline`).
   - Test `cmdline_tiny_widths_no_panic` (`tests/global_view_test.rs`, widths 1..=8 × heights 2..=4,
     focus `CmdLine`, 10-char text). RED: `attempt to subtract with overflow` at
     `cursor_cells - scroll`. Fix: return `None` and draw nothing when
     `available = width - "agm> " == 0`; all cursor/scroll math saturating. GREEN.
2. **Unsanitized text in the command bar** — `src/board/global.rs`, `global_view.rs`.
   - `control_chars_not_inserted` (in-file): RED: `"\x1b\x07\n\r\x7f"` inserted into `cmd.text`
     and the popup input. Fix: `!c.is_control()` on `KeyCode::Char` in the command line, the
     confirm popup and the panel's unbound-printable path. GREEN.
   - `send_prefill_sanitized` (in-file): RED: prefill kept `\x1b]0;pwn\x07`/`\x1b[31m` from the
     terminal name/address. Fix: `sanitize_one_line` on the name/address (empty sanitized name
     falls back to the sanitized address). GREEN.
   - `cmdline_draw_hostile_text_inert` (`tests/global_view_test.rs`): RED: OSC payload reached the
     buffer. Fix: `sanitize_one_line` the command text before measuring/drawing the cursor slice.
     GREEN.
3. **Refresh right after a dispatched command** — `src/board/app.rs`.
   - `run_effect_requests_refresh` (in-file; helper `effect_requests_refresh`): RED: helper absent.
     Fix: refresh for every `Outcome` except `Quit`/`OpenBoard`; the loop's new `AppAction::Refresh`
     runs `global.refresh` (snapshot + `state.sync`) before redrawing, keeping `note`, `output`,
     `popup`. GREEN.
   - `sync_clears_closed_inbox_selection` (in-file): RED: after the closed item vanished, the old
     selection stayed (until sync). Fix: `sync` moves to the neighbour. GREEN.
4. **PgUp/PgDn from the command bar** — `src/board/global.rs` (`on_key_cmdline`).
   - `pgdn_scrolls_output_from_cmdline` (in-file): RED: paging ignored in the command bar. Fix:
     `PageUp`/`PageDown` adjust `output_scroll` by 5 when `data.output.is_some()`. GREEN.

## Nên sửa

5. **`sync` keeps the neighbour, not the last row** — `src/board/global.rs`.
   - `sync_selects_neighbour_after_middle_removal` (in-file; rows 1..5, select 3, remove 3):
     RED: selection jumped to the last row. Fix: private `prev_terminal_idx` / `prev_inbox_idx`
     fields (also updated by panel Up/Down) let `sync` clamp the previous index
     (`prev.min(len - 1)`). `GlobalState::new`/pub fields unchanged. GREEN.
6. **Output panel keeps SGR colours** — `src/board/global_view.rs` (`draw_output`).
   - `output_keeps_sgr_colour` (`tests/global_view_test.rs`): RED: `\x1b[1;32mok` lost its green.
     Fix: `ansi_to_text(text)` (bridge still drops controls/non-SGR) + `Wrap { trim: false }`,
     keeping the scroll offset. GREEN.
7. **One visible-panel list for `Tab` and drawing** — `src/board/global.rs` / `global_view.rs`.
   - `tab_matches_drawn_panels_for_empty_and_messages_only_view` (in-file, empty view and
     `[Messages]`): RED: `Tab` visited only `[Messages]` while drawing fell back to
     `Section::DEFAULT`. Fix: `pub(crate) fn visible_sections(view: &[Section]) -> Vec<Section>`
     (defaults when empty, messages pinned last) used by `visible_panels` (keys) and
     `draw_sections` (which pops the pinned Messages). GREEN.
8. **No global refresh every board tick** — `src/board/app.rs`.
   - `board_tick_skips_the_global_refresh` (in-file; helper `refresh_targets(ScreenKind)`): RED:
     helper absent. Fix: `ScreenKind::Global` → `{ global: true, board: false }`,
     `ScreenKind::Board` → `{ global: false, board: true }`; returning to the global screen
     refreshes it on the next tick. GREEN.

## From the CEO preview

9. **Terminals columns sized from content** — `src/board/global_view.rs` (`draw_terminals`).
   - `terminals_columns_fit_app_and_model` (`tests/global_view_test.rs`, 150 cols, app
     `antigravity`, model `claude-opus-5-5`): RED: app cut to 7 cells, model to 9. Fix: content
     widths capped at name ≤ 20, app ≤ 12, model ≤ 22, status ≤ 14, `ctx` 9, tokens 6, last 10,
     place 14; optional columns are added only while the flexible task column keeps ≥ 6 cells,
     which takes the rest. GREEN.
10. **Messages one line when wide** — `src/board/global_view.rs` (`draw_messages`).
    - `messages_one_line_when_wide` (`tests/global_view_test.rs`, 140 cols, six messages): RED: the
      text was always on the next line. Fix: at `inner.width >= 100` emit
      `from → to KIND age  text` as a single clipped line; below 100 keep the two-line wrapped
      form. `MESSAGES_H` is 8 (six content rows + the two borders), so six rows show six
      messages on wide screens. GREEN.

## Gates

- `cargo test`: 584 passed, 0 failed, 0 ignored.
- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
