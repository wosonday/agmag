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

### Step 2: Translate clap help (GREEN)
- Translated all clap bout/doc comments/arg help in src/main.rs (Cli, Cmd, BudgetCmd, HookCmd) to concise English matching current behavior. Kept protocol codes with English gloss and English aliases for send. Preserved safety warnings for kill/interrupt ("Codex/Claude and unknown apps still require a user to type y"). Did not change command/flag names, aliases, defaults, parsers, JSON keys, or runtime strings.
- cargo fmt -- src/main.rs
- Result: cargo test --offline --test cli_test help -> 4 passed (including the 3 new help tests).
- Sample help: root lists all commands in English; send --help shows code gloss+aliases; kill --help shows confirmation warning; udget set --help shows English arg descriptions.

### Step 3: CHANGELOG
- Added Unreleased/### Thay đổi bullet: "Help CLI chuyển sang tiếng Anh."

### Verification
- cargo test --offline: all tests passed (full workspace, including 	ests/cli_test.rs help tests: 4/4). Key suites observed in run: cli 22, agy_quota 12, config 3, context 3, correlate 2, daemon 4, ingest 3, launch 6, model 5, opencode 15, opencode_go_quota 9, opencode_metrics 10, parse_claude 4, parse_codex 5, policy 12, report 8, rotation 27, safety 12, spawn_fallback 9, store 11, store_v2 11, sysmon 10, doc-tests 0.
- cargo clippy --offline --all-targets -- -D warnings: clean (exit 0).
- cargo fmt --check: clean.
- git diff main -- src/main.rs: inspected with git diff --unified=0 main -- src/main.rs. All changes are clap help text only: /// doc comments and bout = string on Cli; no runtime strings, command/flag names, aliases, defaults, parsers, or JSON changed. Minimal structural reflow: Done and Link variants expanded from single-line to multi-line solely to attach per-field /// help (field names/types unchanged: id: i64, 	erminal: String, session: String). No other hunks alter logic.
- Help spot-checks (also covered by cli_help_is_english_for_every_command over all commands/subcommands): gm --help, gm send --help, gm kill --help, gm budget set --help are English; send explains protocol codes with English gloss and aliases; kill/interrupt preserve "Codex/Claude and unknown apps still require a user to type y".
