# tui-board-d log — Task 4 (runtime, `agm board`, `cd` integration)

Branch: `feat/tui-board-d`, base `8d209e1` (Task 0 scaffold).
Scope: `src/board/mod.rs` logic, `src/main.rs`, `src/tui.rs`, `src/dashboard.rs`,
`tests/board_runtime_test.rs`; in-file unit tests for the terminal guard. No frozen signature changed.

## Steps

- [x] 1. Wrote the failing tests named in Plan Task 4:
  - `tests/board_runtime_test.rs`: `require_human_refuses_non_tty` (spawns the real
    `agm board` with piped stdio), `resolve_project_unknown_and_ambiguous` (temp store, two
    repositories called `shop`), `load_data_without_project_lists_projects`,
    `apply_effect_errors_become_notes` (missing `done` id, reply to a resolved item, send to an
    unknown terminal through `FakeHc`), `apply_effect_save_theme_persists`,
    `parse_cd_t` (`cd -t shop`, `cd -t`, `cd shop`, `cd`).
  - `src/board/mod.rs` unit tests: `guard_leaves_once_on_drop`, `guard_leaves_on_early_return`,
    plus `explicit_leave_is_not_repeated_on_drop` (fake `TermControl` recording calls).
- [x] 2. `cargo test --test board_runtime_test` → compile FAILED (`TuiCmd::CdText` missing), i.e.
  red before any implementation. The in-file guard tests could not exist before `TermGuard`.
- [x] 3. Implemented, in this order:
  - `dashboard.rs`: `TuiCmd::CdText(String)`; `cd` parser splits `-t` from its argument
    (`cd -t` alone → usage error mentioning both `cd <project>` and `cd -t <project>`);
    `cd`/`cd -t` doc comments; footer usage string extended to
    `[project|..] (text: -t <project>)` (the old `cd [project|..]` substring still matches
    `tests/dashboard_test.rs`).
  - `board/mod.rs`:
    - `require_human`: stdin+stdout TTY and `detect_caller(.., tty, false) == Caller::User`;
      error text names `agm board`.
    - `resolve_project`: `dashboard::snapshot` → `terminals(false)` → `compute_roots` →
      `list_projects` → `match_project`; `ambiguous: …` / `no project matching '…'`.
    - `load_data`: snapshot, terminals, open inbox, roots, project list; with a chosen project:
      `load_project_view`, `pair_latest_messages` over the agents' `flows::agent_keys`, `flows::edges`,
      `state.sync`, then the selected agent's newest 30 messages via `messages_involving`.
    - `apply_effect`: reply / done (legacy single-id path, same as `agm done <id>`) / send from
      `user` / save `board.theme`; every outcome or error becomes `state.note`; never panics and
      never returns an error. A recorded-but-undelivered message (`status == "failed"`) is an
      error note.
    - terminal guard: private `TermControl`, `TermGuard<T>` (leaves exactly once, in `Drop` or via
      explicit `leave`), real `ConsoleTerm` (raw mode + `EnterAlternateScreen` + hide cursor and the
      reverse).
    - `run`: reads `board.theme` before raw mode, installs a panic hook that leaves the console before
      the previous hook (and restores the previous hook on exit), enters the guard, then loops:
      `scan` → `load_data` (failure keeps last data + error note) → `view::draw` with
      `AGM_ASCII=1` → `event::poll` until the next refresh; keys go to `state.on_key`; `Quit` exits,
      `SwitchProject` updates `state.project`, anything else runs `apply_effect` and refreshes at once.
  - `main.rs`: `Cmd::Board { project: Option<String>, refresh }` ("Full-screen project board");
    `require_human` → optional `resolve_project` → `board::run` with the same scan closure as `Cmd::Tui`.
  - `tui.rs`: stdin reader now sends one line then blocks on a `resume` channel; the main loop sends
    `resume` exactly once per handled line, so no stdin read is pending while the board owns the
    console. `cd <q>` resolves the project (helper `lookup_project`, original note kinds kept) and
    runs the board with the text refresh interval; on return `last_frame = None; after_input = true`
    for a full repaint and the text scope is not changed. `CdText(q)` keeps the old text project view
    (including `..` / `/`).
- [x] 4. Green gates:
  - `cargo test` → 468 passed, 0 failed (37 test targets: 8 lib + 7 main + 6 new
    `board_runtime_test` + the pre-existing suites).
  - `cargo fmt --check` → clean.
  - `cargo clippy --all-targets -- -D warnings` → clean (grouped the loop's mutable state into
    `Session` to satisfy `too_many_arguments`).
- [x] 5. Commit `feat(board): runtime, agm board, cd integration` (this log included).

## Test names (new)

`require_human_refuses_non_tty`, `resolve_project_unknown_and_ambiguous`,
`load_data_without_project_lists_projects`, `apply_effect_errors_become_notes`,
`apply_effect_save_theme_persists`, `parse_cd_t`,
`guard_leaves_once_on_drop`, `guard_leaves_on_early_return`,
`explicit_leave_is_not_repeated_on_drop`.

## Open questions

1. `BoardState::sync` is still `todo!("Task 2")` in this worktree, so `load_data` calls it only when
   a project view exists (agent/inbox selections only exist there) and clamps `sel_project` itself on
   the projectless screen. When Task 2 merges, the call is already in the right place; no change is
   needed unless the CEO wants `sync` called unconditionally on the no-project screen too.
2. Plan says `note.ok == false` for "send to an unknown terminal", but `actions::send` records a
   failed delivery and returns `Ok(MessageRow { status: "failed" })`. Implemented
   `ok = m.status != "failed"` with the plan's text (`msg #id → to: status`), so the footer still
   shows an error and the test asserts `note.ok == false`.
3. `tests/dashboard_test.rs` (not in `files_allowed`) pins `cd [project|..]` in the footer, so the
   `-t` hint was appended after that substring rather than reworded: `[project|..] (text: -t <project>)`.
   The `cd -t` usage error spells out both forms.
4. "On return keep the scope global": `cd <project>` only launches the board and never sets `scope`.
   If the REPL already had a text scope from an earlier `cd -t`, that scope is left untouched (it is
   not forcibly cleared). Least surprising behavior chosen.
