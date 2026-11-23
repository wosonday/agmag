# CEO handoff — agmag, 2026-11-23 (agm-ceo, Claude Opus 5.5, terminal t7p)

Rotation requested by the user ("hãy rotate phiên ceo hiện tại") after a long session; telemetry for t7p shows no
context % (session not linked), so this is a phase-boundary handoff, target ≤ 50% per docs/CONTEXT-ROTATION.md.

## State
- `main` (worktree D:/Project/agmag.wt/main), NOT pushed: 752f2c9 caller fix (agm tui no longer refuses a human in a
  herdr shell pane with stale reported/app flags; t7v review ĐẠT) + 09663e4/dfb0c10 TUI `cd` project view
  (feat/tui-project, agy gemini-4-argon-high final review ĐẠT). Gates on main: cargo test 454 passed, fmt, clippy,
  97 harness tests. origin/main is at 6ab619c (v0.4.1 tag pushed).
- Checkout D:/Project/agmag is on branch feat/tui-project with SOMEONE ELSE's uncommitted "delegated approval" work
  (src/actions.rs ApprovalDecision, src/store schema v5, main.rs, config.rs, BOT-CEO.md, tests). Not ours, not t7v's.
  Do not commit/discard; user has not answered whose it is.
- Worktrees under D:/Project/agmag.wt/: main, tui-project-[a..k], fix-caller-shell, tui-review, tui-verify —
  all merged; safe to remove after the user confirms.

## Active work: TUI board (ratatui)
- User tested the text project view and wants a Toad / Lazygit-style UI. Decided: approach B (new board), entered by
  `cd <project>` inside `agm tui` and by `agm board [project]`; three columns (projects+status | orchestration tree
  with live flow edges | agent detail + its messages), inbox bottom, themes Catppuccin/Tokyo Night/Nord; no % progress.
- User then asked: if possible move the whole `agm tui` to ratatui too → recorded as Phase 2 in the spec.
- Spec: docs/superpowers/specs/2026-11-23-tui-board-design.md (DRAFT, not yet approved by the user).
- Builder check was running on opencode/iceman-free; its result was not yet folded in. Re-run it if lost.
- Next action: fold builder-check answers, get user approval of the spec, then superpowers:writing-plans with
  parallel tracks (OpenCode workers in worktrees; reviewer agy gemini-4-argon-high; helper agent t7v (agy Gemini 4
  Hydrogen) reachable via `hc send t7v '...'` for read-only reviews).

## Policies to keep
- Free models first, then cheap (opencode-go/deepseek-v4.1-flash); deepseek-v4-pro stalled on long prompts.
- Judge worker stalls by worktree file mtimes; kill only specific worker PIDs, never all opencode.exe.
- agy headless: `agy --model gemini-4-argon-high --mode plan --print="..."`, give it a diff file, no shell commands.
- Never push without the user's OK.
