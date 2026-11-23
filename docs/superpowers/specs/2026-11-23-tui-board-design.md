# TUI board: full-screen project view (ratatui) — design

Status: draft for approval (2026-11-23); builder check done (opencode/iceman-free), answers folded in

## Intent

The user tested the text project view (`cd <project>` in `agm tui`, 0.4.x work on `feat/tui-project`) and found
it not visual enough. They want a terminal UI that looks like modern TUIs — references: Toad (Textual: cards,
rounded borders, a highlighted selected card, sectioned layout, key-hint footer) and the "awesome-tui-design"
catalogue (Lazygit-style titled panels, k9s-style highlighted rows, btop-style bars, `✓ ▸ ✗ !` status lists,
coherent color themes such as Catppuccin / Tokyo Night / Nord). Above all they want to see, per project, **the
flows that are running**: the orchestrator connected to its workers, who is talking to whom, and what each agent is
doing right now — and to answer from there.

## Decisions taken with the user

- Approach **B**: a new full-screen board; the existing line-command `agm tui` stays for dangerous actions
  (`kill`, `interrupt`, `cleanup`, `policy`) and as the global overview.
- Entry: inside `agm tui`, `cd <project>` opens the board for that project full-screen; leaving the board returns
  to the `agm>` prompt. `agm board [project]` opens it directly.
- Look: three columns (projects + status summary | orchestration tree | selected agent detail + its messages),
  inbox along the bottom, key-hint footer, one coherent theme.
- Work progress in % stays out (no data yet); branch stage is shown as a pipeline instead.

## Goals

- Full-screen, keyboard-driven board built with `ratatui` on the existing `crossterm` backend.
- Orchestration panel: the project's agent tree (parent → children from `terminals.parent`) drawn as connected
  cards; each connector shows the latest message kind and age between the two agents and is highlighted when that
  message is under 5 minutes old ("flow is live").
- Agent detail panel for the selected card: app, model/effort, status, task, stage pipeline
  (`wip → ready → merging → merged`, current step highlighted), context bar, token sparkline + total, `doing:` with
  age, last message.
- Messages panel: the selected agent's conversation (messages it sent or received in this project, newest first,
  wrapped).
- Inbox panel: the project's open inbox items, selectable.
- Actions on the selected inbox item / agent: reply (`r`), done (`d`), send (`s`), show full text (`⏎`).
  They call the existing `actions::reply`, done and `actions::send` paths with a human `Caller`, exactly like
  `agm tui` does.
- Themes: Catppuccin Mocha (default), Tokyo Night, Nord; `t` cycles; the choice is saved in the `settings` table.

## Non-goals

- No `kill`, `interrupt`, `cleanup`, `policy`, `view` in the board.
- No % progress, no network-graph physics, no animation beyond the 2 s refresh.
- No change to the text `agm tui` global view, the CLI, MCP, or the data model.
- No mouse support in this round.

## Anti-goals

- Inventing data: unknown values stay `-` (same rule as the text project view).
- Weakening the human-only guard: the board refuses non-human callers exactly like `agm tui` (`detect_caller`,
  TTY required).
- Leaving the terminal broken: raw mode and the alternate screen are always restored (normal exit, error, panic).

## Constraints (from the code today)

- Data layer exists and is reviewed: `project::{list_projects, match_project, members, compute_roots,
  load_project_view, git_common_root, RootCache}`, `ProjectView`, `AgentView`, store queries in
  `src/store/project.rs`, `dashboard::snapshot`. The board renders from these; it adds only what the flows need
  (per-edge latest message) and the per-agent message list (filter `ProjectView.messages` by address/name).
- `crossterm 0.29` is already in `Cargo.lock` (via comfy-table). Add `ratatui` (0.30.x, current stable) with the
  crossterm backend; check that it resolves to the same crossterm version.
