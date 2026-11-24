# tui-w1 log — board view: CEO blocks, 4-line centred cards, colours, Status column

## Red → Green
1. Added failing tests first:
   - `tests/board_view_test.rs`: `card_lines_are_centred`, `card_shows_model_and_task`, `card_colours_by_role_and_line`, `two_ceos_share_top_row`, `workers_stay_under_their_ceo`, `orphan_worker_in_no_ceo_block`, `ceo_blocks_stack_when_narrow`, `board_has_no_projects_panel`, `status_panel_one_status_per_line_hides_zero`, `detail_shows_task`, `draw_in_respects_area`.
   - `src/board/state.rs`: `tab_skips_projects_panel`.
   - Initial run failed as expected (`draw_in` unresolved, `tab_skips_projects_panel` failed).
2. Implemented changes in `src/board/view.rs` and `src/board/state.rs`:
   - 4-line centred cards (`CARD_H = 6`): `<symbol> <label>`, model/app/`-`, `doing:`/`stage:`/`-`, `‹task›`/`-`.
   - Role/line colours (`theme.accent` + bold for CEOs, `theme.title` for workers, `theme.muted` for model, default for doing, `theme.selected` for task, all `theme.muted` for exited).
   - CEO blocks via `layout::ceo_blocks` and `layout::block_widths`, stacking vertically when `None`, `no CEO` block without connectors, and `── no CEO` separator in narrow list.
   - Removed `Projects` panel from board; left column is `Status` only (1 status per line, zero counts hidden, `-` when empty, width = longest line + borders, min 14); `Tab`/`BackTab` skip `Panel::Projects`.
   - Detail panel `task:` line renders `AgentView.task` (`-` when `None`).
   - Added `pub fn draw_in(...)` and wired `draw` to call it with `frame.area()`.
3. All tests, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` pass.

## Final text previews

### 160×40 (2 CEOs + orphan)
```
╭ Status ────╮╭ agmag · orchestration ───────────────────────────────────────────────────────────────────────────────────────╮╭ worker-b · opencode ───────────╮
│● working 3 ││╭──────────────────────────────╮                       ╭────────────────────────╮ no CEO                      ││app: opencode                   │
│◐ blocked 1 │││          ● agm-ceo-1         │                       │       ● agm-ceo-2      │ ╭──────────────────────────╮││model: deepseek-v4.1-flash      │
│✓ done 2    │││           opus-5.5           │                       │       sonnet-4.5       │ │        ✓ orphan-w        │││status: working                 │
│            │││doing: cargo test --test boar…│                       │doing: cargo test --tes…│ │    deepseek-v4.1-flash   │││task: tui-w2                    │
│            │││          ‹tui-board›         │                       │        ‹api-v2›        │ │doing: cargo test --test …│││stage: wip→ready→merging→merged │
│            ││╰───────────────┬──────────────╯                       ╰────────────┬───────────╯ │         ‹cleanup›        │││ctx: ███░░░░░ 42%               │
│            ││             ┌──┴───────────────────────┐                           ┼             ╰──────────────────────────╯││tokens: 123k                    │
│            ││             │REPORT 1m                 │ASSIGN 15m                 │QUESTION 30s                             ││ ▂▄▁▇▃▅█▂▁▆▄                    │
│            ││╭────────────┴───────────╮ ╭────────────┴───────────╮  ╭────────────┴───────────╮                             ││doing: cargo test --test board_v│
│            │││       ✓ worker-a       │ │       ● worker-b       │  │       ◐ worker-c       │                             ││last: ← ASSIGN                  │
│            │││   deepseek-v4.1-flash  │ │   deepseek-v4.1-flash  │  │   deepseek-v4.1-flash  │                             ││                                │
│            │││doing: cargo test --tes…│ │doing: cargo test --tes…│  │doing: cargo test --tes…│                             ││                                │
│            │││        ‹tui-w1›        │ │        ‹tui-w2›        │  │       ‹api-auth›       │                             ││                                │
│            ││╰────────────────────────╯ ╰────────────────────────╯  ╰────────────────────────╯                             ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              │╰────────────────────────────────╯
│            ││                                                                                                              │╭ Messages · worker-b ───────────╮
│            ││                                                                                                              ││worker-b → agm-ceo-1 REPORT 2m  │
│            ││                                                                                                              ││tests red→green for state.rs, 20│
│            ││                                                                                                              ││pass                            │
│            ││                                                                                                              ││agm-ceo-1 → worker-b ASSIGN 20m │
│            ││                                                                                                              ││Read .agents/tasks/tui-w1.md and│
│            ││                                                                                                              ││do it                           │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
│            ││                                                                                                              ││                                │
╰────────────╯╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────╯╰────────────────────────────────╯
╭ Inbox (1) ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│▸ #43 QUESTION t9c needs a decision: keep API v1 or move to v2?                                                                                               │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 ↑↓ select  tab panel  ⏎ open  r reply  d done  s send  / filter  t theme  q back
```

### 100×30
```
╭ agmag · orchestration ─────────────────────────────────────────╮╭ worker-b · opencode ───────────╮
│╭──────────────────────────────╮                                ││app: opencode                   │
││          ● agm-ceo-1         │                                ││model: deepseek-v4.1-flash      │
││           opus-5.5           │                                ││status: working                 │
││doing: cargo test --test boar…│                                ││task: tui-w2                    │
││          ‹tui-board›         │                                ││stage: wip→ready→merging→merged │
│╰───────────────┬──────────────╯                                ││ctx: ███░░░░░ 42%               │
│               ┌┴──────────────────────────────┐                ││tokens: 123k                    │
│               │REPORT 1m                      │ASSIGN 15m      ││ ▂▄▁▇▃▅█▂▁▆▄                    │
│╭──────────────┴──────────────╮ ╭──────────────┴──────────────╮ ││doing: cargo test --test board_v│
││         ✓ worker-a          │ │         ● worker-b          │ ││last: ← ASSIGN                  │
││     deepseek-v4.1-flash     │ │     deepseek-v4.1-flash     │ ││                                │
││doing: cargo test --test boa…│ │doing: cargo test --test boa…│ │╰────────────────────────────────╯
││          ‹tui-w1›           │ │          ‹tui-w2›           │ │╭ Messages · worker-b ───────────╮
│╰─────────────────────────────╯ ╰─────────────────────────────╯ ││worker-b → agm-ceo-1 REPORT 2m  │
│                                                                ││tests red→green for state.rs, 20│
│╭──────────────────────────────╮                                ││pass                            │
││          ● agm-ceo-2         │                                ││agm-ceo-1 → worker-b ASSIGN 20m │
││          sonnet-4.5          │                                ││Read .agents/tasks/tui-w1.md and│
││doing: cargo test --test boar…│                                ││do it                           │
││           ‹api-v2›           │                                ││                                │
│╰──────────────────────────────╯                                ││                                │
│                                                                ││                                │
╰────────────────────────────────────────────────────────────────╯╰────────────────────────────────╯
╭ Inbox (1) ───────────────────────────────────────────────────────────────────────────────────────╮
│▸ #43 QUESTION t9c needs a decision: keep API v1 or move to v2?                                   │
│                                                                                                  │
│                                                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
 ↑↓ select  tab panel  ⏎ open  r reply  d done  s send  / filter  t theme  q back
```
