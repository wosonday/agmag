# TUI overview: content-fitted layout, resizable panels, mouse

Date: 2026-11-24 · Requested by the user ("phần terminals tui nên custom lại cho gọn gàng (phù hợp với tỷ lệ của
khung)…", "tính toán lại tỷ lệ các phần", chose approach C, "hãy gọi các model").

## Intent

The full-screen `agm tui` overview wastes space: with real data (11 terminals, 0 inbox) at 140×40 the top pair
(Quota | Terminals) always takes 3/5 of the height (Terminals shows 11 rows in 20), an empty Inbox takes 14 rows,
Messages is fixed at 8 rows; the Terminals `task`/`last`/`st` columns are empty for every row yet take ~60 cells at
200 columns. The user wants proportions that follow the content, wants to adjust them by dragging with the mouse,
and wants mouse interaction in the TUI generally.

## Goals

1. **Content-fitted heights (overview).** Each section asks for the rows it needs; space nobody needs goes to the
   section that benefits most. No large blank areas.
2. **Compact Terminals panel.** Hide columns that are empty (`-`) for every row; agents first, plain shells last
   and dimmed; title shows counts.
3. **Resizable panels (overview).** Drag the boundaries with the mouse; the chosen proportions are remembered.
4. **Mouse** on the overview and on the project board: click to focus/select, wheel to scroll, double-click to
   open.

## Non-goals

- `agm tui --plain` (line UI) is unchanged; no mouse there.
- Resizing the project board's three columns (later round).
- Drag-and-drop of rows between panels; context menus; hover effects.

## Anti-goals (would be a failure even if it works)

- **The mouse must never trigger a destructive or sending action**: no kill/interrupt/cleanup/done/reply/send by
  mouse. Mouse only focuses, selects, scrolls, opens a view, or pre-fills the command line (the user still presses
  ⏎). Confirmation popups (`y` + ⏎) stay keyboard-only; mouse events are ignored while a popup is open.
- Breaking keyboard-only use: every existing key keeps its meaning; nothing requires a mouse.
- Leaving the console in mouse-capture mode after exit or panic.
- A resize state that hides a panel completely or survives a window resize as nonsense (store ratios, clamp).

## Design

### 1. Content-fitted heights (src/board/global_view.rs)

- Each section reports `desired_rows` (borders included): Quota = quota lines + 2; Terminals = header + rows + 2;
  Inbox = items + 2, **min 3** when empty (title `Inbox (0)`; one muted line or none); Messages = flexible.
- Pure function (unit-testable, no Frame): `fit_heights(desired: &[u16], flex: usize, total: u16, min: u16) ->
  Vec<u16>`: give each section `min(desired, fair cap)`, then the surplus to the flexible section (Messages by
  default; Terminals if it still needs rows); if total < sum of desired, shrink largest first, never below `min`
  (3 = border + 1 row).
- ≥ 120 columns: the top pair's height = max(desired Quota, desired Terminals), capped (default ≤ 60 % of the
  sections area), the rest stacked below with the same fitting. < 120 columns: one stack, same fitting.
- Messages is no longer a fixed 8 rows: it is the default flexible section (min 4 rows).

### 2. Compact Terminals (src/board/global_view.rs `draw_terminals`)

- A column whose value is `-` for every row is not shown (`task`, `last`, `st`, `app`, `model`, `ctx`, `tok`).
- Row order: terminals with an agent app or a linked session first (current order kept within the group), then
  plain shells (app is a shell such as `pwsh`/`bash`/`cmd`/`sh` and no session) dimmed with `theme.muted`.
- Title: `Terminals (N agents · M shells)` (omit the part that is 0).
- The flexible width goes to `name`/`model` before `task` when `task` is shown.
- Selection still uses the terminal address (unchanged behaviour of ↑↓/⏎).

### 3. Hit map (shared by mouse and resize)

- Each draw records what it drew where: `HitMap { panels: Vec<(Rect, Section)>, rows: Vec<(Rect, RowTarget)>,
  borders: Vec<(Rect, BorderId)> }`, rebuilt every frame and kept in the screen state (overview: `GlobalState`;
  board: its state). `RowTarget` = terminal address / inbox id / message index / agent address / project key.
- Hit testing is a pure function over the map (unit-tested); draw code only pushes rects.

### 4. Resizable panels (overview)

- Draggable boundaries: the vertical divider between the two top panels (≥ 120 cols) and each horizontal boundary
  between stacked sections (top pair / next / … / Messages).
- Dragging sets an override stored as **ratios** (e.g. top-pair width fraction, per-boundary height fraction),
  clamped so every panel keeps ≥ 3 rows / ≥ 20 columns; content fitting still applies inside the override.
- Persist in the DB settings key `tui_layout` (like the theme key), compact text format, invalid values ignored.
- Reset: double-click on a boundary resets that boundary; command `layout reset` clears all overrides.

### 5. Mouse

- Enable: `EnableMouseCapture` when entering the console, `DisableMouseCapture` when leaving **and** in the panic
  hook path (src/board/mod.rs `enter_console`/`leave_console`). `AGM_MOUSE=0` disables capture (escape hatch,
  like `AGM_ASCII`). Text selection in Windows Terminal then needs Shift+drag: mention it in `?` help and README.
- Windows emits press and release; act on `MouseEventKind::Down(Left)`, `Drag(Left)`, `Up(Left)`, `ScrollUp/Down`
  only.
- Double-click: two `Down(Left)` on the same target within 400 ms.
- **Overview:** click on a panel → focus it; click on a Terminals/Inbox row → select it; wheel → scroll the panel
  under the pointer (move the selection in Terminals/Inbox, scroll Messages/Output); double-click a terminal row
  → same as ⏎ (open the board for its project); double-click an inbox row → pre-fill `reply #<id> ` in the command
  line (focus moves there, nothing is sent); click on the command line → focus it.
- **Board:** click on a column/panel → focus it; click on an agent card / project row / inbox row → select it;
  wheel → move the selection / scroll the conversation; double-click a project → open it (same as ⏎ in the
  picker); double-click an inbox row → pre-fill reply like `r` does (nothing sent).
- Mouse events while a popup/confirm is open are ignored.

## Tracks (parallel where files do not overlap)

- **T1 layout** (global_view.rs, global.rs layout parts): §1, §2, §3 for the overview (record rects; no mouse yet).
- **T2 board mouse** (board/mod.rs console enter/leave + panic path, board event loop, board/state.rs, board/view.rs,
  the board branch of app.rs): §5 capture + board part, §3 for the board.
- **T3 overview mouse + resize** (after T1 and T2 merge): §4 and the overview part of §5.

## Left to the builder

Exact caps (60 % top pair, fair share), how muted shells look per theme, the compact `tui_layout` text format,
the hit-map type names, and test structure — keep them small and tested.
