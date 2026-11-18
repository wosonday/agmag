# agmag Kế hoạch 2: Hoàn thiện v1 (P2–P5) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Người thực thi: Claude (inline) — hạn mức tuần ChatGPT còn < 14% nên không giao thợ OpenCode `gpt-6.1-sol`. Review cuối: Gemini Argon qua `tools/argon_review.py` + reviewer Claude.

**Goal:** Đủ mục tiêu v1 của spec §2: đo OpenCode + agy, hạn mức Claude qua statusline, daemon theo dõi `hc`, hộp thư người dùng, gửi/trả lời tin có ghi lại, spawn có hạn mức, ngân sách, xoay vòng thợ OpenCode (§5.9), bảo vệ Codex/Claude khỏi bị kill tự động, TUI.

**Spec:** `docs/superpowers/specs/2026-11-18-agmag-design.md` · **P0:** `.agents/decisions/p0-research.md`

## Quyết định thiết kế mới (Ruling, ghi lại để người dùng kiểm)

1. **Nối `hc` bằng tiến trình con `hc <lệnh> --json`** và luồng `hc events --json`, không tự viết client JSON-RPC cho `hc connect`. Đầu ra `--json` là đúng các đối tượng của API (`hc schema`), nên vẫn thỏa "không phân tích văn bản". Giá: mỗi lệnh một tiến trình (vài chục ms).
2. **Không có IPC daemon↔CLI.** Daemon chỉ quan sát + chạy policy; CLI tự gọi `hc` và ghi SQLite (WAL, `busy_timeout` 5 s). Đúng chế độ dự phòng của spec §5.1, bỏ một lớp dễ hỏng.
3. Mọi lời gọi `hc` đi qua trait `Hc` để test bằng bản giả.
4. OpenCode: đọc `opencode.db` **chỉ đọc**, chỉ bảng `session_v2`, `session_message`; con trỏ = `max(time_updated)` đã thấy (bảng `cursors`, khóa `opencode-db`). Mỗi phiên OpenCode là một hàng `sessions` với `path = "opencode://<id>"`.
5. `gpt-6.1-sol` có `context_window = 475000` trong cấu hình mặc định (đã thấy trong log Codex `model_context_window`).
6. Hạn mức Claude: `agm hook statusline` đọc JSON statusline → quota provider `claude` (cửa sổ 300/10080) + `ctx_window` cho phiên có `transcript_path` + ghép terminal (`HC_TERMINAL_ID`). Lệnh cài (`agm hook install`) có sẵn nhưng **không tự chạy** — sửa `~/.claude/settings.json` là việc của người dùng.
7. "Người dùng ra lệnh" = stdin là TTY **và** (không có `HC_TERMINAL_ID` **hoặc** terminal gọi không phải agent: `reported=false`) **và** xác nhận y/N (hoặc `--yes`, chỉ hợp lệ khi đã thỏa hai điều trước).

## Files

| Tệp | Trách nhiệm |
|---|---|
| `src/config.rs` | + `cost_usd`; + mục `[policy]` (max_workers, manager, ctx_warn 50, ctx_rotate 70, handoff_timeout_min 10) |
| `src/model.rs` | `Source::{OpenCode, Agy}` |
| `src/parse/opencode.rs` | đọc `opencode.db` → `ParseEvent` theo phiên |
| `src/store/schema_v2.sql`, `src/store/mod.rs` | bảng `terminals`, `messages`, `inbox`, `budgets`, `actions`, `rotations`, `settings`; migration `user_version` 1→2 |
| `src/hc.rs` | trait `Hc` + `HcCli` (tiến trình con) + kiểu `Terminal` |
| `src/hook.rs` | `agm hook statusline` |
| `src/daemon.rs` | `apply_event`, vòng lặp sự kiện + quét định kỳ |
| `src/correlate.rs` | ghép terminal ↔ phiên |
| `src/policy.rs` | ngân sách, cảnh báo ngữ cảnh, máy trạng thái xoay vòng |
| `src/actions.rs` | send / reply / spawn / kill / interrupt / rotate + kiểm "người dùng ra lệnh" |
| `src/ask.rs` | `agm ask` (agy một lần, ghi token) |
| `src/tui.rs` | `agm tui` (ratatui) |
| `tools/argon_review.py` | gửi mã cho Gemini Argon review theo từng khúc |

