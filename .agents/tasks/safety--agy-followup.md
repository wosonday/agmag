# safety--agy-followup

## Goal
Fix the independently reproduced unsafe handoff/cleanup paths, consult agy, then obtain independent Opus review.

## Ownership
- Worktree: `D:/Project/agmag.wt/safety--agy-followup`.
- Branch/base: `safety/agy-followup` / `3302104`.
- Do not edit the shared checkout, the active TUI lane, user tutorial changes, or production state.

## Scope
- `src/actions.rs`, `src/rotation.rs`, `src/ops.rs`, `src/main.rs` (flag help only): fail-closed live checks and mandatory typed cleanup confirmation (the existing `--yes` flag remains accepted for compatibility).
- Safety regression tests (`tests/safety_test.rs`, `tests/cli_test.rs`), existing mocks necessary to provide realistic fresh hc state, and task/log/decision/review records.
- No migration, feature redesign, merge, install, release, actual kill, interrupt or permission bypass.

## Acceptance
- A failed/missing fresh status read never permits a handoff send.
- A stale/dead/unverifiable successor or unknown/mismatched old app never permits cleanup killing.
- Revalidate the frozen completed rotation relationship before cleanup.
- `cleanup --yes` cannot bypass typed confirmation for live Codex/Claude/unknown apps; retain one confirmation per batch.
- Keep exited-only cleanup non-destructive, preserve histories/handoff files, retain existing OpenCode behavior.
- Offline tests and strict Clippy pass; record agy's input and Opus's verdict faithfully.

skills: diagnosing-bugs, rust-best-practices
