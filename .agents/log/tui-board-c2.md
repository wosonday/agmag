# tui-board-c2 log (view polish round, follow-up to Plan Task 3)

## 2026-11-24

### State before
- Worktree: `D:\Project\agmag.wt\tui-board-c2`, branch `feat/tui-board-c2`
  (base `94d8578 merge: board ratatui rendering (worker c, deepseek-v4.1-flash)`).
- Task: `docs/superpowers/plans/2026-11-23-tui-board.md` → Task 3 fall-out:
  card width, real connectors, English inbox kinds, message labels.
- Files allowed: `src/board/view.rs`, `tests/board_view_test.rs`, this log. No pub signature changed.

### Step 1: failing tests (RED)
Added 5 tests to `tests/board_view_test.rs`, plus helpers `card_bounds` / `card_centre` (locate a card's
borders from a label on its first inner line, then its geometric centre):
- `cards_grow_to_fill_width` — 140×36, parent `agm-ceo` + 4 children; `agm-ceo claude` and
  `doing: cargo test` must appear untruncated.
- `connector_joins_parent_and_children` — `┬` on the parent card's bottom border, `│` on each drop line,
  `┴` on each child's top border (same column as the drop), `─` bus, `REPORT` label on the drop row.
- `connector_ascii_uses_plus_pipe_dash` — same tree with `+`, `-`, `|` and no box-drawing glyphs.
- `inbox_kind_is_english` — inbox `kind: "HỎI"` renders `QUESTION`, never the stored code.
- `messages_use_agent_labels` — `t1 → t2` renders `alpha → beta`; an address without an agent stays as-is.

`cargo test --test board_view_test` → 0 of 5 new tests pass (14 old pass); the RED run also showed the
old fixed-18 card text clipped (`agm-ceo claude` cut, `HỎI` in the inbox, raw addresses in Messages).

### Step 2-3: implementation (GREEN)
`src/board/view.rs`:
- **Cards size to the row.** `card_rows` groups agents by depth and wraps only when a card would be
  narrower than 16 (`(inner_width + 1) / 17` per row). `row_card_width` = `min(32, (inner - gaps) / n)`
  with a minimum of 16 (and of the panel width), one-column gaps. Regrouped the panel into a two-pass
  layout: place `CardSlot`s row by row (reserving two connector rows at each depth increase), draw the
  cards, then draw connectors from `flows::parents`.
- **Line content.** Line 1 `symbol label app/model`, truncated with `…` by display width
  (`unicode_width`, already a dependency). Line 2 `doing: <summary> <age>` when it fits, else
  `doing: <summary>` (age dropped before the summary is cut), else the summary truncated with `…`;
  `stage: <stage>` truncated likewise.
- **Real connectors.** Per parent: `┬` on its bottom border, a bus on the next row (`┌ … ┐`, child
  centres `┬`, parent stem `┴`, `┼` when the stem and a drop share a column), then a drop row with `│`
  above each child (its top border marked `┴`) and the edge label `KIND age` beside the drop. Drop,
  border mark and label are `theme.accent` when `flows::is_live`, `theme.muted` otherwise; children with
  no edge get a muted drop and no label. Labels are clipped to the gap before the next drop/panel edge.
  ASCII mode uses `+ - |` only (`ConnGlyphs`).
- **Inbox kinds.** The inbox row (and the `ShowItem` popup header) map the stored code through
  `actions::kind_to_english`, like the Messages panel.
- **Message labels.** `agent_label` maps an address to the project agent's `label`; unknown addresses
  stay as-is. Filtering still matches the raw address as well as the displayed label.

`cargo test --test board_view_test` → 19 passed (14 old + 5 new).

### Step 4: gates
- `cargo test` → 500 passed, 0 failed (39 suites; 478 before this task's 5 new tests + 17 others in
  suites that were not run by the earlier track log's baseline — board_view is 19, was 14).
- `cargo fmt --check` → clean.
- `cargo clippy --all-targets -- -D warnings` → clean.
- Preview tool (`tests/zz_preview_tmp.rs`, temporary copy, deleted before commit) checked at
  140×36 / 100×30 / 70×24.

### Final 140×36 preview

```
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

### Open questions
- At 70×24 the narrow layout gives the orchestration panel 8 inner rows; with the required two
  connector rows (parent 4 + bus/drop 2 + children 4) the child cards are clipped to their top/bottom
  borders. This is a height budget issue in narrow mode, not a connector bug; the 100×30 and 140×36
  layouts are unaffected. Giving orchestration more height in Narrow would be a separate layout tweak.
- Children that wrap onto an extra row at the same depth (e.g. the 4th child at 100×30) are not
  connector-linked: the bus only joins a parent row to the immediately following deeper row. Same
  limitation as before; routing to a wrapped row would need a taller routing channel than the 2 rows
  the task specifies.
- `stage:` is still the line-2 fallback when `doing` is absent (kept from Task 3); the task text says
  "or stage", which this satisfies.
