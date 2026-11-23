# tui-board-c log (Task 3: ratatui view)

## 2026-11-23

### State before
- Worktree: D:\Project\agmag.wt\tui-board-c, branch feat/tui-board-c (base 8d209e1 feat(board): scaffold module and frozen interfaces).
- Files to touch: src/board/view.rs, tests/board_view_test.rs, this log.
- Task 0 interfaces confirmed: `layout_mode(width, height)`, `draw(frame, state, data, theme, ascii)`;
  `BoardData`, `BoardState`, `Theme`/`theme()`, `AgentView`/`ProjectView`, `flows::{Edge, is_live}`,
  `dashboard::{sanitize_one_line, format_status}`, `project_render::age_label`, `actions::KINDS_EN`.

### Step 1: failing tests (RED)
- Added `tests/board_view_test.rs` (ratatui `TestBackend`, 14 tests):
  `layout_mode_thresholds`, `wide_has_all_panel_titles`, `medium_drops_projects_column`, `narrow_stacks`,
  `selected_card_uses_selected_color`, `unknown_values_render_dash`, `hostile_text_is_inert_and_clipped`,
  `live_edge_accent_stale_edge_muted`, `ascii_mode_has_no_box_drawing`, `empty_states_no_panic`,
  `tiny_terminal_no_panic`, `popup_reply_visible`, `stage_pipeline_mapping`, `inbox_rows_and_selected_row`.
- `cargo test --test board_view_test` -> 0 passed, 14 failed, all `not yet implemented: Task 3`.
- Two test-helper refinements during RED (not implementation-driven): narrow search helpers to the
  column of the panel under test (`find_in`) because the agent label also appears in the detail title,
  and stage colours are read from the detail column (106..140) rather than the card's `stage:` line.

### Step 2-3: implementation (GREEN)
- `src/board/view.rs`:
  - `layout_mode`: ≥120×≥20 Wide, 80–119×≥20 Medium, else Narrow.
  - Wide: 20-col Projects/Status | flexible orchestration | 34-col Detail/Messages, 5-row Inbox,
    1-row footer. Medium drops the left column. Narrow stacks orchestration / detail / inbox.
  - Panels are rounded blocks (`BorderType::Rounded`) with title in `theme.title`; `AGM_ASCII` switches
    every border to a custom `+ - |` set (no `U+2500..=U+257F` glyphs anywhere).
  - Orchestration: agents grouped by depth, cards 18×4, wrapped per row; card line 1 status symbol +
    label + app/model, line 2 `doing: … age` or `stage:` or `-`; exited cards muted; selected card
    border `theme.selected` + bold label. Connector row between a depth row and its children draws
    `─ KIND age ─` per edge, `theme.accent` when `flows::is_live`, `theme.muted` otherwise, nothing
    without an edge.
  - Status symbols: `● ◐ ✓ ✗ -` (ASCII `* ~ v x -`).
  - Detail: app, model/effort, status (`dashboard::format_status`), task, stage pipeline
    `wip→ready→merging→merged` with the current step highlighted, ctx 8-cell gauge (`ctx: -` when
    None), token total (`-` when None) + `Sparkline` when a 12-bucket series exists, `doing:` + age,
    `last` message kind/direction.
  - Messages: `from → to KIND age` + wrapped text, newest first, `msg_scroll`, `state.filter`
    case-insensitive over text/from/to.
  - Inbox: `▸ #id kind terminal text`, selected row `theme.selected` + bold.
  - Popups (`Reply`, `Send`, `ConfirmDone`, `ShowItem`, `Filter`, `PickProject`, `Help`) centered with
    `Clear`; footer shows notes (ok/error colour) or the key hints.
  - Every untrusted string (project/label/app/model/status/stage/doing/edge kind/message/inbox/note/
    popup input/to) passes through `dashboard::sanitize_one_line` before reaching a `Span`.
- `cargo test --test board_view_test` -> 14 passed.

### Step 4: gates
- `cargo test` -> 473 passed, 0 failed (459 baseline + 14 new).
- `cargo fmt --check` -> clean.
- `cargo clippy --all-targets -- -D warnings` -> clean.

### Open questions
- The plan asks the detail panel for `task`, but `AgentView` has no task field (it was not carried from
  `TerminalRow`). Per the "unknown values render as `-`" rule the line is rendered as `task: -` rather
  than inventing data; giving `AgentView` a task field would be a Task 0 interface change, out of scope.
- The 34-column detail panel cannot fit `stage: wip → ready → merging → merged` plus a deployed/blocked
  badge on one line, so separators are single characters (`→`, ASCII `>`) and the badge (if any) goes on
  the next line. Every step and badge stays visible.
- Narrow (<80 cols or <20 rows) shows orchestration / detail / inbox only, per the plan; the Messages
  panel is not part of the stacked narrow layout.

### Next
- Task 3 complete. No frozen signature/type changed; only `src/board/view.rs`,
  `tests/board_view_test.rs` and this log.
