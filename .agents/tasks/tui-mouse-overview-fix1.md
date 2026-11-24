---
id: tui-mouse-overview-fix1
tier: mid
skills: test-driven-development
branch: feat/tui-mouse-overview
worktree: D:/Project/agmag.wt/tui-mouse-overview
---

# Fix round 1 for tui-mouse-overview — read .agents/reviews/tui-mouse-overview-1.md first

Line references in the review point at copies of src/board/global.rs, global_view.rs, app.rs and
tests/global_view_test.rs at 2204798 (`global.rs.txt#L660` = src/board/global.rs line 660).

Fix EVERY item of `### Bắt buộc sửa` and `### Nên sửa`, one commit per item (or per small group of "Lặt vặt"),
each with a NEW test written first and seen failing (except pure comment/doc typos):
1. Drag clears `last_click` (Down → Drag → Up → Down within 400 ms on the same border must NOT reset it).
2. `start_drag` uses exactly the same floor as `apply_height_overrides` (number of groups from
   `visible_sections(&data.view)` and the wide/narrow mode); end-to-end test with `draw_global` at 40x10 proving a
   drag neither changes the frame nor saves a layout.
3. `layout reset` closes the Output overlay like other commands and accepts extra spaces.
4. Fix `mouse_double_click_border_resets_only_that_boundary` so the `!stored` branch is really exercised.
5. Clear `drag`/`last_click` when a popup or the Output overlay owns the mouse.
6. Extra tests listed in the review (window shrink end-to-end with an override, narrow `Horizontal(1..=3)`,
   OutputTop drag ignored, anti-goal test also asserts the `GEffect` of double-click and drag sequences).
7. Lặt vặt: comment typo, one shared helper for `GEffect::SaveLayout`, and make the CHANGELOG/README text match
   the exact pre-filled reply text the code produces (check it against the `r` key).

Constraints: same as the main brief (only this worktree; no push/merge/branch switch; keep EOLs — CHANGELOG.md
CRLF; run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before EVERY commit; log under
`## Fix round 1` in .agents/log/tui-mouse-overview.md with new test names; final gates fmt/clippy/test recorded with
totals). Finish with `DONE tui-mouse-overview-fix1 <sha>` or `BLOCKED: <reason>`.
