# TUI: `cd` into a project — design

Status: draft for approval (2026-11-23); builder check done (OpenCode qwen4-max-free), answers folded in

## Intent

`agm tui` mixes every project into one INBOX and one RECENT MESSAGES block. RECENT MESSAGES holds only the 5
newest messages of all projects, each cut to 60 characters, inbox lines are cut to one line, and the whole frame is
printed and scrolled (the TUI passes `height: None`, so `fit_blocks` is not active). With several projects running
the user can neither find nor read everything, nor answer it. The user wants to step into one project and see, for that project only:
which agents run, what each one is doing right now, their inbox items and their messages in full, and answer from
there (or from the global view, as today).

## Goals

- `cd <project>` scopes the TUI to one project; `cd ..` returns to the global view; `cd` alone lists projects.
- Project view shows an agent tree with, per agent: status, current activity ("what it is doing"), context bar,
  token sparkline + total, and branch stage when known.
- Project-filtered INBOX and MESSAGES with much more text per item than the global view.
- Answering works the same inside and outside: `reply`, `done`, `send` keep global `#ID`s and terminal addresses.
- New `show <#>` prints one inbox item's full text.

## Non-goals (this round)

- Work progress in % (brief checklists, option B) — later, if wanted.
- Network graph, animated message particles, permission/approval overlays, raw-mode keyboard navigation (the
  pasted "Agent Network" spec). The TUI stays a line-command REPL that redraws a frame.
- Changing the global view's layout, the `view` command or any runtime behavior (spawn, send, rotation).

## Anti-goals

- Showing anything not backed by data: unknown values render as `-`, never `0` or a guessed state.
- Rendering untrusted text (turn summaries, message text, BRANCHES.md values) without the existing escape
  sanitizing — these come from model output and other agents.

## Constraints (from the code today)

- TUI = `src/tui.rs` loop: read a line (`recv_timeout(refresh)`), `parse_command` (`src/dashboard.rs`), act,
  redraw the whole frame via `render_view(snapshot, width, sections, height)` + `render_frame`. Colors are raw ANSI;
  helpers exist: `paint`, `one_line`, `bounded_styled_line`, `sanitize_one_line`, `fit_blocks`, 1/8-cell bars
  (`~line 865`), sparkline glyphs `SPARK` (`~line 704`).
- Data: `terminals` (address, name, app, cwd, task, role, parent, state, status, session link), `messages`
  (`from_t`, `to_t`, kind, task, text, status), `inbox` (terminal, source, kind, text), `sessions` (cwd, model,
  ctx), `usage` (session, ts_ms, token columns), `turns` (session, ts_ms, role, summary — filled by all four
  parsers: Claude, Codex, agy, OpenCode; tool calls appear as `[tool] <name>`). `TermView` already carries model,
  effort, tokens, context_pct, last_msg, cwd.
- Turns/usage are only as fresh as the last ingest (`agm daemon` / TUI refresh).

## Approaches considered

1. **Project scope inside the existing REPL frame (chosen).** Add `cd`/`show`, a scope in the TUI loop, and a
   project renderer reusing the ANSI helpers. Small, testable, no new dependency.
2. Raw-mode interactive TUI (ratatui-style widgets, arrow keys). Richer, but rewrites `tui.rs` and its confirmation
   flows; rejected for now.
3. Separate `agm project <name>` one-shot command only. Cheap, but the user wants to answer from the live view.

Progress source: user picked **A (branch stage from BRANCHES.md) + C (measured activity: time, tokens, context)**;
B (checklists) deferred. User priority: "what each agent is doing" over work progress.

## Design

### Project identity

- A terminal's project = repository root of its `cwd` (fallback: the linked session's cwd, as `TermView.cwd`).
- Worktrees belong to their main repository: resolve with `git -C <cwd> rev-parse --path-format=absolute
  --git-common-dir` and take its parent directory. Cache per cwd for the TUI's lifetime (cwd → root), including
  failures.
- Not a git repo / git fails: use the cwd itself as the root.
- Project name = last path component of the root. If two roots share a name, show the root path to disambiguate
  and accept either in `cd`.
- Resolution lives in a pure function taking a resolver closure, so tests do not need git.

### Commands

| Command | Effect |
|---|---|
| `cd` | List projects: name, live agents, open inbox items; plus a `(no project)` row counting inbox items whose terminal is empty or matches no terminal row. That row is informational only — those items stay in the global INBOX; `cd` cannot enter it. |
| `cd <name>` | Enter project; unique prefix and case-insensitive match accepted; ambiguous/unknown → error note listing candidates. |
| `cd ..` or `cd /` | Back to the global view. |
| `show <#>` | Print one inbox item in full (wrapped), as a block above the note line until the next command. |

- Prompt shows the scope: `agm[agmag]>`; global stays `agm>`.
- Scope is kept in the TUI loop only (not persisted). `reply`/`done`/`send`/`interrupt`/`kill` are unchanged and
  use global IDs/addresses, so the same command works in both views.
- Add `cd` and `show` to `TUI_COMMANDS` (footer help) without short aliases (`c` is cleanup, `s` is send).

