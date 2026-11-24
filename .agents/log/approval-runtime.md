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
