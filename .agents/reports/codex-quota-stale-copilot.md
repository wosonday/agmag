# Handoff → copilot-git: `fix/codex-quota-stale` (task `codex-quota-stale`)

- **Branch:** `fix/codex-quota-stale` · worktree `D:/Project/agmag.wt/codex-quota-stale`
- **Base:** contains `main` @ `1465336`; fast-forward possible while `main` is unchanged.
- **Approved head:** the commit that adds this file (`git log -1 fix/codex-quota-stale`); reject if more commits appear.
- **depends_on:** none. `feat/opencode-go-quota` depends on this branch — merge this first.
- Backup of the raw worker state before cleanup: branch `fix/codex-quota-stale-wip` (do not merge; delete after this lands).

## What it changes
- Quota rows are marked **stale** when older than `[policy].quota_stale_min` (default 60) or when the same plan was used after the reading (Codex ⇐ Codex CLI + OpenCode `openai/*`; Claude ⇐ Claude Code + OpenCode `anthropic/*`). `agm quota` table/`--json` and TUI show `stale · seen <age> ago`; budgets treat stale data as unknown.
- Fresh Codex plan quota at zero tokens via `codex app-server` → `account/rateLimits/read` (`src/codex_quota.rs`), polled by the daemon (`[policy].codex_quota_poll_min`, default 15) and by `agm quota --refresh`. One shared 8 s deadline, process-tree kill on timeout.
- Fixes the reported bug: TUI showed Codex 7d 84% left (reading from 2026-11-19 06:30) while `/status` showed ~2x%; live refresh now reads 8% left.

## Evidence
| Gate | Result |
|---|---|
| `cargo test --offline` | 291 passed / 0 failed (CEO re-run on `2f3ddc5`) |
| `cargo clippy --offline --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Live (temp `AGM_HOME`) | `quota --refresh`: codex 7d used 92%, `stale: false`; `quota` without refresh on old data: `stale: true` |
| Review | `.agents/reviews/codex-quota-stale-review.md` (`7e7f8fb`, opencode/hy4-flash-free): FAIL on 2 should-fix items (table missing stale label; timeout test 0.7 s margin) → fixed in round 1 (`a131af0`, `7a7fdce`), verified by CEO |
| Docs | README.md, README_vi.md, CHANGELOG.md; `.agents/decisions/codex-quota-source.md` |
| Schema / deps | no schema change, no new crate |

## Post-merge
Re-run the three cargo gates on `main`. Do not remove the worktree until `agm ps` shows no live session in it.
