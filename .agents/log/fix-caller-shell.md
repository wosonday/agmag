# fix-caller-shell

- 2026-11-23: Wrote failing tests in `tests/actions_test.rs` (6 cases: stale `reported`, stale DB app, live status, non-shell foregrounds, hc get error, no TTY). Confirmed they failed because stale flags/apps made the caller an agent.
- 2026-11-23: Implemented `idle_shell_foreground` + `is_interactive_shell` in `src/actions.rs`; `detect_caller` now treats a shell foreground with no live `status.app` as the user, ignoring stale `reported` and the agm.db app. Tests green; `cargo fmt --check`, clippy, full `cargo test` pass.
- 2026-11-23: Added `## [Unreleased]` changelog entry (Vietnamese).
