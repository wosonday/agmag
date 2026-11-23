# TUI project scope — worker D (Task 4) log

## 2026-11-23 · cd/show commands
- Branch `feat/tui-project-d`, worktree `D:/Project/agmag.wt/tui-project-d`. Task 4 only.
- Tests added to `tests/dashboard_test.rs`: `parse_cd_and_show` (cd list / arg / `..` / `/`, show with and without `#`, errors mention `show <#>`) and `footer_lists_cd_and_show` (footer shows `cd [project|..]` and `show <#>`, no stray `·  ·`).
- `src/dashboard.rs`: added `TuiCmd::Cd(Option<String>)` and `TuiCmd::Show(i64)`; `TUI_COMMANDS` grown 9 → 11 with `("cd", "", "[project|..]")` and `("show", "", "<#>")` after `view`; `parse_command` accepts empty alias only for named commands, parses cd/show; footer rendering unchanged (ignores the alias field, so empty aliases render cleanly).
- Fixed the E0004 build break with one placeholder arm in `src/tui.rs`: `Ok(TuiCmd::Cd(_) | TuiCmd::Show(_)) => note = Some((NoteKind::Warn, "cd/show: not wired yet".into()))`. Task 5 will replace it. No other lines touched in tui.rs.
- Gates all green: `cargo test` (all suites pass, incl. 2 new tests), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
- Committed as `feat(tui): cd and show commands`. Did not push/merge/switch branches.
