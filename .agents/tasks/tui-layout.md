---
id: tui-layout
tier: mid
skills: test-driven-development
branch: feat/tui-layout
worktree: D:/Project/agmag.wt/tui-layout
---

# T1 — Overview: content-fitted heights, compact Terminals, hit map

Read the spec first: `docs/superpowers/specs/2026-11-24-tui-layout-mouse-design.md` (sections Intent, Goals,
Anti-goals, Design §1, §2, §3). You implement §1, §2 and §3 for the OVERVIEW screen only. No mouse handling, no
resize (another worker does mouse for the board; T3 comes later and will use your hit map).

Code: src/board/global_view.rs (`draw_global`, `split_messages`, `draw_sections`, `draw_stacked`,
`draw_terminals`, `MESSAGES_H`), src/board/global.rs (`GlobalState`, `GlobalData`, `visible_sections`).
Existing tests for the overview render live in tests/ (grep for `draw_global` / `TestBackend`) — follow them.

## Steps (TDD: red → code → green → commit, one commit per step)
1. Pure `fit_heights(desired: &[u16], flex: usize, total: u16, min: u16) -> Vec<u16>` (+ unit tests: surplus goes
   to `flex`; deficit shrinks largest first, never below `min`; sum == total when total ≥ count*min; empty input).
2. Each section's desired rows (Quota lines+2, Terminals header+rows+2, Inbox items+2 with min 3 when empty,
   Messages flexible, min 4; Usage/Budgets/System: their rendered line count + 2). Use them in `draw_sections` for
   both the ≥120-column layout (top pair height = max of the pair's desired, capped at 60 % of the area) and the
   stacked layout. Messages is the flexible section (no longer fixed 8). Render tests at 140×40 and 200×50 with
   ~11 terminals and 0 inbox: no section has more than 1 blank inner row beyond its content except the flexible
   one; Inbox (0) is 3 rows high.
3. Compact Terminals (spec §2): hide all-`-` columns; agents first, plain shells (app in {pwsh, powershell, bash,
   sh, zsh, cmd, fish} and no session/model) last, dimmed with `theme.muted`; title `Terminals (N agents · M
   shells)` (omit zero parts). Selection by address unchanged; ↑↓ follows the new visual order — check
   `GlobalState` navigation uses the same order (put the ordering in ONE function used by both draw and navigation).
4. Hit map (spec §3): a small type (e.g. `HitMap` in src/board/global.rs or a new src/board/hit.rs) with panel
   rects, row rects (terminal address / inbox id) and boundary rects (`BorderId`: top-pair vertical divider; each
   horizontal boundary between stacked sections). `draw_global` fills it every frame; store it so the app loop can
   read it after drawing (e.g. a `RefCell`/`Cell` field in `GlobalState` or return it from `draw_global` — your
   call, keep the call sites in src/board/app.rs compiling). Pure `hit(&HitMap, x, y) -> Option<Hit>` + unit tests.
   No mouse events are handled in this task.
5. CHANGELOG.md `[Unreleased]` (create the section at the top if missing, above `## [0.5.0]`) → `### Thay đổi`:
   one Vietnamese bullet for the content-fitted overview and compact Terminals.

## Constraints
- Only this worktree/branch; no push/merge/branch switch; do not edit src/board/mod.rs, state.rs, view.rs (T2 owns
  them).
- Keep line endings (CHANGELOG.md, src/main.rs, src/tui.rs are CRLF; src/board/* LF). `git ls-files --eol <file>`
  before each commit; never commit `w/mixed`.
- Log each step in `.agents/log/tui-layout.md`; commit after EVERY step.
- Final gates clean and recorded: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Finish with `DONE tui-layout <HEAD sha>` or `BLOCKED: <reason>`.
