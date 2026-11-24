---
id: tui-layout-fix1
tier: mid
skills: test-driven-development
branch: feat/tui-layout
worktree: D:/Project/agmag.wt/tui-layout
---

# Fix round 1 for tui-layout — read .agents/reviews/tui-layout-1.md first

Every "Bắt buộc sửa" and "Nên sửa" item there is accepted by the CEO; fix ALL of them. TDD per item: write the
failing test first (run it, see it fail), fix, green, commit (`fix(tui-layout): …`, one commit per item).

Must fix:
1. `section_layout`: the top-pair floor/cap must use MIN_SECTION_H (3), not 2*MIN_SECTION_H; the 60 % cap must not be
   raised above what the area allows. Test: 140x40 with 1 quota line + 1 terminal → no fixed panel has > 1 blank
   inner row; main.height = 8 does not squeeze the rest.
2. `split_messages` / layout: every rect is clamped inside `main` — Messages never overlaps the command line or
   footer, at any size. Test sizes 100x1 … 100x8 and 140x3 … 140x10: no two drawn panels overlap, no hit-map rect
   leaves `main`, nothing panics.
3. `draw_stacked`: when the area cannot fit every section at the minimum, drop (do not draw, do not record in the
   hit map) the sections that do not fit — never draw overlapping panels. Same test matrix as 2.

Should fix:
4. Messages row rects in the hit map follow the real wrapped height of each message (compute wrapped line counts
   with the same width/wrap rule the Paragraph uses), not a fixed 2 lines. Test with a long message at width 100.
5. When the Output panel is open, the hit map must not expose panels/rows/borders hidden under it: record the
   Output panel itself (add a `Section`/target for it, e.g. `HitPanel::Output`) and remove or clip hidden entries.
6. `section_desired_rows`: empty Terminals = 3 rows (border + one `-` line); for Usage/Budgets/System in the top
   pair use the real column width, not `main.width`.
7. Tests: make the border-vs-row priority test actually overlap; add the tiny-height tests (2, 3) and a scrolled
   Terminals/Inbox table test where the selected row is below the first page — the row rects must match the rows
   actually drawn.

Constraints: same as .agents/tasks/tui-layout.md (only this worktree, no push/merge; keep line endings; log under
`## Fix round 1` in .agents/log/tui-layout.md; commit after EVERY item; final gates fmt/clippy -D warnings/test
clean and recorded). Finish with `DONE tui-layout-fix1 <HEAD sha>` or `BLOCKED: <reason>`.
