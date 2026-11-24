# approval-runtime log

Đưa grant đã duyệt vào runtime: `agm approval check` + MCP `agm_approval_check`. Task: `.agents/tasks/approval-runtime.md`.

## 2026-11-24

### State before
- Worktree: `D:\Project\agmag.wt\approval`, branch `feat/approval-runtime` (base `ecfcc79`).
- `src/actions.rs` còn `evaluate_delegated_approval` + `runtime_approval_decision` (chưa có ai gọi, luật cũ
  với `TASK_ONLY_GRANTS`); `agm approval add/ls/revoke` chỉ có ở CLI (TTY-only cho add/revoke).
- Chưa có `agm approval check`, chưa có tool MCP `agm_approval_check`; docs chưa nhắc approval.
- EOL: `src/main.rs`, `CHANGELOG.md` CRLF; các tệp còn lại LF. `git status` sạch.

### Plan (theo task)
1. Luật `decide` 5 nhánh trong `src/actions.rs` (bỏ `TASK_ONLY_GRANTS`/`evaluate_delegated_approval`).
2. `approval add` từ chối action hard-prohibited + lane/task/action rỗng; sửa câu lỗi revoke.
3. `agm approval check` + exit 5/6.
4. Tool MCP `agm_approval_check`.
5. Validate `delegated_actions` trong `Policy::validate`; dùng chung một hằng `HARD_PROHIBITED_ACTIONS`.
6. Docs: BOT-CEO.md §2.1, README/README_vi, CHANGELOG.

### Step 1: luật `decide` 5 nhánh (RED → GREEN)
- tests/actions_test.rs: bỏ test cũ `delegated_approval_evaluates_grant_scope_and_hard_blocks`; thêm helper
  `approval_cfg`/`approval_grant`/`approval_decision` và 9 test:
  `approval_rule_allows_global_whitelist_without_grant`,
  `approval_rule_hard_prohibited_beats_whitelist_and_grant`,
  `approval_rule_active_grant_allows_with_grant_source`, `approval_rule_expired_grant_requires_human`,
  `approval_rule_revoked_grant_requires_human`, `approval_rule_grant_for_other_lane_or_task_requires_human`,
  `approval_rule_needs_both_lane_and_task_to_use_a_grant`, `approval_rule_unknown_action_requires_human`,
  `approval_rule_empty_action_is_error` (action có khoảng trắng quanh vẫn khớp sau trim).
- Đỏ: `cannot find function decide in module actions` (API mới chưa có).
- src/actions.rs: xóa `evaluate_delegated_approval`, `runtime_approval_decision`, `TASK_ONLY_GRANTS`; thêm
  `HARD_PROHIBITED_ACTIONS` (10 mục), `is_hard_prohibited`, `ApprovalDecision::as_str`,
  `Decision { decision, source }` và `decide` đúng 5 nhánh (hard → global → grant active → requires_human).
- src/main.rs: `approval add` tạm chỉ từ chối hard-prohibited (thay lời gọi hàm đã xóa); bước 2 làm đủ.
- Xanh: `cargo test --test actions_test approval_rule` → 9 passed. `cargo fmt --check` sạch.
- EOL: actions.rs/tests LF, main.rs CRLF (giữ nguyên).
- Commit: `feat(approval): runtime decision rule for delegated grants`.

### Step 2: `approval add` từ chối đầu vào xấu + sửa lời nhắn revoke (RED → GREEN)
- tests/actions_test.rs: thêm `approval_grant_input_rejects_empty_fields_and_hard_prohibited` (lane/task/action
  rỗng hoặc chỉ khoảng trắng, `push`, `"  merge  "` → lỗi nêu đúng trường; action được trim khi hợp lệ).
- tests/cli_test.rs: thêm `approval_add_and_revoke_stay_tty_only_and_revoke_says_thu_hoi` (không TTY: add và
  revoke đều thoát 1; câu lỗi revoke có "TTY" + "thu hồi", không còn "được tạo").
- Đỏ: `validate_new_grant` chưa có; revoke vẫn in "…mới được tạo".
- src/actions.rs: `validate_new_grant(lane, task, action)` chỉ kiểm tra (không ghi), trả action đã trim;
  từ chối rỗng và hard-prohibited.
- src/main.rs: add gọi `validate_new_grant` trước `upsert_approval_grant` (sau kiểm TTY); revoke đổi câu lỗi
  thành "chỉ người dùng ở terminal (TTY) mới được thu hồi".
