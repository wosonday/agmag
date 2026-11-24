---
id: approval-runtime
tier: mid
skills: test-driven-development
branch: feat/approval-runtime
worktree: D:/Project/agmag.wt/approval
---

# Delegated approvals: make grants usable at runtime (`agm approval check` + MCP tool)

## Context

Commit 36fbee4 (on this branch) added schema v5 `approval_grants`, `[policy].delegated_actions`, and
`agm approval add/ls/revoke` (TTY-only). Nothing reads a grant at runtime yet: `actions::runtime_approval_decision`
(src/actions.rs) has no caller, and its rules are inconsistent (an action outside a hard-coded
`TASK_ONLY_GRANTS` list is always refused even when globally whitelisted). BOT-CEO.md §2.1 states the intended rule:
an agent (the CEO) may auto-approve an action only if it is in the global whitelist OR in an active task grant;
hard-prohibited actions never.

## Decision rule (CEO decision — implement exactly)

`decide(store, cfg, action, lane: Option, task: Option, now) -> Decision { decision, source }`:
1. `action` trimmed; empty → error.
2. `action` in HARD_PROHIBITED (keep the existing list in src/actions.rs) → `hard_prohibited`, source `"hard"`.
3. `action` in `cfg.policy.delegated_actions` → `allowed`, source `"global"`.
4. `lane` AND `task` both given, and `store.approval_grant(lane, task, action, now)` returns an active grant
   (not revoked, not expired) → `allowed`, source `"grant:<id>"`.
5. otherwise `requires_human`, source `"none"`.

Replace `runtime_approval_decision` with this (remove `TASK_ONLY_GRANTS`). Simplify or remove
`evaluate_delegated_approval` (and its `#[allow(clippy::too_many_arguments)]`) if no longer needed; update
tests/actions_test.rs accordingly — keep every existing behaviour that still matches the rule above.

## Work (TDD: red test → code → green → commit, one commit per step)

1. **Rule** in src/actions.rs as above. Unit/integration tests for each of the 5 branches, including an expired
   grant, a revoked grant, a grant for another lane/task, and a hard-prohibited action that is ALSO in
   `delegated_actions` (must stay `hard_prohibited`).
2. **`agm approval add`** must refuse a hard-prohibited action and empty lane/task/action (clear error, nothing
   written). Keep the TTY-only rule for `add` and `revoke`; fix the revoke error text (it says "tạo", should say
   it only lets a TTY user revoke).
3. **`agm approval check <action> [--lane <L>] [--task <T>]`** (no TTY requirement; agents call it): prints
   `{"decision":"allowed|requires_human|hard_prohibited","action":..,"lane":..,"task":..,"source":..}` as JSON
   (follow the existing `--json` / non-TTY convention in src/main.rs) or one human line on a TTY.
   Exit code: 0 for allowed, **5** for requires_human, **6** for hard_prohibited (add constants next to
   `EXIT_BRIEF_UNDELIVERED` and map them in `actions::exit_code`, same pattern as `BriefUndelivered`).
   Read-only: writes nothing except optionally an action log row. Test in tests/cli_test.rs (see how other CLI
   tests set `AGM_HOME` to a temp dir).
4. **MCP tool `agm_approval_check`** in src/mcp.rs: args `action` (required), `lane`, `task` (optional strings);
   unknown args rejected like the other tools; returns the same JSON object. Read-only. Add it to the tool list.
   Tests in tests/mcp_test.rs. There is still NO MCP tool to add/revoke grants.
5. **Config**: in `Policy::validate` (src/config.rs) reject empty `delegated_actions` entries and hard-prohibited
   names in `delegated_actions` (error names the entry). Test in tests/config_test.rs. Move/share the
   HARD_PROHIBITED list so config.rs and actions.rs use one constant.
6. **Docs**: BOT-CEO.md §2.1 (how the CEO checks: `agm approval check` / `agm_approval_check`, exit codes);
   README.md and README_vi.md command reference (approval add/ls/revoke/check, exit codes 5/6, MCP tool count
   7 → 8 wherever the docs state it); CHANGELOG.md `[Unreleased]` → `### Thêm mới` one bullet (Vietnamese).

## Constraints

- Only this worktree/branch. No push, no merge, no branch switch. Do not touch other worktrees.
- **Line endings:** keep each file's existing ending. src/main.rs and CHANGELOG.md are CRLF; most other files are
  LF. If you edit with a script, preserve CRLF/LF (`newline=''` in Python). Check with
  `git ls-files --eol <file>` before each commit — never commit `w/mixed` or a whole-file EOL change.
- Log each step in `.agents/log/approval-runtime.md` (what changed, test names, gate results) and commit after
  EVERY step, so a replacement can resume from disk if your model is capped.
- Comments follow the file's existing style (short Vietnamese is fine).
- Final gates, all clean, results written to the log: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test`, `python scripts/lint_harness.py`.
- When done, print one line: `DONE approval-runtime <HEAD sha>` (or `BLOCKED: <reason>`).
