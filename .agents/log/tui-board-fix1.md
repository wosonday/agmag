# tui-board-fix1 — review round 1 fixes

Worktree `D:/Project/agmag.wt/tui-board-fix1` (branch `feat/tui-board-fix1`, base `feat/tui-board`).
Each item was done test-first: failing test (red) → fix → green.

## 1. Conversation follows the selected agent — `323ead0`

- Test: `board::tests::conversation_follows_selected_agent` (in-file, `src/board/mod.rs`).
  - Red: the test referenced `reload_conversation`, which did not exist.
  - Green: extracted `pub(crate) fn reload_conversation(store, state, data)` in `mod.rs`; it clears
    `data.conversation` first and then queries `messages_involving(agent_keys(selected), 30)`.
    `load_data` uses it, and `run_loop` compares `state.sel_agent` before/after `on_key` and reloads
    before `terminal.draw` when it changed (`Effect::None` branch). A query error sets an error note
    and leaves the conversation empty — the old agent's messages are never shown under the new title.

## 2. Narrow layout as a list — `2b9ecf0`

- Tests: `narrow_lists_every_agent` (70×20, 1 root + 5 children → all 6 labels),
  `narrow_scrolls_to_selected` (70×20, 15 agents, last selected → its row visible),
  `narrow_nothing_lost_at_70x24` (preview size sanity).
  - Red: the old narrow branch reused card rows; at 70×20 only the first depth-0 card fit and the
    five child labels were missing (the buffer showed cards).
  - Green: `draw_narrow_orchestration` renders one line per agent, `2 × depth` spaces of indent,
    `symbol label [KIND age] app/model`, selected line in `theme.selected` + bold, edge label
    accent when live / muted otherwise, `scroll_offset` keeps the selected agent visible. Narrow now
    splits content into orchestration + detail with the fixed-height inbox.

## 3. Scroll to selection in Projects, Inbox, PickProject — `bf60b3e`

- Tests: `inbox_scrolls_to_selected_item` (8 items, last selected, 140×36 → `#8` present with `▸`),
  `projects_scroll_to_selected` (12 projects, last selected), `pick_project_scrolls_to_selected`
  (30 projects, idx 29).
  - Red: all three failed — `Paragraph` was rendered without `.scroll`.
  - Green: `scroll_offset(selected, height, len)` clamps the offset so the selected index stays in
    `[offset, offset + height)`; applied to Projects, Inbox and the `Mode::PickProject` popup.

## 4. Connectors across wrapped rows — `fdbe0d9`

- Test: `wrapped_children_keep_edge_labels` (100×30, 1 root + 6 children, each with an edge → all
  six `KIND-n` labels present).
  - Red: children wrapped to a second row of the same depth; the old `rows.windows(2)` +
    `next.depth > prev.depth` loop skipped that row, so three labels were missing.
  - Green: `card_rows` now puts the two connector rows before every row after the first (wrapped
    continuation included). Connectors are grouped per child row by `flows::parents`, find the row
    that holds the parent (`slot_in_rows`) and draw the bus/drops/labels there; an adjacent parent
    gets the `┬` stem on its bottom border, a wrapped continuation row draws the stem at the left
    margin of its own gap and feeds its own bus. Every child with an edge shows `KIND age`.

## 5. Sticky notes — `261966d`

- Tests: `board::tests::refresh_error_note_clears_on_success` (helper unit test) and
  `board::state::tests::normal_key_clears_note`.
  - Red: `apply_refresh` did not exist; `on_key` never cleared `state.note`.
  - Green: `apply_refresh(data, state, result, refresh_note)` replaces data on success, remembers the
    error text of a failed refresh and clears exactly that note on the next success (action notes are
    untouched); `BoardState::on_key` drops any note at the start of a key handled in `Mode::Normal`
    (popup modes keep it), so action results stay visible until the user presses something.

## Gates

- `cargo test`: 519 passed, 0 failed (all suites; includes 26 `board_view_test`, 6
  `board_runtime_test`, 29 lib `board` unit tests).
