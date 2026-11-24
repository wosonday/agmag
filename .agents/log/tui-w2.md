# Task tui-w2 log

## TDD Red → Green

1. **Global screen widths (`tests/global_view_test.rs`, `src/board/global_view.rs`)**
   - Red: added `quota_panel_fits_content_not_half` and `terminals_gets_remaining_width`; `cargo test --test global_view_test` failed (19 passed, 2 failed) because Quota took 50% (75 cols) instead of fitting content.
   - Green: added `quota_panel_width` in `src/board/global_view.rs` (widest quota row + 2 borders, Terminals gets `Min(0)`), shortened Terminals headers to `name`, `app`, `model`, `st`, `ctx`, `tok`, `last`, `task` with `column_spacing(1)` and 1-cell symbol column. `cargo test --test global_view_test` passed (21 passed, 0 failed).

2. **Command bar on the board screen (`src/board/app.rs`, `src/board/global.rs`, `src/board/global_view.rs`, `src/board/view.rs`)**
   - Red: extracted `CmdLine::on_key` (`CmdKey`) in `global.rs`, `draw_cmdline_bar` / `draw_output_overlay` / `draw_confirm_popup` / `draw_footer_bar` in `global_view.rs`, and `draw_in` in `view.rs`; added `board_in_app_shows_command_bar`, `colon_focuses_board_command_bar`, `board_command_bar_enter_runs_dispatch`, `esc_returns_to_board_keys` in `src/board/app.rs`. `cargo test --lib board::app` failed (4 passed, 4 failed).
   - Green: wired `draw_frame` and `handle_board_key` in `src/board/app.rs` to render `board::view::draw_in` into area minus 1 row, draw the shared `agm> ` command bar above the footer, focus on `:` or `Tab` after `Panel::Inbox`, edit via `CmdLine::on_key`, run `Enter` through `dispatch` + `apply_dispatched` (supporting `Note`, `Shown`, `NeedsConfirm`, `OpenBoard`, `Quit`), and unfocus on `Esc`. `cargo test --lib board::app` passed (8 passed, 0 failed).

## Global screen preview (150×40)

```text
╭ Quota ───────────────────────────────────────────────╮╭ Terminals ─────────────────────────────────────────────────────────────────────────────────╮
│opencode-go  mo █████░░░░░  49% left resets in 626h24m││    name     app    model     st      ctx       tok last     task                           │
│                                                      ││  ● worker-1 claude opus/high working ██░░  42% 12k → REPORT build the app                  │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
│                                                      ││                                                                                            │
╰──────────────────────────────────────────────────────╯╰────────────────────────────────────────────────────────────────────────────────────────────╯
╭ Inbox (0) ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│-                                                                                                                                                   │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ Messages ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│-                                                                                                                                                   │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
│                                                                                                                                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
agm>                                                                                                                                                  
 tab panels · ↑↓ select · ⏎ open · r reply · d done · s send · t theme · ? help · q quit                                                              
```
