---
id: tui-mouse-overview
tier: mid
skills: test-driven-development
branch: feat/tui-mouse-overview
worktree: D:/Project/agmag.wt/tui-mouse-overview
---

# T3 — Mouse + drag-resize on the `agm tui` overview

Read the spec first: `docs/superpowers/specs/2026-11-24-tui-layout-mouse-design.md` (Intent, Goals, **Anti-goals**,
Design §3, §4 and the **Overview** bullet of §5). T1 (layout + overview hit map) and T2 (mouse capture + board mouse)
are merged on main. You implement §4 and the overview part of §5. Do not change the board's mouse behaviour.

Code you build on:
- src/board/global.rs: `HitMap`, `hit(map, x, y)` (border > row > panel), `RowTarget`, `BorderId`
  (`TopPairVertical`, `Horizontal(i)`, `OutputTop`), `HitPanel`, `GlobalState` (`focus`, `sel_terminal`,
  `sel_inbox`, `scroll`, `output_scroll`, `popup`, `hit_map()`), `GEffect`.
- src/board/global_view.rs: `section_layout`, `split_messages`, `pair_column_widths`, `fit_heights`,
  `draw_sections`, `draw_stacked`, `MIN_SECTION_H`.
- src/board/state.rs `BoardState::on_mouse` / `on_double_click` / `DOUBLE_CLICK_MS = 400`: copy the pattern
  (clock passed in, `last_click: Option<(target, ms)>`, a double-click consumes the pair).
- src/board/app.rs `handle_mouse`: `Screen::Global => Next::Skip` is the hook you replace; `handle_global_key`
  shows how a `GEffect` becomes an action (OpenBoard, SaveTheme → `store.set_setting`).

## Steps (TDD: red → code → green → commit, one commit per step)
1. **Mouse on the overview (pure).** `GlobalState::on_mouse(&mut self, ev: MouseEvent, data, now_ms) ->
   (bool /*changed*/, GEffect)` (or a small outcome enum): returns "no change" when nothing changed so the loop does
   not redraw. Uses the hit map of the last frame. Act only on `Down(Left)`, `Drag(Left)`, `Up(Left)`,
   `ScrollUp/Down`; ignore everything else. Ignore ALL mouse events while `popup.is_some()`.
   - click a panel → focus it; click a Terminals/Inbox row → focus the panel and select that row; click a
     Messages row → focus Messages; click the command line (below the sections) → focus the command line.
   - wheel → the panel under the pointer: move the selection in Terminals/Inbox, scroll Messages; Output overlay
     scrolls `output_scroll`. Wheel outside any panel → no change.
   - double-click (same target within 400 ms) a terminal row → `GEffect::OpenBoard(address)` (same as ⏎);
     double-click an inbox row → pre-fill the command line with `reply #<id> ` and focus it — nothing is
     dispatched.
   - While the Output overlay is shown, only the Output panel and its border react; clicks elsewhere are ignored.
2. **Layout overrides (pure).** A `LayoutOverride` type: optional top-pair width fraction and per-boundary height
   fractions keyed by `BorderId::Horizontal(i)`. Applied inside `section_layout`/`draw_sections` AFTER content
   fitting, clamped so every panel keeps ≥ `MIN_SECTION_H` rows and ≥ 20 columns, and recomputed from ratios on
   every frame (a window resize never yields nonsense or hides a panel). Compact text format with
   `to_string`/`parse`; `parse` of garbage → `None` (ignored). Unit tests: round trip, garbage, clamping at tiny
   sizes, a stale index for a section no longer in the view is ignored.
3. **Drag-resize.** `Down(Left)` on a border starts a drag (remember `BorderId` + start); `Drag(Left)` updates the
   override live; `Up(Left)` ends it and returns an effect to persist (e.g. `GEffect::SaveLayout(String)`).
   Double-click a border → reset that boundary. Command `layout reset` on the overview clears all overrides and
   persists. Persist in the settings key `tui_layout` (like `theme::SETTING_KEY`); load it at startup in app.rs
   next to the theme. `OutputTop` is not resizable (ignore drags on it).
