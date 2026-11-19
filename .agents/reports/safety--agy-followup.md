# Safety follow-up handoff

## Scope and implementation
- Branch: `safety/agy-followup`.
- Worktree: `D:/Project/agmag.wt/safety--agy-followup`.
- Base: `3302104`; initial implementation: `ddbd79c`; final reviewed implementation: **`b2ac868`**.
- Fixes the three originally reproduced handoff/cleanup paths.
- Cleanup now checks the frozen completed rotation relationship and fresh old/successor state, app, creation identity and cwd.
- Handoff requires fresh live state and matching app/address; failed/missing reads no longer use cached `done`.
- CLI batch cleanup requires one typed confirmation even when the legacy `--yes` flag is present.
- Exited-only removal remains database-only and preserves histories/handoff files.

## Collaboration and checks
- Read-only agy consultation succeeded: `ac968e24-dac9-4279-bba1-283cad292e95`; conclusions in `.agents/decisions/safety--agy-followup.md`.
- Initial regression run: 0 passed / 6 failed, demonstrating unsafe mock calls.
- Full offline suite after final fixes: **206 passed / 0 failed**.
- Strict offline Clippy: passed.
- Git whitespace check: passed.

## Opus review status
- OpenCode Anthropic API attempt failed before review: insufficient API credit; no verdict was obtained through that channel.
- Independent Claude Code review requested with explicit `claude-opus-5-5`, high effort, no tools, plan permissions, no permission prompts, and full relevant source/diff through stdin.
- Final code-gate verdict: **PASS at `b2ac868`**, no Critical/Major must-fix in supplied code.
- Final review: `.agents/reviews/safety--agy-followup-opus-2.md`.
- Opus did not independently run tests; engineering ran the reported suite. The verdict is not approval to merge or release.

## Integration constraints and limitations
- No shared-checkout edits, user-document overwrites, TUI lane modifications, merge, install, release or real terminal stopping performed.
- The active TUI lane is independent; do not blindly combine mutable checkouts. Integrate reviewed commits through the designated integration process.
- Documentation follow-up after approval: `cleanup --yes` is accepted for compatibility but cannot bypass its single typed confirmation.
- hc lacks identity-conditional kill; fresh checks reduce get/kill TOCTOU but cannot remove that race.
- No destructive real-TTY manual trial performed.

## Opus round 1 follow-up
- Opus reviewed `ddbd79c`: PASS with documented limitations, no Critical/Major must-fix in supplied code.
- Review artifact: `.agents/reviews/safety--agy-followup-opus-1.md`.
- Proactively addressed selection-time identity drift, missing fresh-finished audit entries, per-item audit errors and additional negative coverage.
- Full suite after follow-up: 206 passed, 0 failed; strict Clippy passed.
- Final follow-up review completed: Opus explicitly passed `b2ac868`.

## Remaining nonblocking work
- Human-TTY cleanup and all real successor launch variants still need controlled manual verification before release.
- Unknown creation timestamps deliberately skip live cleanup.
- DB read failures can still end a batch; multi-write bookkeeping and post-kill audit crash windows remain documented limitations.
- An item may be completed while also reporting an audit error; count categories can overlap.
- One real read-only DB/hc identity comparison (`t2s`) matched exactly; no production writes or terminal actions were performed.
