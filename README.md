# agmag

**English** · [Tiếng Việt](README_vi.md)

**A model control panel for agent teams running in [herdr](https://github.com/yigitkonur/awesome-herdr) / herdr-core.**

`agm` sits next to herdr and tells you, in one place:

- **how many tokens** each agent (Claude Code, Codex, OpenCode, agy/Gemini) has used and **how full** its context is;
- **how much quota** is left on your subscriptions (ChatGPT, Claude) and **when it resets**;
- tokens used in the **last 24 h / today / this month**, plus a **4-month** chart (local time);
- which agent is **stopped and waiting for you** (inbox);
- **what the agents said to each other**.

It also lets you intervene in a controlled way: assign work with worker-count and budget limits, answer agents, stop agents (Codex/Claude only when **you** say so), automatically **rotate** OpenCode workers when their context fills up, and **hand off to a fresh session** for Claude Code/Codex without ever killing the old one automatically.

> Primary platform: Windows 11 (also runs on Linux/macOS). Written in Rust — a single `agm.exe`, no runtime needed. Current version: **0.4.0** — see [CHANGELOG.md](CHANGELOG.md).
>
> Tables and command output are in **English**. Message types stored by `agm send` are still the Vietnamese codes listed below; `agm tui` also accepts their English names.

---

## Contents

1. [Install](#1-install)
2. [Up and running in 1 minute](#2-up-and-running-in-1-minute)
3. [How agm works — do I install it per session?](#3-how-agm-works)
4. [Background daemon and dashboard](#4-background-daemon-and-dashboard)
5. [When to use `hc` vs `agm`](#5-when-to-use-hc-vs-agm)
6. [Letting agents use agm](#6-letting-agents-use-agm)
7. [All commands](#7-all-commands)
8. [Token statistics over time](#8-token-statistics-over-time)
9. [Budgets, policy and rotation](#9-budgets-policy-and-rotation)
10. [Configuration](#10-configuration)
11. [Safety and data](#11-safety-and-data)
12. [Known limitations](#12-known-limitations)
13. [Development](#13-development)

---

## 1. Install

Requires [Rust](https://rustup.rs) (with `cargo`).

```powershell
git clone <this repo URL> agmag
cd agmag
cargo install --path .          # copies agm.exe into ~/.cargo/bin (already on PATH)
agm --version                   # agm 0.4.0
```

> `cargo build --release` **only** produces `target\release\agm.exe` — typing `agm` will say "not recognized". Use `cargo install --path .`.
> After every code update, run `cargo install --path .` again. Reopen PowerShell if the command still isn't found.

agm keeps its data in `~/.agmag/` (`agm.db`, `providers.toml`). Override with the `AGM_HOME` environment variable (other variables: see [section 10](#10-configuration)).

> Upgrading from 0.1.x: the database migrates to schema v3 automatically on first run (rolled back on error). Nothing to do.

## 2. Up and running in 1 minute

No configuration needed:

```powershell
agm quota          # subscription quota: 5-hour / 7-day windows, time until reset
agm ls             # today's sessions: model, tokens, % context
agm cost           # token totals by model  (--by day | month | source | cwd, --days 7)
agm cost --summary # 24h / today / this month + 4-month chart
agm sys            # live RAM / CPU / disks + how much each agent's processes use
```

Example `agm quota` (columns: plan, window, % left, status, time until reset). Stale rows show `stale · seen <age> ago` instead of a fresh bar/percentage — the same label as the TUI:

```
plan    window  left         status                   reset
codex   5h      █████████░   86%                     in 4h38m
codex   7d      █░░░░░░░░░   7%                      in 115h41m
codex   5h      ··········   stale · seen 1d05h ago  reset passed
```

The first run is a bit slow (it reads the last 7 days of logs); after that only new data is read. For older data in the monthly statistics: `agm scan --since 2026-08-01` (or let `agm tui` backfill automatically).

## 3. How agm works

**No per-session install.** agm is installed once per machine and *reads the traces each tool already writes*, so every session — old, new, opened via herdr or by hand — is counted:

```
 Claude Code ──writes──▶ ~/.claude/projects/*.jsonl ──────────────────────┐
 Codex       ──writes──▶ ~/.codex/sessions/*.jsonl  ──────────────────────┤
 OpenCode    ──writes──▶ ~/.local/share/opencode/opencode.db ─────────────┼──▶ agm ──▶ ~/.agmag/agm.db
 agy (CLI)   ──writes──▶ ~/.gemini/antigravity-cli/brain/*/…/transcript.jsonl ┘     ▲
 herdr-core  ──events (hc events)──▶ agm daemon ────────────────────────────────────┘
                                          │
                        you look at: agm quota / ls / cost / inbox / tui
```

| Tool | What do I need to do? | Where do I see the numbers? |
|---|---|---|
| **Claude Code** | Optional. For **5h/7d subscription quota %** (and to use `agm manage`), run `agm hook install` **once** (applies to every Claude session) | Claude Code's bottom status line: `Opus 5.5 · ctx 34% · 5h 76% left · 7d 59% left`; and every `agm` command |
| **Codex** | Nothing (quota is read from Codex logs; the daemon can also refresh it from `codex app-server` every 15 min, or now with `agm quota --refresh`) | `agm quota / ls / tui` |
| **OpenCode** | Nothing (agm reads `opencode.db`, read-only). Go plan quota (5h/7d/mo) is read by the daemon every 15 min or with `agm quota --refresh` via `opencode api GET /api/experimental/usage/limits`; [Go Console](https://opencode.ai/console/go) remains the place for Extra Usage/budget | `agm quota / ls / cost / tui` |
| **agy (Gemini)** | Nothing. agm reads `transcript.jsonl` of every interactive `antigravity-cli` session and `conversation_summaries.db` (read-only, for the working directory); `agm ask` is recorded separately and never double-counted. **Plan quota** (Gemini / Claude-GPT via agy / Argon) is read by the daemon every 15 min via `agy --print /usage` (no tokens), or now with `agm quota --refresh` | `agm ls / cost / quota / tui` |

> `agm hook install` adds a `statusLine` to `~/.claude/settings.json`. If you already have your own statusLine, it refuses and does not overwrite it — call `agm hook statusline` from your own script instead.

## 4. Background daemon and dashboard

The commands in section 2 are enough to **look**. For **continuous monitoring** (which terminals are running, who is asking you something, budgets, rotation), run the daemon — preferably inside herdr-core so it lives independently of any window:

```powershell
hc run --bg --no-parent --name agm-daemon agm daemon
hc read agm-daemon            # see what it's doing
hc kill agm-daemon            # stop it
```

Then open the dashboard in a herdr pane:

```powershell
agm tui
```

```
 ◆ agmag  ● POLICY ON │ 7 terminals (1 blocked) │ inbox 1 │ cpu 23% ram 57% C: 71% │ UTC+07:00
── ◈ PLAN QUOTA (left) ───────────────────────────────────────────
  codex      5h  █████████░  86% left  resets in 4h38m
  codex      7d  █░░░░░░░░░   7% left  resets in 115h41m
── ▪ USAGE (UTC+07:00) ───────────────────────────────────────────
  24h 125k │ Today 480k │ This month 4.2M
  Aug ▂ 1.1M │ Sep ▄ 2.3M │ Oct ▆ 3.4M │ Nov █ 4.2M
── ▸ TERMINALS ───────────────────────────────────────────────────
┌───────────────┬─────────────┬─────────────────────────────┬────────┬─────────┬─────────┬────────┬─────┬────────────┬────────────────────┐
│ name          │ agent       │ model                       │ effort │ where   │ task    │ tokens │ ctx │ last msg   │ status             │
╞═══════════════╪═════════════╪═════════════════════════════╪════════╪═════════╪═════════╪════════╪═════╪════════════╪════════════════════╡
│ ● t1q         │ codex       │ gpt-6.1-sol                 │ high   │ app / 1 │ -       │  21.2M │ 58% │ ← ASSIGN   │ working            │
│ ● t3          │ claude-code │ claude-opus-5-5             │ max    │ app / 2 │ -       │   1.4M │ 71% │ → REPORT   │ idle [HANDED OFF]  │
│ ● w-login-7-1 │ opencode    │ opencode-go/deepseek-v4-pro │ high   │ app / 3 │ login-7 │   340k │ 50% │ → QUESTION │ BLOCKED (question) │
└───────────────┴─────────────┴─────────────────────────────┴────────┴─────────┴─────────┴────────┴─────┴────────────┴────────────────────┘
── ◆ BUDGETS ─────────────────────────────────────────────────────
  task:login-7             █░░░░░░░░░  11%  340k / 3.0M tokens soft
── ● INBOX ───────────────────────────────────────────────────────
  #1 [blocked] t5 — Chạy test?
── » RECENT MESSAGES ─────────────────────────────────────────────
  (no messages)
──────────────────────────────────────────────────────────────────
cmd: reply <#> <text> · done <#|a-b|kind:K|term:T> · send <terminal> <TYPE> <message> · cleanup [1,2|all]
     interrupt <terminal> · kill <terminal> · policy on|off · view <up to 3 sections> · quit
     TYPE: ASSIGN REPORT ERROR QUESTION BLOCKED PASS FAIL
```

TUI commands (type, then Enter):

| Command | Action |
|---|---|
| `reply 1 yes` | Reply to inbox item 1 (typed into the related terminal) |
| `done 1` | Dismiss item 1 (types nothing anywhere) |
| `send t5 REPORT All tests passed` | Send a recorded message |
| `cleanup` · `cleanup 1,2` · `cleanup all` | List / select handed-off sessions to clean up (requires `y` confirmation) |
| `interrupt t5` · `kill t5` | Interrupt / kill (both require an extra `y` confirmation) |
| `policy off` | Disable all automatic actions |
| `view` · `view terminals inbox` · `view default` | Show the current view / pick up to 3 sections (`quota`, `usage`, `terminals`, `budgets`, `inbox`, `system`); remembered across restarts. RECENT MESSAGES is always shown and does not count |
| `quit` | Quit |

Each command also accepts its first letter as a shortcut (`r`, `d`, `s`, `c`, `i`, `k`, `p`, `q`). Message types for `send`: `ASSIGN`, `REPORT`, `ERROR`, `QUESTION`, `BLOCKED`, `PASS`, `FAIL` (any letter case). The legacy Vietnamese types (`GIAO`, `BÁO CÁO`, `LỖI`, `HỎI`, `CHẶN`, `ĐẠT`, `CHƯA ĐẠT`) still work in the TUI; messages are stored and forwarded with the Vietnamese code, and `agm send --type` is unchanged.

TERMINALS columns: `●` is colored by state (green working, cyan idle/done, red blocked, yellow handed off, gray exited) followed by the terminal name, shortened to 12 characters (address when unnamed; commands accept either). `agent` is the CLI, `model`/`effort` come from the linked session log (Codex `effort`, Claude Code `effort`, OpenCode variant, agy model suffix such as `-high`), and `where` is herdr's location as `hc ls` shows it (`MATHY-AI / 1`, `agmag / agm-ceo`), falling back to the working folder name. `last msg` is the latest message involving the terminal, in English: `→ REPORT` means it sent a report, `← ASSIGN` means it received an assignment. Colors match RECENT MESSAGES (cyan ASSIGN, green REPORT/PASS, yellow QUESTION, red ERROR/FAIL, bold red BLOCKED).

The dashboard prints every row (nothing is cut to the pane height); scroll up with the mouse to see the top. Each repaint clears the screen and the scrollback first, so older frames never pile up, and it only repaints when something changed or after you type a command, so scrolling up to read is not interrupted on every refresh. It shows at most 3 chosen sections (default `quota terminals inbox`) plus RECENT MESSAGES, always pinned just above the command bar. The pane width is detected automatically; `COLUMNS` overrides it (clamped to 60–200). Long names, paths and messages are shortened to that width. Unicode text is measured in terminal cells without splitting graphemes; embedded terminal control sequences are stripped. Truncation affects display only, not stored messages or action targets.

The TUI shows a **red** warning when a handed-off session and its successor are both running in the same directory, and backfills missing months for the statistics automatically. Only **you** can open the TUI from a terminal (agents can't).

## 5. When to use `hc` vs `agm`

`hc` is the **hands** (run, look, type into terminals). `agm` is the **ledger and the rules** (record, check quotas, block dangerous actions) — agm calls `hc` under the hood.

| Task | Use |
|---|---|
| Open an agent **to work with it yourself** | `hc run` / open in herdr (then `agm manage` if you want agm to rotate it) |
| Look in / type directly | `hc attach t5`, `hc read t5` |
| Run commands or background tests (not agents) | `hc run --bg …`, `hc exec …` |
| **Assign work to a worker** (spawn an agent per task) | `agm spawn worker-mid --task login-7 -- "…"` |
| **Message an agent** | `agm send w-login-7-1 --type GIAO --task login-7 "…"` |
| **Answer an agent asking a question / permission** | `agm reply 1 "yes"` (or `r` in the TUI) |
| **Stop an agent** | `agm kill t5` (guarded; `hc kill` is not) |
| See tokens, quota, who is waiting for you | `agm quota / ls / cost / inbox / tui` |

Rules of thumb:
1. **Agent-team** work (assign, message, answer, stop, spawn) → `agm`.
2. **You operating by hand** → `hc`.
3. Agents talking to each other → always `agm send` (so there's a record and questions reach your inbox).

Agents started with `hc run` are still **metered**, but have no task label → agm won't apply task budgets to them. For full management, spawn with `agm spawn`; for hand-opened Claude Code/Codex sessions, enable rotation with `agm manage <t>`.

## 6. Letting agents use agm

Add this to your project's `AGENTS.md` (read by Codex, OpenCode) and `CLAUDE.md` (read by Claude):

```markdown
## Communication via agmag
- Message another agent: `agm send <terminal-name> --type GIAO|BÁO CÁO|LỖI|HỎI|CHẶN|ĐẠT|CHƯA ĐẠT --task <ID> "<short text>"`
- Need a user decision: `agm send user --type HỎI --task <ID> "<question>"` (goes to the user's inbox)
- Check the inbox (Codex manager): `agm inbox --json`
- Never use `agm kill` on Codex/Claude sessions — only the user may do that.
```

Message types: `GIAO` assign · `BÁO CÁO` report · `LỖI` error · `HỎI` question · `CHẶN` blocked · `ĐẠT` pass · `CHƯA ĐẠT` fail.

agm knows who is sending via the `HC_TERMINAL_ID` variable that herdr-core sets in each terminal. Every command prints JSON when output is not a terminal (or with `--json`) so agents can parse it. See [AGENTS-TUTORIAL.md](AGENTS-TUTORIAL.md) (Vietnamese) for a detailed agent guide.

### MCP server

`agm mcp` is an MCP server over stdio (newline-delimited JSON-RPC 2.0; protocol versions 2025-06-18, 2025-03-26, 2024-11-05) so Claude Code and OpenCode can call agm as tools. Register it:

```bash
# Claude Code
claude mcp add -s user agm -- agm mcp
```

```json
// opencode.json
{"mcp":{"agm":{"type":"local","command":["agm","mcp"]}}}
```

| Tool | Purpose |
|---|---|
| `agm_send` | Send a recorded message (`kind`: `ASSIGN\|REPORT\|ERROR\|QUESTION\|BLOCKED\|PASS\|FAIL`; Vietnamese codes also accepted) |
| `agm_inbox` | Read the inbox |
| `agm_ps` | List herdr-core terminals |
| `agm_quota` | Subscription quota as stored in the DB, kept fresh by `agm daemon` (no log scan, no refresh) |
| `agm_spawn` | Spawn an agent (same checks as `agm spawn`) |
| `agm_branch_get` | Read an entry of `BRANCHES.md` (`branches_file` must be an absolute path) |
| `agm_branch_update` | Update an entry of an existing `BRANCHES.md` (absolute `branches_file`; same rules as `agm branch set`) |

The sender is the caller's `HC_TERMINAL_ID`, or `agent` — never `user`. Unknown tool arguments are rejected. Enable the `agm` MCP server only in manager/review profiles (BOT-CEO, Copilot, reviewers), not in ordinary worker profiles: `agm_spawn` can start terminals the same way `agm spawn` can. `agm mcp` has no kill/interrupt/manage/cleanup tools (nor reply/done); those stay with the user.

### BRANCHES.md

`agm branch get <ENTRY> [--file PATH]` prints one entry; `agm branch set <ENTRY> <KEY=VALUE>... [--file PATH] [--worktree PATH]` updates it. `--file` defaults to `BRANCHES.md`, which must already exist (agm never creates it; create it with a title line first); the MCP tools need an absolute path. Entries are `### <lane>/<slug>` blocks of `- key: value` lines. Writes take the lock directory `<dir of BRANCHES.md>/.agents/.runtime/locks/branches` (agm holds it for milliseconds, waits up to 30 s and reclaims a lock older than 60 s, so do not take it by hand with `mkdir`: a manual hold over 60 s can be reclaimed and lose an update) and write via temp file + rename. Valid statuses: `wip blocked ready merging merged deployed needs-rebase failed`. Valid keys: `status lane tier head base depends_on migration touches_hotspot summary verify review docs registered git`. `status: ready` requires a 12–40 hex `head`, `--worktree`, a clean worktree and a matching HEAD; terminals with role `worker` are refused.

## 7. All commands

| Command | Purpose |
|---|---|
| `agm quota [--refresh]` | Subscription quota left per time window (`--refresh`: read agy, Codex, and OpenCode Go quota now; stale rows are marked in the table, the TUI and in `--json`) |
| `agm ls [--days N]` | Sessions: source, model, tokens, % context, warnings |
| `agm cost [--by model\|day\|month\|source\|cwd] [--days N \| --window 24h\|today\|month\|4months] [--summary]` | Token totals with a bold **SUM** row; 24h/today/this-month summary + 4-month bar chart in local time. **USD** = tokens × each model's list API price for every model (subscriptions included, as what the usage would cost on the API); `≥ $x` when some tokens have no known price, `?` when none do. `billed (API)` in `--summary` is what API-billed models actually cost |
| `agm log <session> [--limit N] [--full]` | Turn summaries (`--full`: raw log lines, Codex/Claude only) |
| `agm scan [--days N \| --since YYYY-MM-DD]` | Scan Codex/Claude/agy/OpenCode logs now (view commands scan automatically) |
| `agm daemon [--interval 10] [--replay <file>]` | Background: hc events, log scanning, terminal↔session matching, budgets, rotation (`--replay`: load events from a file, for debugging) |
| `agm sys` | Live machine load: CPU % with a grid of **every logical core** (`#0 ██████▏░ 76%`, busiest core and idle count on the CPU line), RAM (and swap), every disk (used/free, read/write per second) and each agent's **process tree** (`claude-code`, `codex`, `opencode`, `agy`; desktop apps as `claude-app`, `codex-app`, `agy-app`): process count, RAM, % of machine CPU, plus a **Σ SUM** row. Colored report in three sections (MACHINE · DISKS · AGENTS) opening with a load verdict: green `● OK` (room for more workers), yellow `● BUSY` (CPU or RAM ≥ 60%), red `● HIGH LOAD` (≥ 85%, hold off spawning) and `⚠ <disk> nearly full` at 90%. Each agent keeps its TERMINALS color and gets a RAM-share bar. Takes about 1–1.5 s (CPU % needs two readings, plus a scan of the process table); nothing is stored. `--json` for agents, `NO_COLOR=1` for plain text |
| `agm tui [--refresh 2]` | Dashboard. The header always shows `cpu · ram · fullest disk` (green < 60%, yellow < 85%, red); `view system` adds the full SYSTEM section, with one colored column per core (`┊ cores ▆▃▆▅▅▅`) |
| `agm ps [--all]` | herdr-core terminals known to agm |
| `agm inbox [--all]` · `agm reply <#> <text>` · `agm done <IDS...> [--kind K] [--terminal T] [--source S]` | Inbox; answer an agent's question; dismiss inbox item(s) |
| `agm send <target> --type <TYPE> [--task ID] <text>` | Send a recorded message; target `user` → inbox |
| `agm spawn <template> --task <ID> [--role worker] [--name …] [--cwd …] -- <args…>` | Spawn an agent (`hc run --bg --outlive-parent`), keeping each argument after `--` intact; if `<template>` is an hc template, those arguments are joined into one task string. On Windows, when `hc run <template>` fails with `cannot start "sh"`, agm falls back to running the template's `command:` directly and sends the template body plus the task to the terminal; if that brief cannot be delivered (agent not ready within 20 s, terminal exited), the brief goes to the inbox (`brief_failed`), the address is still printed and agm exits with code **3** — the terminal is running, so do not clean up its worktree or spawn again. Checks `max_workers` and budgets |
| `agm branch get <ENTRY> \| set <ENTRY> <KEY=VALUE>...` `[--file PATH] [--worktree PATH]` | Read / update an entry of `BRANCHES.md` under a lock, atomically |
| `agm mcp` | MCP server over stdio exposing 7 tools (see [MCP server](#mcp-server)) |
| `agm kill <t> [--yes]` · `agm interrupt <t> [--yes]` | Kill / interrupt — Codex, Claude, unknown apps: **you only**, always requires typing `y` (`--yes` doesn't apply) |
| `agm manage <t> [--off] [--cmd … \| -- <argv>…]` | Enable/disable rotation management for a hand-opened Claude Code or Codex session |
| `agm rotate <t> [--yes]` | Rotate now: OpenCode worker (hand-off + kill + respawn) or a managed Claude/Codex session (hand-off + new session, old one is **not** killed) |
| `agm cleanup [targets…] [--all] [--yes]` | Manually clean up old handed-off sessions (one typed `y/N` confirmation; `--yes` is accepted for compatibility but does not skip it) |
| `agm budget set <scope> --soft X --hard Y [--action notify\|interrupt\|kill]` · `ls` · `rm <scope>` | Budgets: `task:<ID>` (tokens) or `provider:<name>:<minutes>` (% quota) |
| `agm policy on\|off\|status` | Enable/disable all automatic actions (emergency stop) |
| `agm link <t> <session>` | Manually pair terminal ↔ session when agm isn't sure |
| `agm ask [--model gemini-4-argon-high] [--timeout 300] <question>` | One-shot agy question (read-only, runs in a temp dir outside the repo), tokens recorded |
| `agm hook install` · `agm hook statusline` | Install / run the Claude Code statusline hook |

Global flag: `--json` (default when output is not a terminal).

## 8. Token statistics over time

All time boundaries use **local machine time**, not UTC.

```powershell
agm cost --summary              # 24h / Today / This month table + 4-month bar chart
agm cost --by month             # totals per month
agm cost --window today --by source
agm scan --since 2026-08-01     # backfill older logs (the TUI does this for missing months)
```

**Prices.** Built-in list prices live in `assets/prices.toml` (snapshot 2026-11-20 from the providers' public API prices; base tier, no long-context surcharge). OpenCode `provider/id`, dated ids and agy effort suffixes (`-high`) are matched automatically; ids ending in `-free` cost $0. Override or add a model in `~/.agmag/providers.toml`:

```toml
[models."codex-auto-review"]
provider = "codex"
price = { input = 2.0, output = 10.0, cache_read = 0.1, cache_write = 2.5 }   # USD / 1M tokens
```

- The number of months for statistics/backfill is set by `stats_months` in `providers.toml` (default: the last 4 calendar months).
- If a period hasn't been fully scanned, agm flags the data as possibly incomplete and suggests the `agm scan --since …` command, instead of silently showing low numbers.
- USD is computed only for `billing = "api"` models with a declared price; if any API model has no price, the USD total shows `?` (never a made-up 0).

## 9. Budgets, policy and rotation

**Budgets** (checked by the daemon every cycle):

```powershell
agm budget set task:login-7 --soft 2000000 --hard 5000000              # tokens for one task
agm budget set provider:codex:10080 --soft 80 --hard 95                 # % of weekly quota USED (10080 min); TUI/quota show what is left
agm budget set task:login-7 --soft 2000000 --hard 5000000 --action kill # hard limit → kill that task's OpenCode workers
```

- **Soft** limit exceeded → an inbox item (and a message to the manager terminal if `manager` is configured).
- **Hard** limit exceeded → inbox + action (`notify` by default); `agm spawn` for that task is refused.
- Each level fires **once**; dismissing the inbox item doesn't re-trigger it. Codex/Claude are never stopped automatically — only a suggestion in the inbox.

**OpenCode worker rotation** (worker = spawned with `agm spawn` using `--role worker`):

1. Context ≥ `ctx_warn` (50%) → warning in the inbox.
2. Context ≥ `ctx_rotate` (70%) and the worker is idle → agm asks it to write a hand-off to `.agents/log/<ID>.md` and commit.
3. When the worker finishes its turn: agm checks for a **clean worktree** and **a new commit touching that file**, verifies it can start a new session → kills the old session → starts a new one with the **same template/model/directory** and the task "read tasks/log/git log and continue".
4. If the hand-off isn't done yet, it waits; after `handoff_timeout_min` → inbox notice, **old session kept**. No automatic retry after failure — use `agm rotate <t>`.

**Claude Code & Codex session rotation (never auto-kills the old session):**

1. A hand-opened session must be enabled once with `agm manage <t>` — only **you** can do this. agm stores a `launch_spec` (the launch command, sanitized per app; you can specify it with `--cmd "…"` or `-- <argv>…`). Claude requires `agm hook install`. Disable with `agm manage <t> --off`.
2. When it reaches `ctx_rotate` and is idle (`idle` or `done`), agm asks it to write a hand-off to `.agents/handoff/<id>-<ts>.md` ending with the marker `<!-- agm:handoff-done -->`. agm adds `.agents/handoff/` to the repo's `info/exclude` (git worktrees supported) and does not force a WIP commit.
3. Once the hand-off file is valid, agm opens a new session that reads it and then **waits for you** to continue; the old session stays as `[HANDED OFF]` so you can review it and clean it up manually with `agm cleanup` (or `c` in the TUI). This is a coordination convention (no OS-level write lock on the old session) — avoid editing in both windows; the TUI warns in red when both run in the same directory.

`agm policy off` disables all automatic actions (works even if `providers.toml` is broken).

## 10. Configuration

`~/.agmag/providers.toml` (no file → defaults from [assets/providers.default.toml](assets/providers.default.toml)):

```toml
[providers.codex]
billing = "subscription"        # subscription: % quota; USD shown only as list-price value, never billed
[providers.claude]
billing = "subscription"
[providers.openrouter]
billing = "api"                 # pay per token: this USD is actually billed (shown as `billed (API)`)

[models."gpt-6.1-sol"]
provider = "codex"              # OpenCode using openai/gpt-6.1-sol also consumes ChatGPT quota
context_window = 475000
[models."some-api-model"]
provider = "openrouter"
price = { input = 3.0, cache_read = 0.3, cache_write = 3.75, output = 15.0 }   # USD per 1M tokens

[aliases]
"opus" = "claude-opus-5-5"

[policy]
max_workers = 3                 # max concurrent workers
manager = "codex-mgr"           # terminal receiving budget alerts; empty = inbox only
ctx_warn = 50                   # % context: warn
ctx_rotate = 70                 # % context: rotate session
handoff_timeout_min = 10
link_timeout_min = 5
stats_months = 4                # number of recent calendar months for statistics & backfill
agy_quota_poll_min = 15         # minutes between agy quota reads (`agy --print /usage`); 0 = off
codex_quota_poll_min = 15       # minutes between Codex quota reads (`codex app-server`); 0 = off
quota_stale_min = 60            # minutes before quota data is treated as stale (also stale if newer package usage exists)
```

Models without a declared price or context window show `?` — agm never invents a 0.

**OpenCode context windows.** For OpenCode models, a `[models."<providerID/id>"]` (or bare `[models."<id>"]`) entry in `providers.toml` always wins; otherwise agm reads the limit from OpenCode's own model catalog (`opencode api GET /api/model`, cached for the process). Nothing is guessed, so unknown models still show `?`. OpenCode sessions whose total token count is 0 (failed/empty assistant messages) are no longer flagged `unknown format?`.

**Environment variables** (optional):

| Variable | Default | Meaning |
|---|---|---|
| `AGM_HOME` | `~/.agmag` | Where `agm.db` and `providers.toml` live |
| `AGM_CODEX_DIR` · `AGM_CLAUDE_DIR` | `~/.codex/sessions` · `~/.claude/projects` | Where Codex / Claude Code logs are read from |
| `AGM_OPENCODE_DB` | `~/.local/share/opencode/opencode.db` | OpenCode database |
| `AGM_AGY_DIR` | `~/.gemini/antigravity-cli` | Antigravity CLI root directory |
| `AGM_CLAUDE_HOME` | `~` | Home directory containing `.claude/settings.json` (hook check for `manage`) |
| `AGM_HC_BIN` · `AGM_AGY_BIN` | `hc` · `agy` | Path to the `hc` / `agy` executables |

## 11. Safety and data

**Rules for stopping agents**

- **Codex and Claude are never stopped automatically** — not even during rotation. Agents and policy can only stop terminals that `hc get` confirms are OpenCode (allow-list); unknown apps → refused, with a suggestion routed to your inbox.
- The caller is classified as *agent when in doubt*: `HC_TERMINAL_ID` set but terminal unreadable, the terminal is an agent app, or no TTY → agent.
- Stopping Codex/Claude: you must type `y` at the terminal; `--yes` doesn't apply to them.
- `agm manage` and `agm cleanup` can only be run by the user and always require confirmation.

**Data**

- agm **only reads** Claude/Codex logs, agy's `transcript.jsonl` + `conversation_summaries.db`, and `opencode.db` (only the two session/message tables — never the table holding credentials). Those tools' databases are opened `READ_ONLY`.
- Outside `~/.agmag/`, agm writes in only five places: `BRANCHES.md` and its lock directory `.agents/.runtime/locks/branches` (via `agm branch set` or the MCP tool `agm_branch_update`), the `statusLine` in `~/.claude/settings.json` (when you run `agm hook install`), a `.agents/handoff/` line in the repo's `info/exclude` (when rotating a managed Claude/Codex session; the hand-off file itself is written by the agent), and the `agmag-ask` temp directory for `agm ask`.
- Everything agm stores lives in `~/.agmag/agm.db` on your machine: tokens, quotas, ≤ 200-character summaries per turn, messages sent via `agm send`, the inbox, the intervention log. Nothing is sent anywhere.

## 12. Known limitations

- Sessions running in the HERDR APP are metered via logs, but only **herdr-core** terminals can be intervened on.
- agy tokens outside `antigravity-cli` (e.g. IDE sessions that don't write `transcript.jsonl`) can only be measured via `agm ask`.
- ChatGPT quota consumed by OpenCode isn't in any log; agm sees it indirectly via Codex's next `rate_limits` record.
- Provider budgets only warn; they don't block `spawn` yet (agm doesn't know which provider a template uses).
- `agm sys` matches agents by process name (no PID comes from herdr), so it counts every Claude Code/Codex/OpenCode/agy process on the machine, not only herdr terminals; a child counts for its nearest agent ancestor (a worker started by a CEO counts for the worker). Sizes use 1 GB = 1024³ bytes, like Windows Task Manager.
- Claude/Codex hand-off is a convention, not a write lock on the old session — if you keep typing in the old session, both may edit the same files.
- An agent with a TTY that clears `HC_TERMINAL_ID` and types `y` itself would theoretically be treated as the user (agm doesn't inspect the parent process tree).
- `agm tui` is a periodically redrawn panel + command line; no keyboard navigation yet.

## 13. Development

```powershell
cargo test                                    # ~190 tests
cargo clippy --all-targets
python -I tools/argon_review.py               # Gemini Argon (via agy) review per module group
python -I tools/sanitize_fixture.py <log.jsonl> <fixture.jsonl>   # sanitize a real log into a fixture
```

| Path | Contents |
|---|---|
| `src/parse/`, `src/ingest.rs`, `src/opencode.rs`, `src/agy.rs` | Read Codex/Claude/agy logs (jsonl) and OpenCode (SQLite) |
| `src/store/` | SQLite database (WAL, schema v3) shared by daemon and CLI |
| `src/daemon.rs`, `src/hc.rs`, `src/correlate.rs` | herdr-core events, `hc` client, terminal ↔ session matching |
| `src/actions.rs`, `src/policy.rs`, `src/rotation.rs`, `src/launch.rs`, `src/git.rs` | Guarded send/reply/spawn/kill, budgets, rotation & hand-off, `launch_spec`, git checks |
| `src/hook.rs`, `src/ask.rs` | Claude statusline hook, one-shot agy |
| `src/report.rs`, `src/dashboard.rs`, `src/main.rs`, `src/ops.rs`, `src/tui.rs` | Reports & statistics, TUI, CLI |
| `docs/superpowers/specs/` · `docs/superpowers/plans/` · `docs/reviews/` | Design, plans, review reports |
| `.agents/decisions/p0-research.md` | Verified log formats, hc events, statusline |
| [AGENTS-TUTORIAL.md](AGENTS-TUTORIAL.md) · [HERDR-CORE-BOT.md](HERDR-CORE-BOT.md) | Agent guide, herdr-core bot notes (Vietnamese) |
| [CHANGELOG.md](CHANGELOG.md) | Version history |
