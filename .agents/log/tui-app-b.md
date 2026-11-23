# tui-app-b log — ANSI bridge (Task B)

## Task
Plan Task B: implement `ansi_to_text` in `src/board/ansi.rs` (files_allowed: only this file + this log).

## Read first
- `docs/superpowers/plans/2026-11-24-tui-phase2.md`: Global Constraints, Review Focus, Task 0 interfaces, Task B.
- `docs/superpowers/specs/2026-11-24-tui-phase2-design.md`: incl. Builder-check answers.
- `src/board/ansi.rs` (the `todo!("Task B")` stub), `src/dashboard.rs` (SGR constants, `Section`, `render_section_body`, `Snapshot`, `section_body_tests::empty`), `src/board/mod.rs`, `Cargo.toml` (ratatui 0.30.2 resolved).

## Step 1: failing tests (red)
Added all 7 named tests in `src/board/ansi.rs` under `#[cfg(test)]`, with a small `joined(&Line)` helper:
- `plain_text_one_line`
- `newlines_split_lines`
- `colors_and_bold`
- `bright_and_indexed`
- `style_carries_across_newline`
- `osc_and_cursor_sequences_dropped`
- `real_dashboard_section_roundtrip`

`cargo test board::ansi::tests` → **0 passed; 7 failed; 0 ignored** — every test panicked at `todo!("Task B")`.

## Step 2–3: implement
Replaced the stub with a byte scanner that:
- buffers UTF-8 text runs (only ASCII control bytes and ESC terminate a run, so all slice points are UTF-8 boundaries);
- parses `ESC [ params m` only (CSI final byte `0x40..=0x7e`), applying `0` (reset), `1` (BOLD), `2` (DIM), `22` (unset BOLD/DIM), `30..=37`, `39`, `90..=97`, `38;5;n` → `Color::Indexed(n)`; unknown codes ignored;
- drops OSC payloads (BEL or `ESC \` terminator), all other CSI/ESC sequences, and C0 controls except `\n` (also drops DEL);
- splits `Line`s on `\n` and carries the current `Style` across lines until reset;
- merges adjacent spans with the same style so dropped escape sequences do not fragment visible text; skips empty spans;
- consumes (but does not style) `38;2;r;g;b` and `48`/`48;5;n` parameter groups so their bytes cannot leak into SGR handling.

Private helpers: `push_span`, `skip_escape`, `apply_sgr`, `standard_color`, `bright_color`. No frozen signature/type was changed; no other file was edited.

## Step 4: green + gates
- `cargo test board::ansi::tests` → **7 passed; 0 failed**.
- `cargo test` (full, all targets + doc tests) → **529 passed; 0 failed; 0 ignored** (38 test binaries/result lines, incl. 1 doc-test line with 0 tests).
- `cargo fmt --check` → clean (after `cargo fmt`; original failing diff was one wrapped `let snap = dashboard::snapshot(...)` line).
- `cargo clippy --all-targets -- -D warnings` → clean.

## Decisions (least-surprising behaviour where the plan is silent)
- `39` clears the fg override to `None` (default), rather than `Some(Color::Reset)`, so a post-reset span compares equal to a plain `Style::new()` span — this is what the named `colors_and_bold` / `bright_and_indexed` tests assert.
- A trailing `\n` does not add an extra empty trailing `Line`; empty input yields one empty `Line` (so `Text` is never line-less).
- `38;2;…` and background (`48`) codes are consumed but intentionally produce no colour: the dashboard emits foreground SGR only, and the spec lists only `38;5;n`.
- Adjacent same-style text is merged into one span (so `"a<dropped-esc>b"` is one span `"ab"`).

## Open questions
- None. No plan/code contradiction encountered. Other workers' `todo!`s were left untouched.