### Project view layout (replaces the section list while scoped)

```
── agmag · 4 agents · 2 inbox ──────────────────────────────────────────
 ceo  claude opus/high  ● working  ctx ███████░ 72%  ▁▂▄▆█▅ 1.2M
      doing: [tool] Bash · 12s ago
 ├ t7  codex gpt-6.1    ● working  ctx ███░░░░░ 31%  ▁▁▂▃▅  380k  stage: wip
 │    doing: Implementing the parser fallback… · 40s ago
 └ t9  opencode flash   ◐ blocked  ctx -              -      -    stage: -
      doing: - (no linked session) · last msg: REPORT sent
── INBOX · agmag (2) ───────────────────────────────────────────────────
 #43 blocked t9 — needs a decision: keep API v1 or move to v2? The parser
     currently depends on …                                  (wrapped, ≤4 lines)
── MESSAGES · agmag ────────────────────────────────────────────────────
 12m ceo → t7  ASSIGN   brief .agents/tasks/x.md … (wrapped, ≤3 lines)
```

- Header (existing `render_header`) and footer stay; the sections are AGENTS, INBOX, MESSAGES (fixed order).
  Like the global view today, the frame is printed in full and scrolls (no height fitting); the only truncation is
  the per-item wrap caps below. *(Builder-check answer, my call: height fitting would cut exactly the text the
  user wants to read.)*
- **AGENTS:** terminals whose project matches; live ones always, exited ones only if they still have an open inbox
  item. Tree by `parent` (a parent outside the project or missing → root). Per agent:
  - status: herdr status with symbol + word (not color only), reusing the global terminals table's mapping;
  - app + model/effort from `TermView`;
  - `doing:` latest `turns` row of the linked session (role `assistant` or tool), sanitized, one line, with age;
    no linked session → `-` plus the last message kind/direction if any;
  - `ctx` bar from `context_pct` (`-` when None);
  - token sparkline: the linked session's `usage` over the last 60 minutes in 12 five-minute buckets (sum of
    input_uncached + cache_write + output + reasoning; cache_read excluded so cached re-reads do not dominate), plus
    the session total as today (`tokens`); `-` when unlinked;
  - `stage:` from `<root>/BRANCHES.md` read-only: entry `<lane>/<slug>` matched from a worktree directory name
    `<lane>--<slug>`, else from `task` if it has that shape; no file/entry → `-`.
- **INBOX:** open items whose terminal is in the project; text wrapped up to 4 lines each, longer → `…`.
- **MESSAGES:** the newest **20** messages where `from_t` or `to_t` matches a terminal of the project (address or
  name, as the existing `last_msg` query does; exited terminals included), newest first, with the message age (`12m ago`, via `format_age_s`; the crate has no local-time support); text
  wrapped up to 3 lines; same kind badges and status colors as today. Loaded by a new scoped query, not from
  `Snapshot.messages` (which holds only 5 global rows). *(My call: 20.)*
- Narrow panes (< 70 cols): drop sparkline, then ctx bar, before cutting names; nothing may exceed the width
  (`bounded_styled_line`).

### Data access

- Scoped data is loaded only while scoped, into a `ProjectView` struct: the project's full `TerminalRow`s (all
  states — `Snapshot`/`TermView` hold live terminals only and lack `parent`, state and session id), latest turn
  per linked session, usage buckets, stages, the 20 scoped messages, and the project's inbox items. The existing
  `Snapshot` is still built for the header and for `TermView` fields (model, effort, tokens, context_pct) of live
  agents; `Snapshot`, `TermView` and the global queries are not changed.
- New store queries (read-only, bounded): terminals by project membership is done in Rust over `terminals(false)`;
  `messages_involving(addresses_and_names, limit)`; latest turn per session; usage buckets per session since a
  timestamp.

## Testing (required in the plan)

- `parse_command`: `cd`, `cd x`, `cd ..`, `cd /`, `show 4`, bad `show`.
- Project resolution: worktree → main root via fake resolver; non-git fallback; duplicate names; prefix/ambiguous
  `cd` matching.
- Filtering: messages included when either side is in the project; inbox without terminal → `(no project)`.
- Rendering with fixture snapshots at widths 60 and 110: no line wider than the pane (measure with
  `unicode-width` after stripping ANSI), `-` for unknown values, tree glyphs, wrapping limits (the TUI has no `NO_COLOR` mode today; not added).
- Untrusted text: a turn summary / message containing `\x1b]0;…\x07` and CSI sequences renders inert.
- Store queries: bucket boundaries, sessions without usage, latest-turn ordering.
- Full gates: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.

## Left to the builder

- Exact column widths, colors, and how the wrapped text is indented.
- Whether `ProjectExtras` loading lives in `dashboard.rs` or a new `src/project.rs` (prefer a new module if
  `dashboard.rs` would grow by more than ~300 lines).
- Cache structure for cwd → root.

## Delivery

Implemented by an OpenCode worker on a free/cheap model in a worktree, following a step-by-step TDD plan;
independent review by a different model family before merge; README/README_vi TUI section updated in the same
branch.