- Xanh: `cargo test --test actions_test approval_grant_input` 1 passed;
  `cargo test --test cli_test approval_add_and_revoke` 1 passed; `cargo fmt --check` sạch.
- Commit: `feat(approval): validate new grants and fix revoke TTY message`.

### Step 3: `agm approval check` (RED → GREEN)
- tests/cli_test.rs: thêm 5 test:
  `approval_check_hard_prohibited_exits_6_with_json` (exit 6, JSON đủ 5 khóa, source `hard`),
  `approval_check_requires_human_exits_5_with_json` (exit 5, lane/task giữ nguyên),
  `approval_check_global_whitelist_exits_0` (providers.toml `delegated_actions`, action có khoảng trắng
  quanh vẫn trim, source `global`), `approval_check_active_grant_exits_0_and_writes_nothing` (grant trong
  agm.db → `grant:<id>`, vẫn đúng 1 hàng sau khi check), `approval_check_empty_action_fails` (exit 1).
- Đỏ: `unrecognized subcommand 'check'` (exit 2).
- src/actions.rs: `Decision::to_json` (đúng 5 khóa `decision/action/lane/task/source`) và `human_line`;
  hằng `EXIT_APPROVAL_REQUIRES_HUMAN = 5`, `EXIT_APPROVAL_HARD_PROHIBITED = 6`; lỗi `ApprovalRefused`;
  `exit_code` ánh xạ 5/6 (giữ 3 cho `BriefUndelivered`).
- src/main.rs: `ApprovalCmd::Check { action, #[arg(long)] lane, #[arg(long)] task }` không đòi TTY; in JSON
  theo `--json`/non-TTY (hoặc một dòng khi TTY), ghi một hàng action `approval_check`, rồi trả
  `ApprovalRefused` khi không `allowed`. Chỉ đọc grant, không ghi grant.
- Xanh: `cargo test --test cli_test approval_check` → 5 passed. `cargo fmt --check` sạch.
- Commit: `feat(approval): add agm approval check with exit codes 5/6`.

### Step 4: tool MCP `agm_approval_check` (RED → GREEN)
- tests/mcp_test.rs: `tool_list_is_exactly_the_phase1_set` thêm `agm_approval_check` (thứ 8);
  `tools_list_has_schema_for_each_tool` thêm tool; `no_tool_call_ever_kills` gọi thêm tool (xác nhận read-only);
  thêm `approval_check_tool_returns_decision_json_and_reads_global_and_grants`: global whitelist (action trim),
  hard-prohibited, requires_human, grant còn hiệu lực (`grant:<id>`, vẫn 1 hàng grant), `action` rỗng,
  thiếu `action`, tham số lạ bị từ chối.
- Đỏ: `unknown tool: agm_approval_check`; danh sách tool thiếu mục thứ 8.
- src/mcp.rs: thêm định nghĩa tool (action bắt buộc; lane/task chuỗi tùy chọn; `additionalProperties:false`)
  và nhánh `call_tool` dùng `actions::decide` + `Decision::to_json`, chỉ đọc (không ghi action log/grant).
- Xanh: `cargo test --test mcp_test approval` 1 passed; `tool_list_is_exactly` 1 passed;
  `no_tool_call_ever_kills` 1 passed; `cargo fmt --check` sạch.
- Commit: `feat(mcp): add agm_approval_check read-only tool`.

### Step 5: validate `[policy].delegated_actions`, một hằng dùng chung (RED → GREEN)
- tests/config_test.rs: thêm `delegated_actions_reject_empty_and_hard_prohibited_entries` (entry `""`, `"  "`,
  `push`, `" merge "`, `grant_extension` → `Config::load` lỗi `Invalid [policy]` và câu lỗi nêu entry;
  hai action hợp lệ vẫn nạp được).
- Đỏ: entry rỗng/hard-prohibited được chấp nhận.
- src/config.rs: chuyển `HARD_PROHIBITED_ACTIONS` (10 mục) sang đây; `Policy::validate` từ chối entry
  rỗng/chỉ khoảng trắng và entry (sau trim) nằm trong danh sách cấm.
- src/actions.rs: dùng `crate::config::HARD_PROHIBITED_ACTIONS` (một nguồn duy nhất) qua
  `is_hard_prohibited`/`decide`.
- Xanh: `cargo test --test config_test delegated_actions` 1 passed; `invalid_ctx` 1 passed;
  `cargo test --test actions_test approval_rule` 9 passed; `cargo fmt --check` sạch.
- Commit: `feat(config): reject bad delegated_actions entries`.

