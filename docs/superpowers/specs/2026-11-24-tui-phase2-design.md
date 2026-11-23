# TUI Phase 2: whole `agm tui` on ratatui — design

Status: draft for approval (2026-11-24); builder check done (opencode/iceman-free), answers folded in as my calls. Follows `2026-11-23-tui-board-design.md` § "Phase 2".

## Intent

The user moved the project view to a full-screen ratatui board (`agm board`, merged 74788b4) and asked to move the
whole `agm tui` onto the same framework, reusing code as much as possible, so the tool looks like one coherent modern
TUI instead of a line REPL that jumps into a full-screen board and back. They rejected plain text output before ("not
visual enough"), so the sections they look at every day must become real widgets.

## Decisions taken with the user

- From the board spec: one ratatui app with two screens (global overview, project board); `cd <project>` switches
  screens without leaving raw mode; a command bar that reuses `dashboard::parse_command` / `TuiCmd` and the existing
  action dispatch so every command keeps its exact semantics; `kill` / `interrupt` / `cleanup` keep the human-only rule
  and a typed confirmation (popup: type `y` + `⏎`, anything else cancels); the current line UI stays as
  `agm tui --plain`; Phase 2 gets its own design + review round.
- **Approach 3 (hybrid), chosen 2026-11-24:** native ratatui widgets for the four sections used daily — Quota,
  Terminals, Inbox, Messages; the other three — Usage, Budgets, System — reuse today's text renderers inside titled
  panels.

## Goals

- `agm tui` (no flag) opens the full-screen app on the global screen; `agm tui --plain` runs today's line UI unchanged.
- Global screen: up to 3 section panels chosen by `view` (same `Section` list, same `MAX_VIEW = 3`, same saved setting),
  laid out like the board (titled rounded panels, theme colours, key-hint/note footer), plus the always-visible command
  bar.
- Native widgets:
  - **Quota:** one row per provider window: provider, window (`5h`/`7d`/`mo`), a `Gauge`/bar of the percent LEFT
    (green > 50 %, yellow 20–50 %, red < 20 %), `resets in XhYYm` / `reset ready`, a `stale` badge when stale.
  - **Terminals:** `Table` with a selectable row: status symbol (board symbols), name/address, app, model/effort,
    status, task, context % (bar), tokens, last message kind + direction, place. Selected row → `⏎` opens the board of
    that terminal's project; `s` prefills `send <name> ` in the command bar.
  - **Inbox:** `Table`, selectable: `#id`, kind (English), terminal, age, text (one line, ellipsis). `⏎` shows the full
    item in a popup; `r` prefills `reply <#> `; `d` prefills `done <#>`.
  - **Messages:** list, newest first: `from → to KIND age` + wrapped text, agent names instead of addresses when known.
- Bridged sections (Usage, Budgets, System): the body that `render_section` produces today (without its own section
  header line, which the panel title replaces), converted from the dashboard's ANSI colours to ratatui styles and drawn
  in a scrollable panel.
- Project board = the existing `src/board` screen; entered with `cd <project>` (or `⏎` on a terminal row); `q`/`esc`
  in the board returns to the global screen (not out of the app). `agm board [project]` keeps working standalone.
- Command bar at the bottom: typed text goes there whenever no popup is open and the focus is not on a selectable
  panel; `⏎` runs it through `parse_command` and the shared dispatcher; result → footer note (ok / warn / error colours
  as today). Up/down in the command bar recall the last 50 commands of this session.
- Confirmation popup for `kill`, `interrupt`, `cleanup <sel>`: shows the same text `confirmation_note` builds today;
  only `y` then `⏎` confirms; any other input cancels with the same "cancelled" / "cleanup cancelled" notes.
- Theme: the board's three palettes and saved `board.theme` setting apply to the whole app; `t` cycles (when focus is
  on a panel, not the command bar).

## Non-goals

- No change to what any command does, its arguments, its errors, or the data each section shows.
- No mouse support; no new sections; no change to `agm board` behaviour except that it is now also a screen of the app.
- No rewrite of the Usage / Budgets / System renderers in this phase.

## Anti-goals

- Weakening safety: agents and non-TTY callers must still be refused (`detect_caller` once at start, fail-closed);
  `Caller::User { confirmed: true }` is only ever built after the popup's `y` + `⏎`.
