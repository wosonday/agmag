# Handoff → copilot-git: `feat/opencode-go-quota` (task `opencode-go-quota`)

- **Branch:** `feat/opencode-go-quota` · worktree `D:/Project/agmag.wt/opencode-go-quota`
- **Base:** `main` @ `56c8481` (codex-quota-stale); fast-forward possible while `main` is unchanged.
- **Approved head:** the commit that adds this file (`git log -1 feat/opencode-go-quota`); reject if more commits appear.
- **depends_on:** `fix/codex-quota-stale` (merged as `56c8481`).
- Worker history was squashed by the CEO into one product commit so that an OpenCode account identifier that a worker wrote into the log never reaches `main`.

## What it changes
- `agm quota` / TUI show OpenCode Go allowance: provider `opencode-go`, windows 5h / 7d / `mo` (300 / 10080 / 43200), from `opencode api GET /api/experimental/usage/limits` (zero tokens), with a 5 s shared deadline and process-tree kill (Windows). Daemon poller `[policy].opencode_quota_poll_min` (default 15) and `agm quota --refresh`.
- `limited` flag (red) in table and TUI; stale marking when `opencode-go/*` sessions ran after the reading; provider budgets `provider:opencode-go:<window>`.

## Evidence
| Gate | Result |
|---|---|
| `cargo test --offline` | 299 passed / 0 failed (CEO, after review fixes) |
| `cargo clippy --offline --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Live (temp `AGM_HOME`) | `quota --refresh`: opencode-go 5h 100% / 7d 60% / mo 80% left, matching the endpoint |
| Review | `.agents/reviews/opencode-go-quota-review.md` (opencode/hy4-flash-free): **PASS**, no blockers; CEO fixed 2.2, 3.1, 6.1, 7.1; 2.1 recorded as a known limitation in the decision |
| Docs | README.md, README_vi.md, CHANGELOG.md, HERDR-CORE-BOT.md, `.agents/decisions/opencode-go-quota.md` |
| Schema / deps | no schema change, no new crate |
| Identifiers | no `org_…` / workspace name / usage URL in any commit of this branch |

## Post-merge
Re-run the three cargo gates on `main`. Do not remove the worktree until `agm ps` shows no live session in it.
