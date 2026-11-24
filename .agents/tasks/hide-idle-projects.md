---
id: hide-idle-projects
tier: low
skills: test-driven-development
branch: feat/hide-idle-projects
worktree: D:/Project/agmag.wt/ghost-projects
---

# Hide idle ("ghost") projects from the project lists

## Problem
`cd` (no argument) in `agm tui` and the board's project column list every project root ever seen, including old
worktrees such as `agmag.wt/<slug>` with no live agent and no open inbox item. They clutter the list.

## Rule
A project is **idle** when `live_agents == 0 && open_inbox == 0` (`ProjectSummary` in src/project.rs).
- Hide idle projects in the two DISPLAY lists only:
  1. `cd` with no argument: src/tui_cmd.rs (~line 256, `render_project_list`). After the list add one line
     `N idle project(s) hidden — cd <name> still opens them` when N > 0 (keep the existing style of
     src/project_render.rs; no line when N = 0).
  2. The board project column: src/board/mod.rs (~line 149, `BoardData.projects`). Never hide the project the
     board is currently showing/scoped to, even if idle.
- **Do not** change name resolution: `cd <name>` (src/tui_cmd.rs ~406 and src/board/mod.rs ~100 call
  `match_project` on the full list) must still find idle projects. Do not change src/board/app.rs (~302); it maps
  roots to keys and needs the full list.
- Put the filter in one small pure function in src/project.rs, e.g.
  `pub fn split_idle(list: Vec<ProjectSummary>, keep: Option<&ProjectKey>) -> (Vec<ProjectSummary>, usize)`.

## Work (TDD: red → code → green → commit, one commit per step)
1. `split_idle` + unit tests (idle hidden, live kept, inbox-only kept, `keep` kept even if idle, count correct,
   order preserved).
2. `cd` listing uses it + test of the rendered text (hidden line present when N>0, absent when 0). Look at the
   existing tests for `render_project_list` / tui_cmd for the pattern.
3. Board column uses it (keep the scoped project) + test.
4. CHANGELOG.md `[Unreleased]` → `### Thay đổi`: one Vietnamese bullet.

## Constraints
- Only this worktree/branch; no push/merge/branch switch.
- Keep each file's line endings (CHANGELOG.md, src/main.rs, src/tui.rs are CRLF; src/board/*, src/project.rs are
  LF). Check `git ls-files --eol <file>` before each commit; never commit `w/mixed`.
- Log each step in `.agents/log/hide-idle-projects.md` and commit after EVERY step.
- Final gates, clean, recorded in the log: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`.
- When done print one line: `DONE hide-idle-projects <HEAD sha>` (or `BLOCKED: <reason>`).
