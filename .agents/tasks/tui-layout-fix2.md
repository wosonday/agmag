---
id: tui-layout-fix2
tier: mid
skills: test-driven-development
branch: feat/tui-layout
worktree: D:/Project/agmag.wt/tui-layout
---

# Fix round 2 for tui-layout — read .agents/reviews/tui-layout-2.md first

0. **Integrate main first.** `git merge main` (main now has the board mouse work: src/board/app.rs, mod.rs, state.rs,
   view.rs). Resolve conflicts keeping BOTH behaviours (your hit map/layout for the overview, their mouse code for the
   board; the overview mouse is still unhandled — T3 does it). Commit the merge, run the full gates.
1. **Must fix — Messages row rects when scrolled.** In `draw_messages`, compute each message's visible rect in ONE
   coordinate system (viewport): `top_v = top.saturating_sub(scroll)`, `bottom_v = bottom.min(scroll + view) -
   scroll`, skip when `bottom <= scroll` or `top >= scroll + view`, height = bottom_v - top_v, rect inside `inner`.
   Test first: scroll = 1..=N over several wrapped messages → rects never overlap, never leave the Messages panel,
   and `hit()` at each drawn line returns the message actually drawn there.
2. **Exact wrap counts.** Replace the hand-written `wrapped_line_rows` with ratatui's own count: enable the ratatui
   feature in Cargo.toml (`ratatui = { version = "0.30", features = ["unstable-rendered-line-info"] }`, keep any
   existing features) and use `Paragraph::new(<the same Line(s) you render>).wrap(Wrap { trim: true })
   .line_count(width)` per message, so the hit map uses exactly what ratatui draws. Tests: multiple spaces across the
   wrap point, trailing space, wide (2-column) characters in a word longer than the width.
3. Should-fix leftovers from the review: do not push a panel with width or height 0 into the hit map; tests that
   dropped sections are absent from panels/rows/borders; rows never overlap.

Constraints: same as before (only this worktree; no push; keep line endings — CHANGELOG.md, src/main.rs, src/tui.rs
CRLF; Cargo.toml LF; never commit w/mixed; log under `## Fix round 2` in .agents/log/tui-layout.md; commit after
EVERY item; final gates fmt/clippy -D warnings/test clean and recorded). Finish with `DONE tui-layout-fix2 <sha>`
or `BLOCKED: <reason>`.
