# Handoff → copilot-git: `feat/sysmon` (task `sysmon`)

- **Branch:** `feat/sysmon` · worktree `D:/Project/agmag.wt/sysmon`
- **Base:** contains `main` @ `0385149`; fast-forward possible while `main` is unchanged.
- **Approved head:** the commit that adds this file (`git log -1 feat/sysmon`); reject if more commits appear.
- **depends_on:** none. `feat/sysmon-orig` (`9de7b5c`) is the pre-rebase original: do not merge; may be deleted after this lands.
- **New dependency:** `sysinfo 0.40.4` (`system`, `disk` features; +15 crates in `Cargo.lock`) — approved by the user 2026-11-20.

## What it changes
- New `agm sys` (colored MACHINE / DISKS / AGENTS report, `--json`, `NO_COLOR`), TUI header `cpu · ram · fullest disk`, new `system` view section. Live only; no schema change, nothing written to `~/.agmag`.

## Evidence
| Gate | Result |
|---|---|
| `cargo test --offline` | 308 passed / 0 failed (CEO on `003d024`, reviewer re-run) |
| `cargo clippy --offline --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Review | `.agents/reviews/sysmon-review.md` (opencode/hy4-flash-free): **ĐẠT**, 0 blocking / 1 should-fix / 6 minor. 7.1 (README timing) fixed in this commit; 2.1 and the rest deferred as follow-up |

## Post-merge
Re-run the three cargo gates on `main`. Do not remove the worktree until `agm ps` shows no live session in it.
