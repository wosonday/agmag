# safety--agy-followup log

## 2026-11-19 · scope and baseline
- User requested checking/discussing with agy, correcting issues, and reporting to Opus.
- Prior independent mock tests reproduced all three safety failures (see `D:/Project/agmag.wt/english-help/.agents/reviews/agy-support-1.md`).
- Current base `3302104` still contains those paths.
- Old agy terminal `t34` is finished; new `t38` is working on an isolated TUI lane. No input injected into busy/blocked terminals.
- Created a separate safety worktree/branch, avoiding user changes in the main checkout.
- Feedback loop: `cargo test --offline --test safety_test` (regressions copied/adapted from the earlier isolated reproducer).
- Ranked causes already demonstrated by mocks: (1) cleanup trusts cached successor state, (2) failed old-app reads fall through to kill, (3) failed fresh-status reads fall back to cached done and unguarded send.
- Planned verification: red tests, narrow fixes, full tests + Clippy, agy consultation, independent Opus review.

## 2026-11-19 · red/green and consultation
- Added `tests/safety_test.rs`; initial run: 0 passed, 6 failed, with concrete unsafe mock sends/kills.
- Received agy's read-only consultation (`ac968e24-dac9-4279-bba1-283cad292e95`, SUCCESS); recommendations/decisions recorded in `.agents/decisions/safety--agy-followup.md`.
- Implemented fail-closed fresh handoff state, frozen rotation relationship validation, old/successor app + creation + cwd checks, explicit finished-only DB cleanup, mandatory typed CLI batch confirmation.
- Preserved recorded successor addresses even when the successor row is absent, so exited-only cleanup remains available.
- Updated only affected fixtures to provide explicit real-shaped hc responses; did not weaken production checks to satisfy missing-status mocks.
- Targeted run: `cargo test --offline --test safety_test --test actions_test --test rotation_test` → 41 passed, 0 failed.
- Files changed so far: `src/actions.rs`, `src/rotation.rs`, `src/ops.rs`, `src/main.rs` (help), `tests/safety_test.rs`, `tests/common/mod.rs`, `tests/actions_test.rs`, `tests/rotation_test.rs`, task/log/decision files for this task.

## 2026-11-19 · audit before Opus
- Expanded regression coverage to 9 safety tests plus CLI flag-help coverage (10 new tests total).
- `cargo test --offline` → 202 passed, 0 failed.
- `cargo clippy --offline --all-targets -- -D warnings` → passed, no warnings.
- `git diff --check` → passed (Windows line-ending notices only).
- Additional changed file: `tests/cli_test.rs`.
- Remaining risk: hc has no identity-conditional kill; get/kill races are reduced by immediate revalidation, not eliminated.
- No real TTY destructive cleanup trial performed; all destructive calls in tests use mocks.
- Ready for independent Opus inspection of the committed diff; no merge/release/install performed.

## 2026-11-19 · Reviewer dispatch
- Committed implementation and red/green evidence as `ddbd79c`.
- Attempted independent `anthropic/claude-opus-5-5#high` subagent review; API returned insufficient credit before reviewing. No pass claimed.
- Started fallback independent Claude Code Opus (`claude-opus-5-5`, high effort), no tools, safe mode, plan permission mode, permission prompts disabled; source/diff supplied through stdin outside the repo.
- Pending review tracked in `.agents/reports/safety--agy-followup.md`; no source changed after the reviewed implementation commit.
- Additional file for handoff: `.agents/reports/safety--agy-followup.md`.