## Tasks (mỗi task: test đỏ → code → xanh → commit `v1-<n>: …`)

1. **cost_usd + config policy** — tests: `cost_basic` (bỏ ignore), `policy_defaults` (max_workers 3, ctx_warn 50, ctx_rotate 70, timeout 10), `gpt_sol_has_window`.
2. **OpenCode ingest** — `ingest_opencode(store, db_path) -> IngestStats`; fixture SQLite tạo trong test. Tests: `opencode_usage_normalized` (input 191, cache.read 45440 → input_uncached 191, cache_read 45440), `opencode_incremental` (chạy hai lần không nhân đôi), `opencode_turns`, `opencode_missing_db_is_ok`. Gọi trong `ingest_all` khi `Roots.opencode` tồn tại.
3. **Store v2** — tests: `migrates_v1_to_v2`, `inbox_open_dedupes` (cùng terminal+kind+msg đang mở → không thêm), `message_roundtrip`, `settings_roundtrip`, `action_logged`.
4. **Hook statusline** — `hook::statusline(store, json, hc_terminal) -> String`. Tests: `records_claude_quota` (five_hour 23.5 → quota claude 300), `sets_ctx_window_for_transcript`, `missing_rate_limits_ok`, `prints_compact_line`.
5. **hc client + daemon apply_event** — `Hc` trait (`ls`, `get`, `send`, `run`, `kill`, `interrupt`), `apply_event(store, &Value, now)`. Tests: `created_upserts_terminal`, `blocked_question_opens_inbox` (msg nguyên văn), `unblocked_resolves_inbox`, `finished_sets_exit`, `snapshot_line_upserts_all`, `labels_set_task_and_role`.
6. **Correlate** — tests: `unique_cwd_time_match_links`, `ambiguous_not_linked`, `hook_link_wins`.
7. **Actions** — `send`, `reply`, `spawn`, `kill`, `interrupt` trên `&dyn Hc`. Tests: `send_to_user_goes_to_inbox`, `send_records_message_and_calls_hc_if_idle`, `spawn_refuses_over_max_workers`, `spawn_sets_labels_and_outlive_parent`, `agent_cannot_kill_codex_or_claude` (lệnh từ agent → từ chối + inbox), `agent_can_kill_opencode_worker`, `user_kill_requires_confirmation`.
8. **Policy: ngân sách + cảnh báo ngữ cảnh** — tests: `task_soft_budget_opens_inbox_once`, `hard_budget_blocks_spawn`, `hard_action_never_kills_codex_claude`, `ctx_warn_at_50`, `policy_off_does_nothing`.
9. **Xoay vòng (§5.9)** — máy trạng thái `rotations`: `pending → requested → verified → killed → respawned | failed`. Tests: `rotation_waits_while_working`, `rotation_requests_handoff_when_idle`, `rotation_kills_and_respawns_after_commit`, `rotation_fails_to_inbox_without_commit`, `rotation_timeout_to_inbox`, `rotation_only_for_opencode_workers`.
10. **agm ask** — tests: `ask_records_agy_usage` (đầu ra agy giả), `ask_error_status_is_error`.
11. **Daemon loop + CLI** — `agm daemon [--interval 10]`, `send`, `inbox`, `reply`, `spawn`, `kill`, `interrupt`, `rotate`, `budget set|ls|rm`, `policy on|off`, `link`, `ask`, `hook statusline|install`. Tests CLI: `inbox_json`, `budget_set_ls_json`, `policy_toggle`.
12. **TUI** — `agm tui`: hạn mức gói, terminals, ngân sách, hộp thư, tin gần đây; phím `s r i k b q`. Logic dựng dữ liệu (`tui::snapshot(store,cfg,now)`) có test `snapshot_sections`; phần vẽ kiểm tay.
13. **Review** — `tools/argon_review.py` gửi từng mô-đun cho Gemini Argon; sửa Critical/Important bằng test đỏ trước; thêm review Claude cho toàn nhánh.
