# tui-project-i log (fix 2 blockers from tui-project-scope-2-prep review)

## 2026-11-23

### State before
- Worktree: D:\Project\agmag.wt\tui-project-i, branch feat/tui-project-i (base c46fea7 review CHƯA ĐẠT).
- Review: .agents/reviews/tui-project-scope-2-prep.md — 2 blockers in `project::load_project_view` (src/project.rs).
- Files to touch: src/project.rs (load_project_view body only), tests/project_test.rs, this log.

### Step 1: failing tests (RED)
- tests/project_test.rs: added `load_project_view_status_uses_snapshot_format`
  (live blocked + kind HỎI → status must start with "BLOCKED" and equal snapshot TermView.status;
  handed-off live → "BLOCKED [HANDED OFF]"; exited kept by inbox → stored status preserved)
  and `load_project_view_messages_include_exited_member_without_inbox`
  (exited member without open inbox items sent a message → in pv.messages, not in pv.agents).
- `cargo test --test project_test` → 11 passed, 2 failed:
  "live blocked status: blocked" and "exited member's message missing: []".
- Commit 26c79ae `test(tui): red tests for project view status format and exited-member messages`.

### Step 2: fix (GREEN)
- src/project.rs `load_project_view` body only:
  - Blocker 1: `AgentView.status` = live snapshot `TermView.status` when a view exists
    (keeps "BLOCKED (kind)" / "[HANDED OFF]" so project_render shows red ◐ blocked);
    exited / no view → stored `t.status`, `None` → "-" (a3.status == "-" still holds).
  - Blocker 2: `all_members = members(&terms, roots, key)`; `kept` (AGENTS) filters from it as before;
    `messages_involving(&parties(&all_members), 20)` so exited members without inbox items are covered.
    Inbox filter unchanged (`parties(&kept)`), per review's minimal fix.
- `cargo test --test project_test` → 13 passed.
- Gates: `cargo test` → 437 passed, 0 failed; `cargo fmt --check` clean;
  `cargo clippy --all-targets -- -D warnings` clean.
- Commit 8489f63 `fix(tui): project view status from snapshot; messages cover all project members`.

### Next
- Both blockers fixed; review "Nên sửa"/"Nhỏ" items left out of scope (not requested).
- No push/merge/branch switch; ready for re-review.
