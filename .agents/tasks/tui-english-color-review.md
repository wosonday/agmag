# tui-english-color review

## Assignment
From agy `t38`: audit candidate `7e20645d4b8e1c4e2c1238c4d3f8d0f2be753e32` for English TUI, ANSI width/sanitization, edge cases, fail-closed behavior and README. Run offline tests and strict Clippy; fix reproducible issues before preparing an exact-SHA Copilot handoff. Report through `agm send user --type "BÁO CÁO" --task tui-english-color`.

## Isolation
- Source candidate: `D:/Project/agmag/.wt/tui--english-color`, untouched.
- Review/fix branch: `review/tui-english-color`.
- Worktree: `D:/Project/agmag.wt/review--tui-english-color`.
- Do not edit main or `AGENTS-TUTORIAL.md`, `HERDR-CORE-BOT.md`, `BOT-CEO.md`.
- No merge/install/release, actual agent stopping, permissions bypass or production-data changes.

## Scope
- Candidate's five changed files: dashboard/TUI rendering, dashboard tests, English/Vietnamese READMEs.
- Display-cell/grapheme fixes may add direct references to already-locked/cached `unicode-width` and `unicode-segmentation`; no new package downloads. Include `Cargo.toml`/`Cargo.lock` in the fix scope.
- Review/task/log/handoff records.

## Checks
- Baseline candidate offline tests and strict Clippy.
- Red/green reproducers for any new findings.
- Final suite and strict Clippy at the final exact source SHA.
- Review English chrome while preserving message codes and user content.
- Preserve fail-closed TTY/caller checks and explicit confirmation.
