# Opus TUI review 1

- Reviewed product: `5d42e71`.
- Model: `claude-opus-5-5`, high effort.
- Session: `5457824e-748c-4378-8f58-4d3fe21a37b3`.
- Verdict: **PASS for TUI code gate**, no Critical/Major introduced by this branch in supplied code.
- Not merge/release approval. Source-only review; tests/Clippy were run by engineering, not independently by Opus.

## Confirmed
- Sanitization precedes coloring for untrusted fields; final line bounds retain only SGR.
- UTF-8 year slicing guarded by exact ASCII-year validation.
- Grapheme-safe cell bounds, protocol compatibility, display-only truncation.
- Human stdin/stdout TTY and typed-y boundaries remain fail-closed.

## Nonblocking findings
1. Char-based padding for wide scope/provider can hide soft/HARD badges after final clipping.
2. Actual pane size is not queried; width is `COLUMNS`, default 110, clamped to 60–200. README should state this accurately.
3. Long confirmation notes can clip away the typed-y instruction; put it first.
4. Frame height/scrollback is not bounded.
5. Unicode format/bidi characters are not stripped; width behavior is terminal/font-dependent, including ambiguous-width glyphs and ZWJ.
6. Some preliminary wrap calculations still count scalars; final cell clipping protects the frame but can shorten content.
7. Legacy Windows console VT setup/mintty TTY recognition remain platform limitations.
8. Pending confirmations have no expiration.

## Integration constraint
Inherited cleanup backend safety must be integrated from reviewed `safety/agy-followup@b2ac868` before release. TUI approval is not approval of unchanged backend cleanup.

## Engineering response
Proactively address badge padding, confirmation instruction order and README width wording; rerun tests/lint and request final review of the changed SHA. Preserve remaining nonblocking platform/height/bidi limitations in the handoff.
