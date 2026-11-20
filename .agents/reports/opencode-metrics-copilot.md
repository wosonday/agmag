# Handoff → copilot-git: `feat/opencode-metrics` (task `opencode-metrics`)

- **Branch:** `feat/opencode-metrics` · worktree `D:/Project/agmag.wt/opencode-metrics`
- **Base:** already contains `main` @ `18079dc` (spawn-fixes); fast-forward possible while `main` is unchanged.
- **Approved head:** the commit that adds this file (`git log -1 feat/opencode-metrics`); reject if more commits appear.
- **depends_on:** `fix/spawn-argv-template` (merged as `18079dc`) — rotation must verify a successor before killing, because this branch makes `ctx_rotate` fire for OpenCode workers.

## What it changes
- `ctx %` for OpenCode sessions: context window from `providers.toml` (full key, then bare id), else the OpenCode catalog (`opencode api GET /api/model`, loaded once per process, hard 5 s deadline incl. process-tree kill), else none. `ctx_warn` / `ctx_rotate` now work for OpenCode workers.
- No false `unknown format?` warning for zero-token OpenCode sessions (Codex/Claude/agy unchanged).
- `.agents/decisions/opencode-go-quota.md`: Go 5h/weekly/monthly allowance is readable at zero tokens via `GET /api/experimental/usage/limits` — survey only, not installed.

## Evidence
| Gate | Result |
|---|---|
| `cargo test --offline` | 280 passed / 0 failed (CEO + reviewer, after merging main) |
| `cargo clippy --offline --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Live | `agm ls --json`: 70/89 OpenCode sessions have `context_pct` (was 1 model only) |
| Review | `.agents/reviews/opencode-metrics-review.md`: first pass **FAIL** (`f4ed2fe`, unbounded cleanup could hang the daemon); round-1 fix `85f0bc0`; re-review **PASS** (`ee468b4`, hang repro now returns in 5.4 s; old code still hangs >12 s) |
| Docs | README.md, README_vi.md, CHANGELOG.md. No Hydrogen Medium pass (agy quota exhausted). |
| Schema / deps | no schema change, no new crate |

## Post-merge
Re-run the three cargo gates on `main`. Do not remove the worktree until `agm ps` shows no live session in it.
