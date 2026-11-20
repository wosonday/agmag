# opencode-go-quota log

## Step 1 - parse + record (TDD)
- Added `tests/opencode_quota_test.rs` with parse/record cases: 3 meters (300/10080/43200), data:null no-op, missing go/unknown meter/missing fields counted as unknown without fake writes, percent clamp 0..100, bad JSON error with no writes.
- RED: `cargo test --offline --test opencode_quota_test` -> 4 failed (todo stubs).
- GREEN: implemented `src/opencode_quota.rs` (`parse_usage`, `record_usage` with quota_limited settings, fetch/poller skeleton following codex_quota) and registered `pub mod opencode_quota` in `src/lib.rs`.
- `cargo test --offline --test opencode_quota_test` -> 4 passed, 0 failed.

## Step 2 - fetch timeout/kill (TDD)
- Added hanging-binary test in `tests/opencode_quota_test.rs` using `AGM_OPENCODE_BIN`; asserts return within <= 8s with timeout error and process tree killed.
- Implemented `fetch_usage` with 5s common deadline, stdout reader thread, `try_wait` deadline loop, and `kill_tree` on timeout/error (no join of reader on timeout).
- `cargo test --offline --test opencode_quota_test` -> 5 passed.

## Step 3 - config + daemon/refresh wiring
- Added `[policy].opencode_quota_poll_min` (default 15, serde default) in `src/config.rs`.
- Wired `OpenCodeQuotaPoller` into daemon loop in `src/main.rs` and added `agm quota --refresh` fetch/record for `opencode-go`.
- `cargo test --offline -q` -> all existing tests + opencode_quota tests pass.

## Step 4 - QuotaRow.limited + stale for opencode-go
- Added `limited` to `report::QuotaRow` (populated from `quota_limited:{provider}:{window}` setting for display/json).
- Extended `quota_is_stale` usage mapping for provider `opencode-go`: stale when newer usage exists in OpenCode sessions with model prefix `opencode-go/` (all variants treated as same package; documented intent).
- Updated dashboard tests constructing `QuotaRow` literals for new field.
- `cargo test --offline -q` -> pass.

## Step 5 - display (mo/limited) and tests
- CLI quota: `window_label` shows `mo` for 43200; print adds red `limited` label when set.
- TUI/dashboard: quota rows show `mo` for 43200, provider width fits `opencode-go`, red `limited` label, stale/finite handling preserved; fits 60/110 cols.
- Added tests: opencode-go stale by age/newer `opencode-go/*` usage, budget `provider_percent` for `opencode-go:300` (stale -> no data), dashboard render of 5h/7d/mo + limited + stale at 60/110 cols.
- `cargo test --offline -q` -> pass.

## Step 6 - quality checks
- Ran `cargo fmt` on modified Rust files, fixed clippy `useless_format` in `src/main.rs`.
- `cargo test --offline` -> pass; `cargo clippy --offline --all-targets -- -D warnings` -> pass; `cargo fmt --check` (modified files) -> pass.

## Step 7 - real run (temp AGM_HOME)
- AGM_HOME: C:\Users\thanh\AppData\Local\Temp\agmag-opencode-go-quota-ecb3caafb0e84795925ab4bc99af4293
- Command: `cargo run --offline -- quota --refresh`
- Output:
``text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.24s
     Running `target\debug\agm.exe quota --refresh`
[
  {
    "provider": "agy-3p",
    "window_minutes": 10080,
    "used_percent": 100.0,
    "left_percent": 0.0,
    "resets_at_s": 1795650494,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "agy-argon",
    "window_minutes": 10080,
    "used_percent": 100.0,
    "left_percent": 0.0,
    "resets_at_s": 1795577112,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "agy-gemini",
    "window_minutes": 10080,
    "used_percent": 100.0,
    "left_percent": 0.0,
    "resets_at_s": 1795577096,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "codex",
    "window_minutes": 300,
    "used_percent": 100.0,
    "left_percent": 0.0,
    "resets_at_s": 1795166293,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "codex",
    "window_minutes": 10080,
    "used_percent": 93.0,
    "left_percent": 7.0,
    "resets_at_s": 1795599241,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "opencode-go",
    "window_minutes": 300,
    "used_percent": 0.0,
    "left_percent": 100.0,
    "resets_at_s": 1795177392,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "opencode-go",
    "window_minutes": 10080,
    "used_percent": 40.0,
    "left_percent": 60.0,
    "resets_at_s": 1795392000,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  },
  {
    "provider": "opencode-go",
    "window_minutes": 43200,
    "used_percent": 20.0,
    "left_percent": 80.0,
    "resets_at_s": 1797728748,
    "observed_ms": 1795159367498,
    "reset_passed": false,
    "stale": false,
    "observed_age_s": 0,
    "limited": false
  }
]
``
- Exit: 0
- Reference: `opencode api GET /api/experimental/usage/limits`
``text
{"location":{"directory":"C:\\Users\\thanh"},"data":{"workspace":{"id":"org_<redacted>","name":"<redacted>","usageURL":"<redacted>"},"go":{"rolling":{"percent":0,"limited":false,"resetsAt":1795177392862},"weekly":{"percent":40,"limited":false,"resetsAt":1795392000000},"monthly":{"percent":20,"limited":false,"resetsAt":1797728748000}},"budgets":[]}}
``

## Step 8 - docs and decision
- Updated `.agents/decisions/opencode-go-quota.md` (installed, monthly window 43200, stale mapping treats all `opencode-go/*` as same package, Go Console for Extra Usage/budget).
- Updated `README.md`, `README_vi.md` (quota includes OpenCode Go), `CHANGELOG.md` (added OpenCode Go quota entry), `HERDR-CORE-BOT.md` (quota now reads Go; provider budget note; Go Console for Extra Usage/budget).