- `agm tui` reads lines on a stdin thread; entering the board must stop line reading, enter raw mode + alternate
  screen, and give the line reader back afterwards (the stdin thread must not swallow key events while the board
  runs).
- Windows 11 is the primary platform (Windows Terminal, herdr panes); Unicode box drawing is available. Provide an
  ASCII fallback for borders/connectors when `AGM_ASCII=1`.
- Untrusted text (messages, turn summaries, names, BRANCHES.md values) is sanitized before rendering
  (`dashboard::sanitize_one_line`), as today.

## Approaches considered

- **A.** Replace `agm tui` with the full-screen app — most uniform, but every safety flow (typed-`y`
  confirmation for kill/interrupt/cleanup) would have to be rebuilt; high regression risk.
- **B (chosen).** New board module; entered from `agm tui` via `cd` and from `agm board`. Dangerous actions stay
  in the proven line UI.
- C. Python/Textual app like Toad — closest look, but breaks the single Rust binary and adds a runtime. Rejected.

## Design

### Module layout

- `src/board/mod.rs` — `pub fn run(store, cfg, hc, project: Option<ProjectKey>) -> Result<BoardExit>`: terminal
  setup/teardown guard (raw mode, alternate screen, restore on drop and in a panic hook), event loop, refresh.
- `src/board/state.rs` — pure state: selected panel, selected project/agent/inbox index, scroll offsets, input
  mode (normal / composing reply / composing send / confirming done), theme; `fn on_key(&mut self, key) -> Effect`
  returning effects (`Reply{id,text}`, `Done{id}`, `Send{to,kind,text}`, `SwitchProject`, `Quit`, `None`) so key
  handling is unit-testable without a terminal.
- `src/board/view.rs` — rendering with ratatui widgets from `(state, data, theme)`; no I/O.
- `src/board/flows.rs` — pure: per-edge latest message (parent↔child, either direction, address or name) and
  per-agent conversation from `ProjectView.messages`.
- `src/board/theme.rs` — three palettes (Catppuccin Mocha, Tokyo Night, Nord): border, title, selected, ok,
  working, warn, error, muted, accent.

### Layout (≥ 120 columns)

```
╭ Projects ──────╮╭ agmag · orchestration ───────────────╮╭ t9 · worker ───────────╮
│ ▸ agmag   4 ●2 ││      ╭ ● CEO claude opus/high ╮      ││ status / task / stage  │
│   shop    2 ●0 ││      ╰──────────┬─────────────╯      ││ ctx · tokens · doing   │
╰────────────────╯│   ASSIGN 12m    │   QUESTION 5m      │╰────────────────────────╯
╭ Status ────────╮│ ╭ ● t7 ╮  ╭ ◐ t9 ╮  ╭ ✓ t4 ╮         │╭ Messages · t9 ─────────╮
│ ✓1 ●2 ◐1       ││ ╰──────╯  ╰──────╯  ╰──────╯         ││ newest first, wrapped  │
╰────────────────╯╰──────────────────────────────────────╯╰────────────────────────╯
╭ Inbox (2) ───────────────────────────────────────────────────────────────────────╮
│ ▸ #43 blocked t9  needs a decision: keep API v1 or move to v2? …                  │
╰──────────────────────────────────────────────────────────────────────────────────╯
 ↑↓←→ select  tab panel  ⏎ open  r reply  d done  s send  / filter  t theme  q back
```

- 80–119 columns: drop the Projects/Status column (project name in the orchestration title; `p` opens a project
  picker popup).
- < 80 columns or < 20 rows: single-column stacked view (orchestration list, detail, inbox), same keys.
- Orchestration tree: CEO/root cards on top, children on the next row(s), wrapping onto more rows when they do not
  fit; deeper levels indented below their parent. Card = status symbol + label + app/model on line 1, `doing:` or
  stage on line 2. Selected card: thick/accent border. Exited agents (kept only with open inbox) are dimmed.
