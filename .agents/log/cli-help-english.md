# cli-help-english log

## 2026-11-21

### State before
- Branch/worktree: D:\Project\agmag.wt\cli-help-english on current branch.
- git log: 3497b46 cli-help-english: brief (adds .agents/tasks/cli-help-english.md), base includes recent main commits (cost SUM separator etc.).
- git status/diff: clean.
- Existing .agents/log/cli-help-english.md: none (creating now).

### Plan (from .agents/tasks/cli-help-english.md)
1. Port/add help tests in 	ests/cli_test.rs (TDD red): cli_help_is_english_for_every_command for every command/subcommand from current code, cli_stop_help_preserves_user_confirmation_warning for kill/interrupt, cli_send_help_explains_existing_message_codes. Update command list vs 0694e7f to match current main.
2. Translate all clap help in src/main.rs (doc comments / bout / help / long_about) to concise English matching current behavior; keep protocol message codes with English gloss e.g. GIAO (assign), "BÁO CÁO" (report); preserve safety warnings (kill/interrupt require user to type y for Codex/Claude/unknown apps). No behavior/name/alias/default/parser/JSON/runtime-string changes.
3. Add CHANGELOG [Unreleased] ### Thay đổi line: CLI help translated to English. Keep LF.
4. Verify: sample --help outputs, confirm git diff main -- src/main.rs touches only clap help text, run cargo test --offline, cargo clippy --offline --all-targets -- -D warnings, cargo fmt --check.

### Step 1: Help tests (RED)
- Added cli_help_is_english_for_every_command (all current commands/subcommands from src/main.rs Cmd/BudgetCmd/HookCmd), cli_stop_help_preserves_user_confirmation_warning (kill, interrupt), cli_send_help_explains_existing_message_codes to 	ests/cli_test.rs.
- cargo fmt -- tests/cli_test.rs
- Result: cargo test --offline --test cli_test help -> 1 passed (cli_cleanup_help_documents_mandatory_confirmation_even_with_yes), 3 failed as expected (help still Vietnamese / missing English safety+code explanations).