4. **Wire it.** `handle_mouse` `Screen::Global` → `state.on_mouse(...)`; map the effect like `handle_global_key`
   does; return `Next::Skip` when nothing changed, `Next::Redraw` otherwise. Tests at the app.rs level with
   synthetic events (there are helpers like `mouse_ev` in app.rs tests).
   **Anti-goal tests:** no mouse sequence on the overview (clicks, double-clicks, drags, wheel on every target,
   with and without a popup open) ever produces `GEffect::Run`, a dispatch, send/reply/done/kill, or confirms a
   popup.
5. **Docs.** Overview `?` help mentions click/wheel/double-click/drag borders, `layout reset`, Shift+drag to select
   text, `AGM_MOUSE=0`. README.md and README_vi.md TUI section: one short sentence each. CHANGELOG.md
   `[Unreleased]` → `### Thêm mới`: one Vietnamese bullet.

## Constraints
- Only this worktree/branch; no push/merge/branch switch.
- Keep line endings (CHANGELOG.md, src/main.rs, src/tui.rs CRLF; src/board/* and Cargo.toml LF). Run
  `git ls-files --eol <file>` before each commit; never commit `w/mixed`.
- Log each step in `.agents/log/tui-mouse-overview.md`; commit after EVERY step.
- Final gates clean and recorded: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  (report binaries and passed/failed totals).
- Finish with `DONE tui-mouse-overview <HEAD sha>` or `BLOCKED: <reason>`.

## Round 2 — read this before starting (an earlier attempt was rejected)

The first attempt (kept on branch `attic/tui-mouse-overview-beam`, do NOT reuse it) was rejected because: all steps
in one commit; zero new tests; `cargo fmt --check` failing while the log claimed it clean; three duplicate layout
fields; a guessed border→section mapping; existing comments deleted. The CEO re-runs every gate and counts tests.

Hard rules:
- Each step = at least one NEW `#[test]` written first and seen failing, then code, then `cargo fmt`, then commit.
  Main has 745 tests; your final count must be clearly higher. Record per-step test names in the log.
- ONE layout field on `GlobalState` (e.g. `layout: LayoutOverride`) plus the drag state. No duplicates.
- Do not delete or rewrite existing comments/tests that are unrelated to your change.
- Help footer must still fit 100 columns: keep the footer line as it is; put the mouse/layout text in the `?`
  help popup/output instead.

Exact semantics (no guessing):
- `BorderId::Horizontal(i)` is the boundary directly ABOVE group `i` of the stacked layout as `draw_stacked`/
  `split_messages` push it (read those functions to see which index each pushed border gets; the Messages boundary
  is `Horizontal(visible_sections(&data.view).len() - 1)`). Write a test that draws a frame and asserts each
  `Horizontal(i)` rect sits at the bottom border of the group above it — then build the mapping from that.
- `LayoutOverride.heights[i]` = fraction of `main.height` given to the group directly ABOVE `Horizontal(i)` (the top
  pair counts as one group when wide). Dragging `Horizontal(i)` to row `y` sets that group's height to
  `y - group_top` (+1 for the border row as needed), as a fraction of `main.height`. Messages stays the flexible
  section and absorbs the difference; every group keeps ≥ `MIN_SECTION_H` rows and Messages ≥ `MESSAGES_MIN`.
- `top_pair_width` = fraction of `main.width` for the LEFT panel of the top pair; both panels ≥ 20 columns.
- Tests to include at least: parse/to_string round trip and garbage; clamping at tiny sizes (e.g. 40x10) and after a
  window shrink; drag `TopPairVertical` and a `Horizontal(i)` through `on_mouse` changes the next frame's rects as
  expected; double-click a border resets only that boundary; `layout reset`; mouse ignored while a popup is open;
  Output overlay case; anti-goal test over every hit target (no `GEffect::Run`, no dispatch, popup never
  confirmed); `on_mouse` returns "no change" for `Moved`, right/middle button, wheel outside panels.