- Connector label: latest message kind + age between parent and child; accent color when < 5 min old, muted
  otherwise, none when the pair never exchanged a message.

### Keys

| Key | Action |
|---|---|
| `↑↓←→` / `hjkl` | move selection inside the focused panel |
| `tab` / `shift+tab` | cycle panels (Projects → Orchestration → Detail/Messages → Inbox) |
| `⏎` | Projects: open project · Orchestration: focus that agent's messages · Inbox: show full item (popup) |
| `r` | reply to the selected inbox item (one-line input popup, `⏎` sends, `esc` cancels) |
| `d` | mark the selected inbox item done (`y` to confirm in a popup) |
| `s` | send a message to the selected agent (popup: type, text) |
| `/` | filter cards/messages by text |
| `t` | next theme (saved) |
| `q` / `esc` | close popup; else leave the board (back to `agm>` when entered via `cd`, exit when `agm board`) |
| `?` | help popup |

### Data refresh

- Every 2 s (and after each action): `dashboard::snapshot`, `terminals(false)`, `compute_roots`,
  `load_project_view`, `list_projects` + `store.inbox(true)` for the Projects column. Keep selection by address /
  inbox id across refreshes. A failed refresh shows an error line in the footer and keeps the last good data.

### Entry points

- `agm board [project]`: new CLI subcommand; runs the same human-caller guard as `agm tui`; without a project
  opens the Projects panel focused; with a project uses `match_project` (ambiguous/unknown → error before entering
  raw mode).
- `agm tui`: `cd <project>` (Found) launches the board for that project; on return, the REPL redraws and the scope
  stays global. `cd` (list) and `cd ..` keep working in the text UI. The text project view remains reachable as
  `cd -t <project>` for narrow/non-ANSI cases (my call: keeps the reviewed text view useful instead of deleting it).

## Testing

- `state.rs`: key → effect tables (navigation bounds, panel cycling, reply/send/done popups, esc/q semantics,
  selection kept across data refresh, filter).
- `flows.rs`: edge labels (direction-agnostic, address or name, newest wins, live < 5 min), per-agent conversation.
- `view.rs`: ratatui `TestBackend` buffer tests at 140×40, 100×30, 70×20: panel titles present, selected card
  highlighted, unknown values `-`, no text outside its panel, escape sequences in data render inert, ASCII fallback.
- `mod.rs`: terminal guard restores state on error (test the guard type with a fake backend); caller guard refuses
  non-TTY / agent callers.
- Existing gates: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`; manual run in
  Windows Terminal and a herdr pane at 140 and 90 columns.

## Phase 2 — move the whole `agm tui` onto the board framework (user request, 2026-11-23)

The user asked, "if possible", to move the current `agm tui` onto ratatui too, to reuse code as much as possible.
Plan it as a second phase after the board ships, so phase 1 is not blocked by the safety-flow rework:

- One ratatui app: the global screen (today's sections — quota, usage, terminals, budgets, inbox, system,
  messages — as titled panels, `view` choosing which) and the project board are two screens of the same app;
  `cd <project>` switches screens without leaving raw mode.
- A command bar at the bottom (`:` or always-on input line) that reuses `dashboard::parse_command` / `TuiCmd` and the
  existing action dispatch in `tui.rs`, so every current command keeps its exact semantics.
- Dangerous actions (`kill`, `interrupt`, `cleanup`) keep the human-only rule and the typed confirmation: a popup
  that requires typing `y` + `⏎`; anything else cancels. Same `Caller` handling as today.
- Keep the current line UI as `agm tui --plain` (non-ANSI terminals, piping, scripts), so the reviewed text code
  paths stay testable and usable.
- Phase 2 gets its own short design + review round before implementation.

## Left to the builder

- Exact widget choices, spacing, colors within each palette.
- How the stdin line reader is paused/resumed (channel flag vs. restarting the thread), as long as no key event
  is lost to it while the board runs.
- Card wrapping algorithm details.
