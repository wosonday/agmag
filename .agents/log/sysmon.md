# sysmon log — machine RAM / CPU / disk tracking

## 2026-11-20 · goal and scope (ceo-pc)
- Goal: show live machine load so the CEO can tell whether the machine can take more workers. Serves: cost.
- Scope confirmed by the user: whole machine + per agent + disks. No history in the database (no migration).
- Branch `feat/sysmon` from `main` at `a7ce50f`.

## 2026-11-20 · implementation
- New dependency `sysinfo 0.40.4` (`system`, `disk` features only).
- `src/sysmon.rs`: `SysSampler` (keeps OS handles between readings), pure `agent_of` / `attribute` / `merge_disks`.
- `agm sys` (handled before the database opens; `--json`), `agm tui` header `cpu · ram · fullest disk`, selectable `system` section.
- Docs: README.md, README_vi.md (commands, TUI views, known limitations), CHANGELOG.

## 2026-11-20 · findings while verifying on Windows 11
- sysinfo keeps no CPU baseline on a process's first refresh, so the second refresh reports the lifetime average
  (a busy loop under claude-code showed 0.1%). Fixed with a second baseline refresh in `SysSampler::new`;
  re-check showed 4.6% for the claude-code tree (busy loop on 12 logical cores).
- Google Drive mounts one volume as G:, H:, I: (identical FS/size/free bytes). `merge_disks` folds them into one row
  (`also_mounted_at`) so the header does not count the same volume three times.
- herdr gives no PIDs, so agents are matched by process name; desktop apps are labelled `*-app`.

## 2026-11-20 · verification
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: all suites pass, including 8 new tests in `tests/sysmon_test.rs`.
- `python -B scripts/lint_harness.py`: 0 errors; `python -B -m unittest discover -s tests`: 87 OK.
- Not yet: commit, Reviewer gate, `copilot-git` merge (user decides).

## 2026-11-20 · colors and decoration (user request)
- `agm sys` now prints a colored report (MACHINE / DISKS / AGENTS) via `dashboard::render_sys_report`, sized to the
  pane width; `NO_COLOR` strips SGR. The TUI SYSTEM section shares the same line builders.
- Load verdict line: `● OK` / `● BUSY` (≥ 60%) / `● HIGH LOAD` (≥ 85%) from the busier of CPU and RAM, plus
  `⚠ <disk> nearly full` at 90% for fixed disks.
- Agents keep their TERMINALS colors; RAM-share bar uses 1/8-cell steps so 1–5% shares stay visible.
- Verification: clippy `-D warnings` clean, all Rust suites pass (9 sysmon tests), harness lint 0 errors.

## 2026-11-20 · per-core CPU (user request)
- `CpuStats.per_core_percent` from `sysinfo::Cpu::usage`; CPU line detail shows busiest core and idle (<10%) count.
- `agm sys`: grid of every logical core under the CPU bar (columns fit the width); TUI: one spark column per core.
- Verification: clippy `-D warnings` clean, all Rust suites pass (9 sysmon tests incl. grid/spark/narrow width).
