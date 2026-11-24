---
id: tui-mouse-board-fix1
tier: mid
skills: test-driven-development
branch: feat/tui-mouse-board
worktree: D:/Project/agmag.wt/tui-mouse-board
---

# Fix round 1 for tui-mouse-board — read .agents/reviews/tui-mouse-board-1.md first

Every "Bắt buộc sửa" and "Nên sửa" item there is accepted by the CEO; fix ALL of them. TDD per item: failing test
first (run it, see it fail), fix, green, commit (`fix(board-mouse): …`, one commit per item).

Must fix:
1. Overlays are modal for the mouse. While `Mode::PickProject` is open, only the picker's rows and the picker panel
   are hit-testable: clicks/scroll/double-clicks outside the picker rect do nothing; background cards/inbox rows/
   panels are never hit. Same for the Output overlay on the board and for any other popup drawn over the board.
   Implement it in the hit map (when an overlay is drawn, it replaces/clips the background entries) so `hit()` can
   never return a hidden target. Test with the REAL hit map produced by `view::draw_in` (TestBackend), not a
   hand-built one: double-click on the position of a background inbox row while the picker is open → mode stays
   PickProject, no Reply.
2. No redraw for mouse events that change nothing: `on_mouse` returns whether state changed (or an enum); the
   loops in src/board/mod.rs and src/board/app.rs redraw only when it did. `Moved`, `Up`, `Drag`, right/middle
   buttons, events on the overview screen (until T3), and events ignored because a popup/command bar is focused
   → no redraw. Test the return value.

Should fix:
3. Safety tests: run each MouseEventKind from a FRESH `Mode::Normal` state (not one shared state that switches to
   Reply on the first event); cover modes Reply/Send/ShowItem/Filter/Help/Confirm (all ignore the mouse);
   double-click edges: > 400 ms, two different targets within 400 ms, Down/Up/Down (Windows release in between),
   triple click.
4. Tests on the real hit map from `view::draw_in` for: scrolled inbox, scrolled narrow orchestration, picker popup;
   unit tests for `app::handle_board_mouse` (blocked when popup is open / board command bar focused / output open)
   and `BoardScreen::on_mouse` (selecting another agent reloads the conversation).
5. `handle_board_mouse`: ignore the mouse while `data.output.is_some()` (covered by item 1 if Output is modal).
6. Help popup: the mouse line fits without wrapping (shorten it or widen the popup).

Constraints: same as .agents/tasks/tui-mouse-board.md (only this worktree, no push/merge; do not edit
src/board/global_view.rs or src/board/global.rs; keep line endings; log under `## Fix round 1` in
.agents/log/tui-mouse-board.md; commit after EVERY item; final gates fmt/clippy -D warnings/test clean and
recorded). Finish with `DONE tui-mouse-board-fix1 <HEAD sha>` or `BLOCKED: <reason>`.