- `cargo fmt --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `tests/zz_preview_tmp.rs` was removed before committing (not in the tree/history).

## Preview 140×36

```
==== 140x36
╭ Projects ────────╮╭ agmag · orchestration ─────────────────────────────────────────────────────────────╮╭ worker-b · opencode ───────────╮
│  agmag 5●2       ││╭──────────────────────────────╮                                                    ││app: opencode                   │
│  shop 2●0        │││● agm-ceo claude/opus-5.5     │                                                    ││model: deepseek-v4.1-flash      │
│                  │││doing: cargo test --test boar…│                                                    ││status: working                 │
│                  ││╰───────────────┬──────────────╯                                                    ││task: -                         │
│                  ││          ┌─────┴──────────────┬────────────────────┬────────────────────┐          ││stage: wip→ready→merging→merged │
│                  ││          │REPORT 1m           │ASSIGN 15m          │QUESTION 30s        │          ││ctx: ███░░░░░ 42%               │
╰──────────────────╯│╭─────────┴────────╮ ╭─────────┴────────╮ ╭─────────┴────────╮ ╭─────────┴────────╮ ││tokens: 123k                    │
╭ Status ──────────╮││✓ worker-a openco…│ │● worker-b openco…│ │◐ worker-c openco…│ │✓ reviewer openco…│ ││ ▂▄▁▇▃▅█▂▁▆▄                    │
│✓2 ●2 ◐1 ✗0 -0    │││doing: cargo test…│ │doing: cargo test…│ │doing: cargo test…│ │doing: cargo test…│ ││doing: cargo test --test board_v│
│                  ││╰──────────────────╯ ╰──────────────────╯ ╰──────────────────╯ ╰──────────────────╯ ││last: ← ASSIGN                  │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    │╰────────────────────────────────╯
│                  ││                                                                                    │╭ Messages · worker-b ───────────╮
│                  ││                                                                                    ││worker-b → agm-ceo REPORT 2m    │
│                  ││                                                                                    ││tests red→green for state.rs, 16│
│                  ││                                                                                    ││pass                            │
│                  ││                                                                                    ││agm-ceo → worker-b ASSIGN 20m   │
│                  ││                                                                                    ││Read                            │
│                  ││                                                                                    ││.agents/tasks/tui-board-b.md and│
│                  ││                                                                                    ││do it                           │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
│                  ││                                                                                    ││                                │
╰──────────────────╯╰────────────────────────────────────────────────────────────────────────────────────╯╰────────────────────────────────╯
╭ Inbox (1) ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│▸ #43 QUESTION t9c needs a decision: keep API v1 or move to v2?                                                                           │
│                                                                                                                                          │
│                                                                                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 ↑↓ select  tab panel  ⏎ open  r reply  d done  s send  / filter  t theme  q back
```

## Preview 70×24

```
==== 70x24
╭ agmag · orchestration ─────────────────────────────────────────────╮
│● agm-ceo claude/opus-5.5                                           │
│  ✓ worker-a REPORT 1m opencode/deepseek-v4.1-flash                 │
│  ● worker-b ASSIGN 15m opencode/deepseek-v4.1-flash                │
│  ◐ worker-c QUESTION 30s opencode/deepseek-v4.1-flash              │
│  ✓ reviewer opencode/deepseek-v4.1-flash                           │
│                                                                    │
│                                                                    │
│                                                                    │
│                                                                    │
╰────────────────────────────────────────────────────────────────────╯
╭ worker-b · opencode ───────────────────────────────────────────────╮
│app: opencode                                                       │
│model: deepseek-v4.1-flash                                          │
│status: working                                                     │
│task: -                                                             │
│stage: wip→ready→merging→merged                                     │
╰────────────────────────────────────────────────────────────────────╯
╭ Inbox (1) ─────────────────────────────────────────────────────────╮
│▸ #43 QUESTION t9c needs a decision: keep API v1 or move to v2?     │
│                                                                    │
│                                                                    │
╰────────────────────────────────────────────────────────────────────╯
 ↑↓ select  tab panel  ⏎ open  r reply  d done  s send  / filter  t th
```
