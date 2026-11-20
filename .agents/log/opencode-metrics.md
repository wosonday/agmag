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

## 2026-11-20 - 5. Vòng sửa 1 (review `f4ed2fe`)

### #1 (chặn) — hạn chót cho toàn bộ `load_with`

- TDD RED: thêm test `load_with_bails_within_deadline_when_cli_holds_stdout` (fake `.cmd` Windows: `ping -n 30 127.0.0.1` giữ stdout; Unix: `exec sleep 30`). Chạy riêng test → FAIL sau **29.28 s** (`load_with returned after 29.28s, deadline was not enforced`): đúng lỗi `reader.join()` không giới hạn sau khi chỉ kill tiến trình trực tiếp.
- Sửa `src/opencode_models.rs`: một `Instant` hạn chót chung 5 s tính từ lúc spawn, dùng cho cả `recv_timeout` (chờ dữ liệu) và vòng `try_wait()` 20 ms (chờ thoát). Hết hạn hoặc `try_wait` lỗi → `kill_tree()` rồi trả `Err`, **không join** luồng đọc.
- `kill_tree`: Windows chạy `taskkill /T /F /PID <pid>` (diệt cả cây `cmd.exe` → `node.exe`) rồi `child.kill()`/`child.wait()` làm dự phòng; nền tảng khác chỉ `kill`/`wait`. Không thêm crate.
- `load_with` chuyển thành `pub` để test chỉ định binary giả lập (đúng gợi ý review; không dùng biến môi trường).
- GREEN: 8/8 test trong `tests/opencode_models_test.rs`, test timeout trả `Err` trong ~5 s (toàn bộ tệp 5.37 s).
- Test thêm `load_with_returns_catalog_from_cli_json` (fake CLI trả JSON hợp lệ → `Ok`, 1 000 000 token).

### #2 (nên sửa) — test bổ sung

- `ctx_rotate_triggers_from_catalog_only_window` (trong `tests/rotation_test.rs`): terminal opencode worker, model `opencode-go/fixture-kimi` chỉ có trong danh mục fixture, `used_tokens = 800 000`, `window_tokens = None` → `rotation::tick` tạo vòng `pending` với `ctx_pct = 80.0`. Đây là test bảo vệ (tính năng đã đúng từ vòng trước), không phải RED.
- `zero_token_claude_and_agy_keep_the_unknown_format_warning`: hồi quy `unknown format?` cho Claude và agy (Codex đã có ở test cũ). Cũng là test bảo vệ.
- Bỏ test "chỉ tải một lần": cần thao tác trạng thái `OnceLock` toàn cục, đúng điều kiện "chỉ làm nếu không cần hack trạng thái toàn cục" → không làm.

### #3 (nhỏ) — tệp quyết định

- Thêm `\n` cuối `.agents/decisions/opencode-go-quota.md` (byte cuối trước: `.`; sau: `0x0A`).

### Kiểm chứng

- `cargo test --offline`: **263 passed / 0 failed** (27 binary; trước vòng sửa 259 → +4 test mới).
- `cargo clippy --offline --all-targets -- -D warnings`: sạch.
- `rustfmt --edition 2024 --check`: `src/opencode_models.rs` và `tests/opencode_models_test.rs` sạch (`--config skip_children=true` để không kéo `tests/common/mod.rs` có sẵn ở baseline). `tests/rotation_test.rs` có sẵn nhiều lệch fmt từ baseline; riêng khối test mới được kiểm bằng cách trích ra tệp tạm → sạch. Không fmt cả cây.
- Chạy thật: `cargo run --offline -- ls --json` → 1.40 s, 104 phiên; opencode 89 (70 có `context_pct`), claude 8 (7), agy 7 (0). Ví dụ `opencode/deepseek-v4.1-flash-free` 34.3 %, `opencode/iceman-free` 43.8 %, `openai/gpt-6.1-sol` 60.2 %. Không còn treo khi CLI chậm/treo.

