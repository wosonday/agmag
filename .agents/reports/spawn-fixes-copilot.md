# Handoff → copilot-git: `fix/spawn-argv-template` (task `spawn-fixes`)

- **Branch:** `fix/spawn-argv-template` · worktree `D:/Project/agmag.wt/spawn-fixes`
- **Base:** `main` @ `a7ce50f07153085043c5349b98293f3f7c7c43a6` (merge-base; fast-forward possible while `main` is unchanged)
- **Approved head:** the commit that adds this file (see `git log -1 fix/spawn-argv-template`); reject if more commits appear.
- **depends_on:** none.

## What it changes
- `agm spawn` keeps program argv intact; joins the task text only for hc templates.
- Windows fallback when `hc run <template>` fails with `sh: not found` (hc 0.1.21 bug): read template, run `command:` directly, deliver body + task, inbox on timeout.
- Rotation verifies the successor can start **before** killing the old OpenCode worker (on Windows the template file must also load).
- **Also contains a whole-tree `cargo fmt`** (main had 702 rustfmt diffs). Real change vs. formatted main: `src/actions.rs`, `src/hc.rs`, `src/main.rs`, `src/rotation.rs`, tests, docs. Branches merged after this one may see formatting conflicts; resolve by `cargo fmt` after rebase, never by hand-editing feature logic.

## Evidence
| Gate | Result |
|---|---|
| `cargo test --offline` | 271 passed / 0 failed (CEO re-run on head `920422a`) |
| `cargo clippy --offline --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Live check on Windows | argv `['-c', '--prompt', 'a b']`; template `auditor` opened and received its brief (see `.agents/log/spawn-fixes.md`) |
| Review | `.agents/reviews/spawn-fixes-review.md` (`b54f041`, opencode/hy4-flash-free) — **PASS**; finding #1 raised to mandatory by CEO and fixed in round 1 (`920422a`, 5-line diff in `src/actions.rs` + 3 rotation tests), verified by CEO |
| Docs | README.md, README_vi.md, CHANGELOG.md, HERDR-CORE-BOT.md §1.4 updated by the worker; `.agents/decisions/hc-template-sh-windows.md` drafts the upstream herdr-core report. No Hydrogen Medium pass (agy quota exhausted). |
| Schema / deps | no `src/store` schema change, no new crate |

## Post-merge
Re-run the three cargo gates on `main`; reinstall with `cargo install --path .` only if the user asks. Do not remove the worktree until `agm ps` shows no live session in it.
