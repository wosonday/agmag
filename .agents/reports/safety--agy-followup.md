# Safety follow-up handoff

## Scope and implementation
- Branch: `safety/agy-followup`.
- Worktree: `D:/Project/agmag.wt/safety--agy-followup`.
- Base: `3302104`; implementation: `ddbd79c`.
- Fixes the three originally reproduced handoff/cleanup paths.
- Cleanup now checks the frozen completed rotation relationship and fresh old/successor state, app, creation identity and cwd.
- Handoff requires fresh live state and matching app/address; failed/missing reads no longer use cached `done`.
- CLI batch cleanup requires one typed confirmation even when the legacy `--yes` flag is present.
- Exited-only removal remains database-only and preserves histories/handoff files.

## Collaboration and checks
- Read-only agy consultation succeeded: `ac968e24-dac9-4279-bba1-283cad292e95`; conclusions in `.agents/decisions/safety--agy-followup.md`.
- Initial regression run: 0 passed / 6 failed, demonstrating unsafe mock calls.
- Full offline suite after fixes: 202 passed / 0 failed.
- Strict offline Clippy: passed.
- Git whitespace check: passed.

## Opus review status
- OpenCode Anthropic API attempt failed before review: insufficient API credit; no verdict was obtained through that channel.
- Independent Claude Code review requested with explicit `claude-opus-5-5`, high effort, no tools, plan permissions, no permission prompts, and full relevant source/diff through stdin.
- Verdict: pending. Do not treat this report as Reviewer approval.

## Integration constraints and limitations
- No shared-checkout edits, user-document overwrites, TUI lane modifications, merge, install, release or real terminal stopping performed.
- The active TUI lane is independent; do not blindly combine mutable checkouts. Integrate reviewed commits through the designated integration process.
- Documentation follow-up after approval: `cleanup --yes` is accepted for compatibility but cannot bypass its single typed confirmation.
- hc lacks identity-conditional kill; fresh checks reduce get/kill TOCTOU but cannot remove that race.
- No destructive real-TTY manual trial performed.
