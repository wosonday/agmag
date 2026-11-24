---
id: approval-runtime-fix1
tier: mid
skills: test-driven-development
branch: feat/approval-runtime
worktree: D:/Project/agmag.wt/approval
---

# Fix round 1 for approval-runtime (review: .agents/reviews/approval-runtime-1.md — read it first)

All items below were verified by the CEO. TDD: for each item write the failing test first (run it, see it fail),
then fix, then green, then commit (one commit per item, message `fix(approval): …`).

## Must fix
1. **One canonical form for action/lane/task.** Add `pub fn normalize_action(s: &str) -> String` (trim +
   ASCII lowercase) and `normalize_key(s)` (trim only) for lane/task, in src/actions.rs (or config.rs) and use
   them EVERYWHERE: `decide` (input action, input lane/task, and each `delegated_actions` entry before comparing),
   `is_hard_prohibited`, `Policy::validate` (empty + hard-prohibited check on the normalised entry), `approval add`
   (store normalised lane/task/action), `approval revoke`, MCP. Tests: `PUSH`, `Push`, ` push `, `push\n` are
   hard_prohibited in check, rejected in `delegated_actions` and in `approval add`; `delegated_actions =
   [" Continue_Review "]` allows `continue_review`; `approval add " lane-a " " task-a " X` then
   `check X --lane lane-a --task task-a` → allowed.
2. **Human-only add/revoke.** Replace the bare `stdin().is_terminal()` guard in src/main.rs `ApprovalCmd::Add` and
   `ApprovalCmd::Revoke` with the same check `agm board` uses: `agmag::board::require_human(&store, &hc)` pattern
   (src/board/mod.rs: `detect_caller(...)` must return `Caller::User`). Either call a shared helper or factor
   `require_human` into src/actions.rs with an action-specific message. An agent in an hc PTY (has
   `HC_TERMINAL_ID` / known agent terminal) must be refused. Unit-test the decision through
   `classify_caller`/`detect_caller` with FakeHc (see existing tests for `detect_caller`).
3. **Revoke must report reality.** `Store::revoke_approval_grant` returns the number of rows revoked; the CLI
   errors (non-zero exit, no `revoked: true`) when 0 rows matched, and only logs `ok=true` when ≥ 1. Normalise
   inputs as in item 1.
4. (covered by 1)
5. **A new grant supersedes older ones.** In `approval add`, inside one transaction, revoke every active grant for
   the same (lane, task, action) and then insert the new one (so an older no-expiry grant cannot come back when the
   new one expires). Rename `upsert_approval_grant` to `add_approval_grant` if it helps clarity. Test: add without
   expiry, add again with expiry T, at now ≥ T → requires_human.
   Also reject `--expires-ms` ≤ now (it is an absolute epoch-ms deadline; say so in the clap help).

## Should fix
6. `approval check` writes nothing (remove its `log_action`), so it is truly read-only as documented.
7. `exit_code`: `ApprovalRefused` must never map to 0 — make it hold only the two refusal kinds (or map Allowed to 1).
8. `Cmd::Approval` branches: propagate `emit(...)` errors (`emit(...)?` / return it) instead of `let _ =`.
9. Test in tests/store_v2_test.rs: a DB created at schema v4 (copy how the other migration tests build old
   schemas) opens, migrates to v5, has `approval_grants`, and keeps existing v4 data.
10. `approval ls` on a TTY: one readable line per grant (id, lane/task, action, expires or "no expiry", revoked or
    active, reason) instead of `{:?}`.

## Constraints
- Same as .agents/tasks/approval-runtime.md: only this worktree; no push/merge/branch switch; keep each file's line
  endings (src/main.rs and CHANGELOG.md are CRLF — check `git ls-files --eol` before every commit, never commit
  `w/mixed`); log each item in `.agents/log/approval-runtime.md` under `## Fix round 1`; commit after EVERY item.
- Update README.md / README_vi.md / BOT-CEO.md only where behaviour text changes (case-insensitive actions,
  human-only add/revoke via caller detection, `--expires-ms` absolute, check is read-only).
- Final gates clean and recorded: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `python scripts/lint_harness.py`.
- Finish with one line: `DONE approval-runtime-fix1 <HEAD sha>` or `BLOCKED: <reason>`.