- A broken terminal after exit, error or panic (reuse the board's `TermGuard` + panic hook).
- Inventing data: unknown values stay `-`.
- Two divergent command implementations: the plain loop and the app must call the same dispatch code.

## Constraints (from the code today)

- `src/tui.rs` and `src/ops.rs` live in the **binary** crate; `src/board`, `src/dashboard.rs`, `src/actions.rs` in the
  library. The command dispatch in `tui::run` (≈ 300 lines, one big `match parse_command(&line)`) mixes state
  (`note`, `view`, `pending`, `cleanup_list`, `cleanup_selected`, `scope`, `shown`) with actions.
- `render_section` is private and returns ANSI text built with `paint(style, text)` and a fixed set of SGR codes; it
  already sanitizes untrusted text before painting.
- `ratatui 0.30` with crossterm 0.29 is already a dependency. Windows emits key Press and Release events.
- The board already provides `TermGuard`, panic hook, `require_human`, themes, `BoardState` / `on_key` / `Effect`,
  `load_data`, `apply_effect`, `view::draw`.
- Gates: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, harness lint.

## Approaches considered

1. Wrap every section's existing ANSI text in panels — least work, identical content, but still text inside boxes
   (what the user called not visual enough).
2. Rewrite all seven sections as native widgets — best look, most work, more duplication with `--plain`.
3. **Hybrid (chosen):** native widgets for Quota, Terminals, Inbox, Messages; bridge the existing text for Usage,
   Budgets, System. Daily sections look right now; the phase stays bounded; the bridged ones can be ported later.

## Design

### Shared command dispatch (refactor first, behaviour-neutral)

Extract the body of the `match parse_command(..)` in `tui.rs` into a dispatcher both front ends call:

```rust
// src/tui_cmd.rs (binary crate, next to tui.rs; needs crate::ops)
pub struct CmdState { view, scope, pending, cleanup_list, cleanup_selected, roots_cache, .. }
pub enum Outcome {
    Note(NoteKind, String),
    Shown(String),               // `show`, `cd` listing: text block to display
    NeedsConfirm(String),        // prompt text; the next input line is the answer
    OpenBoard(ProjectKey),       // `cd <project>` found
    Quit,
    None,
}
pub fn dispatch(line: &str, st: &mut CmdState, ctx: &Ctx) -> Outcome;  // handles pending confirmations too
```

`--plain` keeps its loop and rendering and calls `dispatch`; its observable behaviour is unchanged (the existing
`tui.rs` tests and a new table test over `dispatch` prove it). `OpenBoard` in `--plain` calls `board::run` exactly as
today.

### App (library, reusing `src/board`)

- `src/board/app.rs`: `pub enum Screen { Global, Board }`, an `AppState { screen, global: GlobalState, board:
  BoardState, cmdline: CmdLine, popup: Option<Popup>, note, theme }`, and `pub fn run_app(store, cfg, hc, refresh,
  scan, dispatch: &mut dyn FnMut(&str) -> Outcome) -> Result<()>` — the dispatcher is injected from the binary crate
  so the library does not depend on `ops`.
- `src/board/global.rs`: `GlobalState` (focused panel, selected terminal / inbox row, scroll per bridged panel) with a
  pure `on_key` like `BoardState::on_key`, and `GlobalData` built from `dashboard::snapshot` each refresh.
- `src/board/global_view.rs`: draws the chosen sections: native widgets for the four, bridge for the three.
- `src/board/ansi.rs`: `fn ansi_to_text(s: &str) -> Text<'static>` — handles only the SGR (`ESC [ … m`) codes the
  dashboard emits (reset, bold, dim, the 8/16 foreground colours it uses); any other escape sequence is dropped.
  `dashboard.rs` gets `pub fn render_section_body(s, sec, width) -> String` (today's body without the header line).
- Board screen: `board::run`'s loop body becomes a screen of the app; `q`/`esc` returns `Screen::Global` when the
  board was entered from the app, and quits when started by `agm board`.

### Layout (global screen)

```
╭ Quota ─────────────────────╮╭ Terminals ─────────────────────────────────────────╮
│ claude   5h ██████░░  76%  ││▸ ● t7p agm-ceo   claude  opus/high  working   42%  │
│ codex    7d ████████ 100%  ││  ✓ t9b worker-b  opencode flash      idle       -   │
╰────────────────────────────╯╰────────────────────────────────────────────────────╯
╭ Inbox (2) ─────────────────────────────────────────────────────────────────────────╮
│▸ #43 QUESTION t9c  5m  needs a decision: keep API v1 or move to v2?               │
╰───────────────────────────────────────────────────────────────────────────────────╯
╭ Messages ─────────────────────────────────────────────────────────────────────────╮
│ worker-b → agm-ceo REPORT 2m  tests red→green for state.rs, 16 pass               │
╰───────────────────────────────────────────────────────────────────────────────────╯
 agm> _                                                     tab panels · ? help · q quit
```

- Messages is pinned (as today: it never counts toward the 3 `view` sections) at the bottom, 6 rows.
- ≥ 120 columns: first two chosen sections side by side, third full width; < 120: all stacked; < 80 or < 20 rows:
  stacked, each panel height-capped, command bar always visible.

### Keys

- Focus starts on the command bar. `tab` / `shift+tab` cycle panels ↔ command bar. In a panel: `↑↓`/`jk` select or
  scroll, `⏎` / `r` / `d` / `s` as listed in Goals, `t` theme, `?` help, `q` = `quit`. In the command bar every
  printable key is text; `esc` clears it; `⏎` runs it.
- `ctrl+c` anywhere quits (restores the terminal).

## Testing

- `dispatch`: table test over every `TuiCmd` (success, error and confirmation paths for kill / interrupt / cleanup,
  `cd` / `cd -t` / `cd ..`, `view`, `policy`) with a fake `Hc` and temp store — proves `--plain` and the app share
  behaviour.
- `ansi_to_text`: colours/bold map; unknown escapes (OSC title, cursor moves) dropped; untrusted text stays inert.
- `global::on_key`: focus cycling, command-bar editing and history, prefills, confirmation popup (`y⏎` → confirm,
  anything else → cancel), Release events ignored.
- `global_view`: `TestBackend` at 140×40, 100×30, 70×20: titles, gauges, selected rows, `-` for unknowns, hostile
  text inert and clipped, no panic at 1×1.
- `--plain`: existing `tui.rs` tests unchanged and green.
- Manual: Windows Terminal + herdr pane at 140 and 90 columns: `kill` confirmation, `cleanup` two-step, `cd` →
  board → `q` → global, `view`, `policy`, `--plain`.

## Builder-check answers (my calls, 2026-11-24)

1. **Text output (`show <#>`, `cd` list, `cleanup` candidate list) → an Output panel.** `Outcome::Shown(String)` is
   drawn in an "Output" panel that overlays the lower half of the section area, above Messages. It does **not** take
   the keyboard: the command bar stays focused, so after `cleanup` the user reads the numbered list and types
   `cleanup 1,2` / `cleanup all` as today, then answers the confirmation popup. The panel closes on the next command or
   `esc`, and scrolls with `PgUp`/`PgDn`. Inbox `⏎` reuses it (it runs `show <#>`).
2. **`cd` variants in the app.** `cd` → project list in the Output panel; `cd <project>` → board screen; `cd ..` / `cd /`
   → note "already at all projects"; `cd -t <project>` → warn note "text project view: use `agm tui --plain`" (the text
   scope exists only in `--plain`). `show <#>` → Output panel.
3. **Shared types live in the library, dispatch in the binary.** `src/board/cmd.rs` (library) defines
   `pub enum Outcome { Note(NoteKind, String), Shown(String), NeedsConfirm(String), OpenBoard(ProjectKey), Quit, None }`,
   `pub enum NoteKind { Ok, Warn, Err }` (moved from `tui.rs`) and `pub struct Dispatched { pub outcome: Outcome,
   pub view: Vec<Section> }`. The binary's `dispatch` owns `CmdState` (including `view`) and returns `Dispatched`
   after every line, so the app always renders the current `view` without reading settings itself. The injected
   callback is `&mut dyn FnMut(&str) -> Dispatched`; the app gets the initial view from the first call's result
   (`run_app` takes `initial_view: Vec<Section>` too).
4. **`⏎` on a terminal row.** `GlobalData` carries `project_of: HashMap<String /*address*/, ProjectKey>` built each
   refresh with `project::compute_roots` + `list_projects` (same `RootCache` kept in the app). No project → warn note
   "<name> has no project".
5. **Leaving the board inside the app.** The app checks `ctrl+c` itself (quit the app) before passing a key to
   `BoardState::on_key`; when that returns `Effect::Quit` (`q` / `esc`) the app switches to `Screen::Global`. No change
   to `BoardState`, `Effect` or standalone `agm board`.
6. **Dispatcher context.** `pub struct Ctx<'a> { store: &'a Store, cfg: &'a Config, hc: &'a dyn Hc, who: Caller,
   now_ms: i64 }` (binary). `who` comes from the single `detect_caller` at start; `Caller::User { confirmed: true }` is
   built inside `dispatch` only when a pending confirmation receives `y`. The app's refresh (`scan`, `snapshot`) stays
   in the app loop; `dispatch` never draws.
7. **Confirmation answer in the app.** `NeedsConfirm(prompt)` opens the popup; the popup's text field sends its line to
   `dispatch` on `⏎` (so `y` confirms and anything else cancels, through the same pending logic as `--plain`); `esc`
   sends an empty line (= cancel).

## Left to the builder

- Exact widget choices, spacing, colour thresholds within a palette, help text wording.
- Whether `ansi_to_text` uses the `ansi-to-tui` crate (only if it resolves to the same ratatui 0.30) or a small
  in-house SGR parser.
- How the board loop is split into a screen (as long as `agm board` behaviour and its tests stay the same).
