# TUI engineering review

## Candidate
- Original: `7e20645d4b8e1c4e2c1238c4d3f8d0f2be753e32`, based on `3302104`.
- Candidate worktree remained clean and untouched.
- Baseline offline suite: 194 passed; strict offline Clippy: passed.

## Findings on original candidate — changes required
1. `one_line` counted Unicode scalars rather than terminal cells: a 12-cell limit produced 23 cells of CJK text.
2. `wrap_plain_words` did not bound long continuation words: a path produced a 277-character warning line at width 60.
3. Inbox/message fixed prefixes were not bounded: metadata produced a 731-character line at width 60.

All three were reproduced by new tests before fixes.

## Corrections on review/tui-english-color
- Grapheme-safe terminal-cell truncation; direct references to existing locked/cached Unicode crates only.
- Bounded continuation words and metadata prefixes.
- Final per-line cell limits preserve trusted SGR and reset styling; frame notes/cleanup displays receive the same defensive bounds.
- Malformed non-ASCII year keys no longer risk invalid UTF-8 slicing.
- Both READMEs describe display-only truncation and sanitization.
- No changes to caller detection, confirmation, policy toggling, action targets, stored messages or backend actions.

## Audit
- `cargo test --offline --locked`: 202 passed, 0 failed after final padding/confirmation follow-ups.
- `cargo clippy --offline --all-targets -- -D warnings`: passed, no warnings.
- Added 8 tests covering Unicode/graphemes, long paths/prefixes, malicious note escapes, SGR/reset, visible soft/HARD badges/confirmation guidance and cell widths at 60/80/110/200.
- Independent Opus review of final correction `bf01c9a`: PASS; see `.agents/reviews/tui-english-color-opus-2.md`.

## Scope limits
- No real interactive/destructive TTY trial.
- Terminal-cell widths follow unicode-width's standard width model; actual fonts/terminal settings can differ.
- Existing cleanup backend defects belong to the separately reviewed `safety/agy-followup` branch (`b2ac868`). Integrate that safety work before calling the combined application release-ready.
- No merge or installation performed. Protected user guidance files on main were not changed.
