# CEO handoff — agmag, 2026-11-24 (CEO t80, Claude Opus 5.5)

rotation_target_pct: 50 (CEO policy, docs/CONTEXT-ROTATION.md) · observed: 50% (500k / 1M, session 662) ·
reason: long session (board, Phase 2, polish, membership fix). Reply to the user in Vietnamese.
t80 is NOT managed (`managed=0`): the user must run `agm manage t80` before `agm rotate t80`; until then this file
is the handoff. Idle successor from rotation 3: t8c `t7p-r1` (managed, done, parent t80) — user decides
(reuse or `agm cleanup`).

## State (verified 2026-11-24 ~09:45+07)
- `main` (worktree D:/Project/agmag.wt/main) HEAD b28f22f, **48 commits ahead of origin/main 75df4cd, not pushed**.
  Gates on main: cargo test 622 passed, clippy -D warnings, fmt, harness lint 36 docs 0 errors, no new CRLF.
- Shipped on main this session: TUI board (`agm board`, `cd <project>`), Phase 2 (`agm tui` full-screen ratatui app,
  `agm tui --plain` = old line UI, shared `tui_cmd::dispatch`), polish (CEO blocks always on top + `no CEO` block,
  4-line centred cards name/model/doing/‹task› with role colours, Status-only left column, `agm>` command bar on the
  board via `:`, Quota fitted + wider Terminals, `↓ N more`), membership fix (live session cwd wins over the pane's
  start cwd; removed `<repo>/.wt/<slug>` dirs resolve via nearest existing ancestor).
- Reviews: .agents/reviews/tui-board-*, tui-app-*, tui-polish-* (all final ĐẠT). Decisions: .agents/decisions/
  nvidia-log-synthesis-2026-11-24.md (NVIDIA blocked: 2 timeouts), log-synthesis-2026-11-24.md (agy synthesis +
  CEO corrections and adopted rules).
- Worktrees left: D:/Project/agmag (someone else's uncommitted delegated-approval work on feat/tui-project — never
  touch), D:/Project/agmag.wt/created-ms (fix/created-ms-identity, NOT merged), branch support/agy-review (unchecked).
- Release to try: D:/Project/agmag.wt/main/target-try/release/agm.exe (target/release/agm.exe was locked by a
  running agm; target-try is in .git/info/exclude). No workers or background jobs running.

## Waiting on the user
1. Try `agm tui` (cd agmag / spyswat, `:` command bar, Terminals width) and say if anything looks off.
2. Push main? 3. `t7p` shows in agmag only because inbox #200 is open (`done 200` hides it) — user claimed t7p
   was elsewhere; data says agmag. 4. Optional: hide projects with no live agent and no open inbox from the `cd`
   list (ghost projects from old agmag.wt/<slug> worktrees).

## Rules to keep (see .agents/decisions/log-synthesis-2026-11-24.md)
Free models first, then opencode-go/deepseek-v4.1-flash or agy gemini-4-hydrogen-low/medium (agy edits + runs cargo
in default mode); reviewer agy gemini-4-hydrogen-high (`--mode plan`, diff files in a scratch dir). Python edits:
`newline=''`. Clean up only exact files you created. Kill only a stalled worker's own PID. Never push without OK.
