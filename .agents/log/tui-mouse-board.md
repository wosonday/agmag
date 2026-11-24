# tui-mouse-board log

## Step 1 - Mouse capture
- Added mouse_enabled(env_value: Option<&str>) -> bool (AGM_MOUSE=0 disables).
- nter_console enables mouse capture when enabled; leave_console disables it; panic path uses leave_console so restore is covered.
- Tests: mouse_enabled_respects_env; existing TermControl/guard board tests pass.
- Status: green
