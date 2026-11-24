---
id: copilot-merge-2026-11-24
role: copilot-git
checkout: D:/Project/agmag.wt/main (branch main, HEAD c75ba9c)
---

# Integrate remaining branches into main (user will push; you never push/tag)

Follow docs/COPILOT-GIT.md. Main checkout must be clean before each merge.

Already merged (verify with `git branch --merged main`, no action): feat/approval-runtime, fix/created-ms-identity,
feat/interactive-ctx, feat/hide-idle-projects, feat/tui-layout, feat/tui-mouse-board, feat/tui-mouse-overview.
Never merge `attic/*` (rejected work).

Evaluate, one at a time:
1. `feat/delegated-approval` (08fc676, 2 commits: ab9fe0a delegated approval grants; 08fc676 docs ctx 40/50).
   Believed SUPERSEDED by feat/approval-runtime (already in main). Check with `git cherry -v main
   feat/delegated-approval` and by comparing content (schema v5, `agm approval`, docs/CONTEXT-ROTATION.md 40/50)
   in main. If everything is already in main → do NOT merge; report "superseded" with evidence. If something is
   missing → merge with `--no-ff`; on any conflict STOP (`git merge --abort`) and report.
2. `support/agy-review` (f35045a, 4 commits: english-help translation of CLI help/tutorials + agy-support records).
   Check whether it is already in main (`git cherry`, compare files). If not, try `git merge --no-ff`; on conflict
   `git merge --abort` and report the conflicting files — never edit feature code to resolve.
After each merge: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` with env
`CARGO_BUILD_JOBS=4`; if anything fails, `git revert -m 1 <merge>` and report.

Forbidden: git push, git tag, git reset, force operations, deleting worktrees/branches/files, editing src/tests.
Keep EOLs (CHANGELOG.md, src/main.rs, src/tui.rs are CRLF).
Write the result to .agents/reports/copilot-merge-2026-11-24.md (per branch: merged <sha> | superseded | conflict
<files> | failed <gate>; gate totals; final `git log --oneline -5`; list of merged worktrees that could be cleaned
up later) and commit it. End with `DONE copilot-merge <HEAD sha>` or `BLOCKED: <reason>`.
