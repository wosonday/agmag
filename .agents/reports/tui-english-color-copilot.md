# Copilot integration handoff — tui-english-color

## Status
**TUI code gate PASS at `bf01c9a`. Ready for designated Copilot handoff, subject to the dependency and authorization rules below.** This is not release approval.

## Candidate and correction
- Original candidate: `7e20645d4b8e1c4e2c1238c4d3f8d0f2be753e32`.
- Original worktree: `D:/Project/agmag/.wt/tui--english-color` (untouched).
- Original engineering audit: 194 tests passed, strict Clippy passed, but three additional width regressions failed.
- Corrected product SHA: `bf01c9a` on `review/tui-english-color` (includes follow-ups to Opus review of `5d42e71`).
- Corrected worktree: `D:/Project/agmag.wt/review--tui-english-color`.
- Current code includes the original TUI branch history; do not merge the unfixed original as the final candidate.
- Source review and red/green evidence: `.agents/reviews/tui-english-color-engineering.md`.
- Final independent Opus verdict: `.agents/reviews/tui-english-color-opus-2.md` (PASS, no Critical/Major must-fix in supplied code).

## Audit after corrections
- `cargo test --offline --locked`: **202 passed, 0 failed**.
- `cargo clippy --offline --all-targets -- -D warnings`: **passed**.
- English chrome, Vietnamese message-code compatibility/user content, sanitization, widths 60/80/110/200, CJK/emoji/graphemes, long paths/prefixes, frame notes and fail-closed TTY/confirmation inspected.
- Reused only cached locked Unicode packages; no package downloads.
- Both READMEs updated; no real interactive/destructive TTY trial.

## Dependency and merge boundaries
- Cleanup safety is a separate reviewed branch: `safety/agy-followup`, product SHA `b2ac868`, evidence head `16d87df`.
- That branch's patch passes `git apply --check` against the corrected TUI tree. This checks textual compatibility only, not combined runtime behavior.
- The TUI gate is not a global cleanup-safety gate. Integrate the reviewed safety fix before treating the combined application as release-ready; repeat all tests on the combined tree.
- No merge, installation, deployment or push has been performed here.

## Instructions for the designated GitHub Copilot integrator
Use these only after independent TUI review passes and integration is authorized.

1. Read this report, the final review artifact, and the latest `agm` handoff message. Pin the **full exact delivery SHA** from that message; verify its product files match reviewed product `bf01c9a`. Later record-only commits may add review evidence, but must not change `src/`, tests, manifests or README files.
2. Before any integration, inspect main's status and record byte hashes of these protected user files:
   - `AGENTS-TUTORIAL.md`
   - `HERDR-CORE-BOT.md`
   - `BOT-CEO.md` (may be untracked)
3. Do **not** stash, reset, checkout, clean, overwrite, stage or commit those files. Do not use `--autostash`. If integration requires changing them or resolving a conflict in them, stop and return a blocker.
4. Use the designated single integrator; do not start a second competing Copilot session or reuse a Copilot session belonging to another project.
5. Verify the exact reviewed safety/TUI heads and dependencies. Merge only authorized immutable heads, never a moving branch name without checking its SHA. Do not silently include unrelated work.
6. Run `cargo test --offline` and `cargo clippy --offline --all-targets -- -D warnings` on the combined result, then verify the protected file hashes/status are unchanged.
7. Do not install/restart agm, stop agents, push or release unless separately authorized. Return merge SHAs, test counts and any blockers.

The CLI is installed and `copilot --help` was checked. This handoff intentionally does not launch it with blanket permissions (`--allow-all`, `--yolo`, etc.).

## Known nonblocking presentation limits
- Set `COLUMNS` to the pane width (default 110, clamped 60–200); automatic console-size detection is not implemented.
- Frame height/scrollback is unbounded; bidi/format characters and terminal/font-dependent ambiguous widths/emoji rendering remain limitations.
- Legacy console VT setup and mintty TTY recognition vary by platform.
- Pending confirmations have no expiry, but still require the human's explicit `y`.
- At narrow widths, selected cleanup target descriptions may be clipped after the visible confirmation instruction/count; preserving the list while waiting is a future UX improvement.
- These are not claims of pixel-perfect rendering on every terminal or a substitute for controlled real-TTY testing.
