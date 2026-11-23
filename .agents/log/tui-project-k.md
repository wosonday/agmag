# tui-project-k log (tui-project-scope-final "Nên sửa" 1 load_project_view parts + 2)

## 2026-11-23

### State before
- Worktree: D:\Project\agmag.wt\tui-project-k, branch feat/tui-project-k (base a000969).
- Review: .agents/reviews/tui-project-scope-final.md — 3 items in scope (exited status format,
  stage cwd fallback, `kept` filter with empty names). src/project_render.rs / tree_order untouched.
- Files touched: src/project.rs (load_project_view + private row_cwd helper), src/dashboard.rs
  (extract `format_status`, no behavior change), tests/project_test.rs, this log.

### Step 1: exited AgentView.status (RED → GREEN)
- tests/project_test.rs: added `load_project_view_exited_status_uses_dashboard_format`
  (exited member kept by open inbox, stored status "blocked" + kind "HỎI" → status must start
  with "BLOCKED" and equal "BLOCKED (HỎI)"). Red: got "blocked".
- src/dashboard.rs: extracted `pub fn format_status(status, kind)` from `snapshot`; snapshot now
  calls it, so dashboard output is unchanged. src/project.rs: exited agents use the same helper.
- Commit b08897e `fix(tui): format exited project agent status like the global view`.

### Step 2: stage cwd fallback (RED → GREEN)
- tests/project_test.rs: added `load_project_view_stage_falls_back_to_live_termview_cwd`
  (live terminal, row cwd None, linked session cwd `<root>\.wt\a--b`, BRANCHES.md a/b status wip
  → stage Some("wip")). Red: got None.
- src/project.rs: private `row_cwd(t, snap)` = row cwd (non-empty), else live `TermView.cwd`;
  `compute_roots` and `load_project_view`'s `branch_entry_for` both use it.
- Commit c71f799 `fix(tui): use the live TermView cwd fallback for project stage`.

### Step 3: empty-name inbox match in `kept` (RED → GREEN)
- tests/project_test.rs: added `load_project_view_kept_ignores_empty_names`
  (exited terminal name `""` + inbox item terminal `Some("")` → terminal absent from pv.agents).
  Red: terminal was kept.
- src/project.rs: the name leg of the `kept` inbox match ignores empty names
  (`t.name.as_deref().filter(|n| !n.is_empty()) == Some(term)`), like list_projects/parties.
- Commit: `fix(tui): ignore empty terminal names when keeping project agents`.

### Gates (final)
- `cargo test` all green; `cargo fmt --check` clean;
  `cargo clippy --all-targets -- -D warnings` clean.
- No push / merge / branch switch.
