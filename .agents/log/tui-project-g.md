# tui-project-g log (Task 5, TUI wiring)

Branch: `feat/tui-project-g` · worktree `D:/Project/agmag.wt/tui-project-g`
Scope: `src/tui.rs`, `src/dashboard.rs` (`render_scoped` only), the three `#![allow]` lines,
`tests/dashboard_test.rs`, this log. Loader (`src/project.rs` bodies) is worker e's, merged already.

## 2026-11-23 · step 1 (red)
- Read plan Task 5 + spec. Confirmed the consumed APIs exist: `project::{compute_roots, load_project_view,
  git_common_root, list_projects, match_project, RootCache, ProjectKey, CdMatch}`,
  `project_render::{render_project_body, render_project_list, render_inbox_item}`,
  `Store::{terminals(false), inbox(true), inbox_item}`.
- Added tests first:
  - `src/tui.rs` unit test `prompt_shows_scope`: `agm[shop]> ` scoped, `agm> ` global, and a scope name
    carrying `\x1b]0;pwn\x07` / `\x1b[2J` renders inert (`render_frame` grew `scope: Option<&str>`).
  - `tests/dashboard_test.rs` `render_scoped_wraps_body_with_header_and_footer`: body markers + AGENTS lines
    appear between the header and the cmd footer, and no line exceeds width-1 at 60 (long body line included).
- `cargo test` → RED: E0061 (render_frame arity) and missing `render_scoped`.

## 2026-11-23 · step 2 (green)
- `src/dashboard.rs`: `pub fn render_scoped(s, body, width)` = `render_header` + body lines + `render_footer`,
  every line through `bounded_styled_line(line, width-1)` (no height fitting, frame scrolls).
- `src/tui.rs`:
  - `render_frame(..., scope)`: prompt `agm[<name>]>` (name via `one_line`, so escapes are stripped and the
    prompt stays inside the pane) or `agm>`; same bright-cyan prompt color.
  - `db_error_text` shares the existing "failed to read database" body between both views.
  - `scoped_text`: `terminals(false)` → `compute_roots(&terms, &snap, &mut cache, &git_common_root)` →
    `load_project_view` → `render_project_body` → `render_scoped`; errors use the DB-error body.
  - Loop state: `scope: Option<ProjectKey>`, `roots_cache: RootCache`, `shown: Option<String>`; `shown` is
    appended between the body and the note and cleared by the next command.
  - `Cd(None)` → `render_project_list(list_projects(terms, inbox(true), roots))` into `shown`;
    `Cd(..|/)` → leave scope (`back to all projects`); `Cd(q)` → `match_project`:
    `project: <name>` / `ambiguous: a, b` / `no project matching '<q>'`.
    `Show(id)` → `render_inbox_item` into `shown` / `no inbox item #<id>`.
  - reply/done/send/interrupt/kill/policy/view/cleanup unchanged.
- Removed `#![allow(dead_code, unused_variables)]` from `src/project.rs`, `src/project_render.rs` and
  `#![allow(dead_code)]` from `src/store/project.rs`; clippy is clean without them.
- Gates: `cargo test` green (all suites), `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings`
  clean.

## Note
- A second, near-identical copy of both new tests (and a duplicate `render_scoped`) appeared in the working
  tree while this session ran (another process writing the same worktree). They were merged into the single
  tests/function above and the duplicates removed; the assertions of both variants are kept.