### Step 6: docs (worker capped mid-step; finished by CEO)
- opencode/deepseek-v4.1-flash-free hit "Free usage limit reached" after editing BOT-CEO.md §2.1, README.md,
  README_vi.md (approval commands, exit codes 5/6, MCP 8 tools), HERDR-CORE-BOT.md, AGENTS-TUTORIAL.md and
  CHANGELOG.md; nothing was committed. CEO reviewed those edits, added the 0.5.0 note in docs/AGM-0.4.0-GUIDE.md.

### Gates (final, 2026-11-24, CEO)
- `cargo fmt --check` sạch; `cargo clippy --all-targets -- -D warnings` sạch; `cargo test` 664 passed, 0 failed;
  `python scripts/lint_harness.py` 0 errors. EOL: CHANGELOG.md, src/main.rs CRLF giữ nguyên; còn lại LF.

## Fix round 1

State before: HEAD `01003cf`, branch `feat/approval-runtime`, worktree sạch; review `.agents/reviews/approval-runtime-1.md`
(CHƯA ĐẠT) đã đọc. EOL trước mỗi commit: `src/main.rs`, `CHANGELOG.md` CRLF; các tệp sửa trong item này LF.

### Item 1 — một dạng chuẩn cho action/lane/task (RED → GREEN)
- RED: tests/actions_test.rs thêm 5 test (`approval_normalization_canonicalizes_action_and_keys`,
  `approval_hard_prohibited_ignores_case_and_surrounding_space`, `approval_delegated_actions_match_case_insensitively`,
  `approval_grant_input_normalizes_fields_and_rejects_hard_prohibited_case`,
  `approval_decide_normalizes_lane_task_and_action_for_grant_lookup`); tests/config_test.rs thêm `PUSH`, `Push`,
  `" push\n"` vào bảng entry bị từ chối; tests/cli_test.rs thêm 2 test (`approval_check_mixed_case_hard_prohibited_exits_6`,
  `approval_check_normalizes_action_lane_and_task`). Chạy `cargo test --test actions_test approval_` → đỏ
  (E0425 `normalize_action`, E0609 `ValidatedGrant.lane/task/action`).
- GREEN: src/actions.rs thêm `normalize_action` (trim + ASCII lowercase), `normalize_key` (trim), `ValidatedGrant`;
  `is_hard_prohibited`, `decide` (input + từng `delegated_actions` + lane/task), `validate_new_grant` dùng chung hàm chuẩn
  hóa; src/config.rs `Policy::validate` kiểm entry đã chuẩn hóa; src/main.rs (add/revoke/check) và src/mcp.rs in/kết quả
  đã chuẩn hóa, add lưu lane/task/action chuẩn; docs cập nhật hành động không phân biệt hoa/thường.
- `cargo test --test actions_test approval_` 15 passed; `cargo test --test config_test delegated_actions` 1 passed;
  `cargo test --test cli_test approval_check` 7 passed; `cargo test --test mcp_test approval` 1 passed;
  `cargo fmt --check` sạch. Item 4 của review (trim không đồng nhất) được phủ trọn bởi item này.
- Commit: `fix(approval): canonicalize actions, lanes and tasks everywhere`.

### Item 2 — add/revoke chỉ dành cho người (RED → GREEN)
- RED: tests/actions_test.rs thêm `approval_add_revoke_require_human_and_refuse_agent_in_hc_pty` (agent trong PTY hc
  có TTY vẫn bị từ chối; shell người + TTY được phép; không TTY hoặc hc get lỗi → fail closed). Chạy test → đỏ
  `E0425 cannot find function require_human in module actions`.
- GREEN: src/actions.rs thêm `require_human(store, hc, hc_terminal, tty, denied_message)` dùng chung
  `detect_caller`/`Caller::User`; src/board/mod.rs `require_human` gọi lại helper này (một cơ chế, lời báo lỗi riêng);
  src/main.rs nhánh Add/Revoke thay guard `stdin().is_terminal()` bằng helper + `HcCli` + `ops::hc_terminal()`,
  thông báo vẫn chứa `TTY`/`thu hồi` để test CLI cũ đúng.
- `cargo test --test actions_test approval_add_revoke_require_human` 1 passed;
  `cargo test --test cli_test approval_add_and_revoke` 1 passed; `cargo test --test board_runtime_test` 6 passed;
  `cargo fmt --check` sạch. EOL: main.rs CRLF, các tệp khác LF.
- Commit: `fix(approval): require human caller for add and revoke`.
