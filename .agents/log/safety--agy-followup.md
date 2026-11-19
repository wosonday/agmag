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

## 2026-11-19 · Opus round 1 and hardening
- Claude Code Opus review succeeded (session `1988cff7-0db1-4628-8b17-4f4442daf5ef`): PASS with stated limitations, no Critical/Major must-fix in supplied excerpts.
- Recorded its findings in `.agents/reviews/safety--agy-followup-opus-1.md` without treating them as release approval.
- Added a new regression for DB+hc identity drift after selection; it failed before the follow-up fix (`get t2`, `get t1`, `kill t1`).
- Froze old/successor creation timestamps and linked session row IDs in `CleanupCandidate`, checking those before cleanup. These are additive JSON metadata fields; no database migration.
- Added cleanup audit entries for freshly finished old terminals and per-item error reporting for audit/bookkeeping failures; later batch items continue.
- Added negative cwd/app/unknown-creation tests, nonmutation assertions, audit-failure injection, and CLI `cleanup --all --yes` non-TTY cancellation coverage.
- `cargo test --offline` → 206 passed, 0 failed.
- `cargo clippy --offline --all-targets -- -D warnings` → passed.
- Non-destructive real schema observation via `hc get`: `t2s` returned top-level live and created_ms=1795003409789; finished `t34` returned finished and retained created_ms=1795007307899. No terminal actions performed.
- Confirmed source `Store::finish_terminal` writes `finished`; `upsert_terminal` preserves nonzero creation timestamps. Legacy unknown timestamps still fail closed.
- Follow-up files: `src/actions.rs`, `tests/safety_test.rs`, `tests/cli_test.rs`, this log and Opus review artifact. Request final Opus review of the follow-up SHA before declaring the gate passed.

## 2026-11-19 · final Opus gate and handoff
- Committed follow-ups as `b2ac868`; no product code changed afterward.
- Opus final review returned **PASS for b2ac868**, with no Critical/Major must-fix in supplied source. Reviewer did not independently execute tests.
- Preserved review scope and nonblocking limitations in `.agents/reviews/safety--agy-followup-opus-2.md`; updated `.agents/reports/safety--agy-followup.md`.
- Per Opus suggestion, opened production SQLite with `mode=ro`, querying only `terminals` for `t2s`: stored created_ms 1795003409789 matches hc. No data write, agent stop or permission bypass.
- This observation covers one real terminal, not all launch variants. Real-TTY destructive cleanup remains untested.
- Attempt to notify active agy `t38` was guarded and sent no input because it was working. Design discussion did complete through the separate read-only agy consultation recorded above.
- Final record-only changed files: this log, final report and Opus round-2 artifact.
- No merge, install, release, shared-checkout edit or TUI-lane modification.
