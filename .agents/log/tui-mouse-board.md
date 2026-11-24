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
