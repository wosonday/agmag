---
id: tui-mouse-board
tier: mid
skills: test-driven-development
branch: feat/tui-mouse-board
worktree: D:/Project/agmag.wt/tui-mouse-board
---

# T2 — Mouse capture + mouse on the project board

Read the spec first: `docs/superpowers/specs/2026-11-24-tui-layout-mouse-design.md` (Intent, Goals, **Anti-goals**,
Design §3 and §5). You implement mouse capture for the whole app and the **board** part of §5 (+ a hit map for the
board). The overview screen's mouse handling is NOT yours (T3 later); do not edit src/board/global_view.rs or
src/board/global.rs (T1 owns them).

Code: src/board/mod.rs (`enter_console`, `leave_console`, `TermGuard`, panic hook ~line 521, the board event loop
~line 447 `event::read`), src/board/state.rs (`BoardState`, `Panel`, `Mode`, key handling), src/board/view.rs
(drawing columns, cards, inbox, picker), src/board/app.rs (the app loop ~line 147: route `Event::Mouse` to the board
when `Screen::Board`, ignore it on the overview for now).

## Steps (TDD: red → code → green → commit, one commit per step)
1. Capture: `EnableMouseCapture` in `enter_console`, `DisableMouseCapture` in `leave_console` and on the panic path;
   skip both when env `AGM_MOUSE=0`. Put the env decision in a pure fn (`mouse_enabled(env_value: Option<&str>)`)
   with tests. Make sure the existing `TermControl` test doubles still pass.
2. Board hit map: view.rs records rects of each panel (`Panel::Projects/Orchestration/Detail/Inbox`), each project
   row, each agent card, each inbox row, and the project picker rows; stored where the event loop can read it after
   drawing. Pure `hit(map, x, y)` + unit tests.
3. Mouse handling in board state (pure, unit-tested with synthetic `MouseEvent`s): `Down(Left)` on a panel → focus
   it; on a project row / agent card / inbox row → select it; `ScrollUp/Down` → move the selection of the panel
   under the pointer (or scroll the conversation in Detail); double-click (two `Down(Left)` on the same target
   within 400 ms; pass the clock in) on a project row or picker row → open it (same as ⏎); on an inbox row →
   pre-fill the reply command exactly like the `r` key does (nothing is sent). Ignore all mouse events while any
   popup/confirm/prompt mode is open. Act only on Down/Drag/Up(Left) and Scroll; ignore others.
4. Wire `Event::Mouse` into the board loop (src/board/mod.rs) and the app's board screen (src/board/app.rs).
   Anti-goal check: add a test proving no mouse sequence can reach kill/interrupt/cleanup/done/send/reply-send.
5. Docs: `?` help text on the board mentions mouse (click/wheel/double-click, Shift+drag to select text,
   `AGM_MOUSE=0`); README.md and README_vi.md TUI section one short paragraph; CHANGELOG.md `[Unreleased]`
   (create it at the top above `## [0.5.0]` if missing) → `### Thêm mới` one Vietnamese bullet.

## Constraints
- Only this worktree/branch; no push/merge/branch switch.
- Keep line endings (CHANGELOG.md, src/main.rs, src/tui.rs CRLF; src/board/* LF). `git ls-files --eol <file>` before
  each commit; never commit `w/mixed`.
- Log each step in `.agents/log/tui-mouse-board.md`; commit after EVERY step.
- Final gates clean and recorded: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Finish with `DONE tui-mouse-board <HEAD sha>` or `BLOCKED: <reason>`.
