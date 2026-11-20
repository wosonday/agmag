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

## 2026-11-20 - 2. Implementation

- Implemented `src/opencode_models.rs`: `ModelCatalog::from_json/from_value/context/load/cached` (Windows uses `opencode.cmd` shim).
- `Config::context_window(source, name)`: `providers.toml` full/bare key wins, then OpenCode catalog for `Source::OpenCode`.
- `report.rs`: context lookup uses `cfg.context_window`; OpenCode zero-token sessions no longer get `unknown format?`.
- `policy.rs`: `session_context_pct` queries source and uses `cfg.context_window`.
- Survey: `opencode api GET /api/experimental/usage/limits` returns Go rolling/weekly/monthly percent + resetsAt; decision file written.
- `README.md` and `README_vi.md` updated.
- Real run: `cargo run --offline -- ls --json` -> 96 sessions, 58 OpenCode sessions with `context_pct`.

## 2026-11-20 - 3. Verification

- `cargo test --offline`: 27 test suites, all passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo fmt --check`: our changed Rust files are formatted; pre-existing files in baseline remain non-clean under this rustfmt version.
- Real run: `cargo run --offline -- ls --json` -> 96 sessions (77 OpenCode, 10 agy, 9 Claude); 58 OpenCode sessions have `context_pct` from catalog/config.
- `opencode api GET /api/experimental/usage/limits`: ~0.84 s, 0 model tokens, returns Go rolling/weekly/monthly percent + resetsAt.

## 2026-11-20 - 4. Resumed after free-limit stop (review, fix, verify)

- Recovered the uncommitted tree from the stopped worker: read `git status`, both diffs and this log, preserved every staged/unstaged change, no reset or deletion.
- Scope fix: reverted the eager `ModelCatalog::cached()` assignment in `src/main.rs` (not in the task's Files list, and it spawned `opencode api` on every CLI call). The catalog is now loaded lazily, once per process, only when an OpenCode lookup misses `providers.toml`.
- Simplified `Config::context_window`: exact `provider/id` then bare id via `model()`, then catalog only for `Source::OpenCode`; other sources return `None` exactly as before.
- `policy.rs`: local `type Row` alias (clippy `type_complexity`), source parsed once, window via `cfg.context_window`.
- `opencode_models::load_with`: bounded 5 s read via reader thread (the 237-model response exceeds a pipe buffer, so waiting for exit before reading would deadlock), then kill on timeout.
- Missing acceptance tests added: zero-token OpenCode session has no warning while Codex keeps `unknown format?`; malformed JSON is an error and a catalog entry without `limit.context` is ignored.
- RED check for the warning test: temporarily dropped the `source != Source::OpenCode` exemption → `cargo test --offline --test opencode_models_test zero_token` failed (`left: Some("unknown format?")`); restored → 5 passed / 0 failed.
- Docs: `README_vi.md` line rewritten with Vietnamese diacritics; `CHANGELOG.md` adds the catalog feature and the removed false warning; decision note typo and blank line fixed.
- Verification: `cargo test --offline` → 259 passed / 0 failed across 27 test binaries (two targets have 0 tests, incl. doc-tests); `cargo clippy --offline --all-targets -- -D warnings` clean.
- Format: `src/opencode_models.rs`, `tests/opencode_models_test.rs` and every line added/changed here are rustfmt-clean. `cargo fmt --check` still reports pre-existing files at HEAD (e.g. `src/actions.rs`, `src/ask.rs`, `src/rotation.rs`, `src/store/v2.rs`); it fails both with rustfmt 1.99.1 and 1.97.1, so it is not caused by this task and the whole tree was not reformatted (Files scope).
- Real run: `cargo run --offline -- ls --json` → 101 sessions (opencode 86, 67 with `context_pct`; claude 8, 7; agy 7, 0). `opencode-go/kimi-k3` gets `context_pct` (5.2 %, 1.2 %) from the 1 048 576-token catalog entry; only one agy session keeps `unknown format?`; live catalog `GET /api/model` returned 237 models.
