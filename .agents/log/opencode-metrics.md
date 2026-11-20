# opencode-metrics — execution log

## 2026-11-20 — 1. Reconnaissance and scope

- Read task and Global Constraints; clean worktree on `feat/opencode-metrics`.
- No agents, dependency/schema changes, push, merge, or worktree moves.
- Confirmed `opencode api --help`: read-only GET requests to running server.
- `opencode api GET /api/model`: 401 ms (second measured call), envelope
  `{ "location": ..., "data": [...] }`, 237 models. Retain only providerID,
  id and limit.context; never log settings/headers (response can contain secrets).
- Sanitized examples: `opencode/iceman-free` 262144,
  `opencode-go/kimi-k3` 1048576, `opencode-go/deepseek-v4.1-flash` 1000000.
- Chosen source: fresh running-server catalog, once per process, bounded CLI
  request. Do not rely on stale models.json; unavailable/missing => None.
- Test boundaries already specified by task: report sessions, policy tick,
  configuration precedence and catalog parsing. Synthetic catalog fixtures only.
- Plan: catalog/config red-green; report red-green; policy red-green;
  zero-token warnings red-green; read-only quota investigation; docs and checks.
  Preserve recorded log/hook windows for non-OpenCode sources.

## 2. Catalog/config RED

- `cargo test --offline --test opencode_models_test`: FAIL (exit 1),
  missing catalog module, config override and context lookup (9 compile errors).
- Added minimal catalog parser, provider-qualified lookup, process-wide lazy
  cache (including failure), 5s bounded GET via installed CLI, shared config
  lookup. Fixture override is in memory only, excluded from TOML.
- In parallel, read-only investigation discovered the documented
  `GET /api/experimental/usage/limits`: actual rolling/weekly/monthly Go meters.
  This task will document it, not install quota ingestion.

## 3. Catalog/config GREEN; report RED

- `cargo test --offline --test opencode_models_test`: 1 passed, 0 failed.
- Added report-boundary test for known and absent catalog models (25% / None).
- RED: same command, 1 passed / 1 failed (`None` instead of 25%).
- Wired report fallback to shared source-aware lookup; stored windows unchanged.
- First build caught a moved Source; changed helper to borrow Source.
- GREEN: `cargo test --offline --test opencode_models_test`: 2 passed / 0 failed.

## 4. Policy RED

- Added public `policy::tick` test: catalog-only 75% opens context warning inbox.

## 5. BLOCKED — concurrent file replacement

- Next `cargo test --offline --test opencode_models_test` failed to compile:
  `catalog()` and `context_window()` disappeared from `src/opencode_models.rs`;
  the replacement also has incompatible `&Vec<Value>` / `&[]` branches.
- Re-read the file: it is now an unrelated 87-line implementation with
  `ModelCatalog::cached()/context()` instead of the implementation written and
  verified in steps 2–3. This session did not make that replacement and spawned
  no agents. Do not overwrite or commit another writer's code.
- Stopping for ownership clarification. Commit this log only; preserve all
  uncommitted source/test changes. Policy implementation, zero-token warning,
  README updates, full tests/clippy/fmt and real `ls` acceptance remain undone.
- Last successful targeted run: 2 passed / 0 failed. Latest run: compile failure,
  no tests executed. Earlier intentional report RED: 1 passed / 1 failed.

### Read-only quota findings to preserve for the resumed task

- `opencode api GET /openapi.json`: 681 ms; documented operation
  `experimental.usage.limits`, GET `/api/experimental/usage/limits`.
  Description explicitly says OpenCode Go allowance and Console budgets;
  unsigned-in workspace => null.
- `opencode api GET /api/experimental/usage/limits`: 696 ms, success.
  Sanitized data (workspace identity/URL removed):
  ```json
  {"go":{
    "rolling":{"percent":100,"limited":true,"resetsAt":1795156077000},
    "weekly":{"percent":40,"limited":false,"resetsAt":1795392000000},
    "monthly":{"percent":20,"limited":false,"resetsAt":1797728748000}
  },"budgets":[]}
  ```
- OpenAPI `UsageLimits.Meter`: percent in 0–100, limited boolean,
  resetsAt explicitly milliseconds since Unix epoch.
- `opencode stats --json`: 6155 ms. Top-level keys:
  range,sessions,subagents,prompts,steps,tokens,cost,tools,activeDays,streak,activity,models;
  no allowance windows.
- These commands issue no generation request (0 generated tokens by operation
  semantics; not independently measured via token counters).
- No auth.json, credential/account/control_account tables, browser sessions or
  cookies read. No quota ingestion installed.
- Resume: write the required decision note; map rolling to 300, weekly to
  10080, percent directly to used_percent, resetsAt / 1000. Monthly window
  representation needs an explicit convention, not a guessed fixed month length.
