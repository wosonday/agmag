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
- Builder check done (iceman-free); 3 answers folded into the spec as my calls (pair/agent message queries, single-item done, stage mapping).
- Next action: fold builder-check answers, get user approval of the spec, then superpowers:writing-plans with
  parallel tracks (OpenCode workers in worktrees; reviewer agy gemini-4-argon-high; helper agent t7v (agy Gemini 4
  Hydrogen) reachable via `hc send t7v '...'` for read-only reviews).

## Policies to keep
- Free models first, then cheap (opencode-go/deepseek-v4.1-flash); deepseek-v4-pro stalled on long prompts.
- Judge worker stalls by worktree file mtimes; kill only specific worker PIDs, never all opencode.exe.
- agy headless: `agy --model gemini-4-argon-high --mode plan --print="..."`, give it a diff file, no shell commands.
- Never push without the user's OK.

## Update 2026-11-24 (CEO session after rotation 3)
- TUI board DONE and merged to local `main` at 74788b4 (not pushed; origin/main still 6ab619c, 79 commits ahead).
  Plan docs/superpowers/plans/2026-11-23-tui-board.md; Task 0 scaffold by CEO, tracks a–d + c2 polish + fix1 by OpenCode
  workers (a hy4-flash-free, b iceman-free, c/d/c2/fix1 opencode-go/deepseek-v4.1-flash; deepseek-v4.1-flash-free and
  mimo-v2.6-flash-free hit the free limit). Reviews .agents/reviews/tui-board-{1,2}.md (agy gemini-4-hydrogen-high:
  round 1 CHƯA ĐẠT, 5 items fixed; round 2 ĐẠT). Gates on main: 521 tests, clippy, fmt, harness lint 34 docs 0 errors.
- Not verified in a real terminal yet (user said "tiếp tục" without reporting a run). Ask for feedback on `agm board`
  and `cd <project>` → `q` back to `agm>` (stdin hand-shake) on Windows Terminal + herdr pane.
- Known gaps: `task:` in agent detail is always `-` (AgentView has no task); edge labels of children of two different
  parents on one row can overdraw the neighbour's drop (round-2 Nên sửa #2). Phase 2 (whole `agm tui` on ratatui,
  `--plain`) needs its own short design + review round.
- Pending user decisions: push main; remove merged worktrees (tui-project-a..k, fix-caller-shell, tui-review,
  tui-verify, tui-board, tui-board-a..d, -c2, -fix1); owner of the uncommitted delegated-approval work in D:/Project/agmag;
  `cargo install --path D:\Project\agmag.wt\main`.

## Update 2026-11-24 later — main pushed, Phase 2 in progress
- main pushed (origin/main 75df4cd). Merged worktrees removed; kept: D:/Project/agmag (someone's uncommitted work),
  created-ms (fix/created-ms-identity, NOT merged), branch support/agy-review (unchecked).
- Phase 2 (whole `agm tui` on ratatui, approach 3 hybrid) approved: spec docs/superpowers/specs/2026-11-24-tui-phase2-design.md,
  plan docs/superpowers/plans/2026-11-24-tui-phase2.md. Branch feat/tui-app (worktree D:/Project/agmag.wt/tui-app):
  tracks a–e merged + Task F wiring (d8714df: `agm tui` = app, `agm tui --plain` = line UI). 571 tests, clippy, fmt, harness OK.
- Review round 1 (.agents/reviews/tui-app-1.md, agy gemini-4-hydrogen-high): CHƯA ĐẠT, 4 must + 4 should; fix worker
  running in D:/Project/agmag.wt/tui-app-fix1 (deepseek-v4.1-flash) with 2 extra CEO items (Terminals column widths,
  one-line messages when wide). Next: merge fix1 → review round 2 → CHANGELOG → merge to local main → ask user to try
  `agm tui` and before push. Preview tool: copy ~/.agmag/agm.db + providers.toml to scratchpad and render draw_global
  with TestBackend (temporary test file, never committed).
