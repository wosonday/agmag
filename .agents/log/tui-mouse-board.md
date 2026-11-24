# tui-mouse-board log

## Step 1 - Mouse capture
- Added mouse_enabled(env_value: Option<&str>) -> bool (AGM_MOUSE=0 disables).
- nter_console enables mouse capture when enabled; leave_console disables it; panic path uses leave_console so restore is covered.
- Tests: mouse_enabled_respects_env; existing TermControl/guard board tests pass.
- Status: green

## Step 2 - Board hit map
- Added BoardHitMap, HitTarget, and pure hit(map, x, y) in src/board/view.rs.
- draw/draw_in now return hit maps; board draw records panel rects (Orchestration/Detail/Inbox, picker popup as Projects), agent cards/rows, inbox rows, and picker rows.
- BoardScreen stores hit_map after each draw; app board draw stores it too.
- Tests: hit_prefers_specific_targets_over_panels, hit_finds_project_and_picker_rows.
- Status: green

## Step 3 - Board mouse handling in state
- Added BoardState::on_mouse (pure, synthetic MouseEvent tested) handling panel focus, row/card selection, scroll, double-click open/reply prefill.
- Ignores mouse in popup/confirm/prompt modes; only Down/Drag/Up(Left) and Scroll acted on.
- Added last_click for 400ms double-click; picker rows prioritized in hit testing.
- Tests: mouse focus/select, scroll, double-click project/picker/inbox reply prefill, ignored popup/non-left.
- Status: green

## Step 4 - Wire mouse events
- Wired Event::Mouse in standalone board loop (src/board/mod.rs) and app board screen (src/board/app.rs); overview ignores mouse for now.
- Added BoardScreen::on_mouse and handle_board_mouse; mouse ignored when global popup/cmd focus active.
- Anti-goal test: mouse_cannot_reach_destructive_or_send_actions proves no mouse sequence produces kill/interrupt/cleanup/done/send/reply-send effects.
- Status: green

## Step 5 - Docs
- Updated board ? help to mention mouse (click/wheel/double-click, Shift+drag text selection, AGM_MOUSE=0).
- Added short TUI mouse paragraph to README.md and README_vi.md.
- Added CHANGELOG.md [Unreleased] ### Thêm mới bullet for board mouse capture.
- Status: green

## Final gates
- cargo fmt --check: pass (after fmt)
- cargo clippy --all-targets -- -D warnings: pass
- cargo test: pass (all suites ok, 0 failed)
- Fixed test call sites in tests/board_view_test.rs for new draw/draw_in return type.
- Status: green

## Fix round 1

### Item 1 - Overlays are modal for the mouse
- Red: new integration test `picker_popup_makes_the_board_behind_it_unreachable` first failed to compile
  (`BoardHitMap` had no `overlay`) and, once the field existed, `hit()` still returned the background
  inbox row / card behind the picker.
- `BoardHitMap` gained `overlay: Option<Rect>` + `begin_overlay(rect)` (clears background entries, marks the
  rect). `hit()` returns `None` outside the overlay, so a hidden target can never win.
- `view::draw_popup` calls `begin_overlay` for every popup mode; the picker then records its own panel/rows.
- The Output panel is modal too: `app::draw_frame` calls `hit_map.begin_overlay(main_area)` after drawing it
  (unit test `output_overlay_hides_the_board_from_mouse`).
- Test drives the REAL map from `view::draw_in` (TestBackend 140x40): double-click on a background inbox row
  while PickProject is open leaves mode PickProject, no Reply, no selection/focus change.
- Status: green

### Item 2 - Redraw only when a mouse event changed something
- `BoardState::on_mouse` now returns `MouseOutcome { effect, changed }`; scroll/click handlers report whether
  visible state (focus, selection, mode, msg_scroll, project) actually moved. Ignored kinds (`Moved`, `Up`,
  `Drag`, right/middle), clicks outside targets, clicks on the already-selected target and scrolls at an end of
  a list report `changed == false`.
- `BoardScreen::on_mouse` returns `bool` (redraw) and still reloads the conversation when the agent changes.
- Standalone loop (`mod.rs`) draws only when `on_mouse` says so; app loop got `Next::Skip`; the overview now
  routes through `handle_mouse` → `Skip` (T3 owns it); `handle_board_mouse` maps "no change" to `Skip`.
- Red: existing call sites/test assertions failed to compile against the new return; new test
  `mouse_reports_whether_it_changed_anything` (plus `board_screen_mouse_only_redraws_on_change` and
  `mouse_events_that_change_nothing_skip_the_redraw`) pins the skipped events.
- Status: green
