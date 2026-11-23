# created-ms-identity log

Sửa `created_ms` sai khiến `agm cleanup` bỏ qua mọi cặp bàn giao. Task: `.agents/tasks/created-ms-identity.md`.

## 2026-11-24

### State before
- Worktree: D:\Project\agmag.wt\created-ms, branch fix/created-ms-identity (base 41a702b).
- `created_ms` sai được ghi ở 3 chỗ: `actions::spawn`, nhánh spawn successor của `rotation`,
  nhánh phục hồi `"spawning"`. `Store::upsert_terminal` không bao giờ sửa lại giá trị đã có.

### Step 1: ghi `created_ms` thật của herdr khi spawn (RED → GREEN)
- tests/actions_test.rs: thêm `spawn_stamps_herdr_created_ms_not_tick_time` (FakeHc `run` không có
  `created_ms`, `get t99` có `created_ms=10_500`, tick now=20_000 → DB phải là 10_500) và
  `spawn_created_ms_falls_back_to_tick_time_when_get_fails` (`get` lỗi → 20_000).
  Đỏ: DB ghi 20_000.
- tests/rotation_test.rs: thêm `spawning_recovery_prefers_herdr_created_ms` (rotation `spawning`,
  hàng kế nhiệm trong store có `created_ms=5_000`, FakeHc `get t99` báo 12_345 → DB phải 12_345).
  Đỏ: DB giữ 5_000.
- src/actions.rs: thêm `herdr_created_ms` (private), `stamp_created_ms` (chỉ khi hàng chưa có giờ) và
  `refresh_created_ms` (nhánh phục hồi luôn ưu tiên herdr). `spawn` dùng `stamp_created_ms`.
- src/rotation.rs: nhánh spawn successor dùng `actions::stamp_created_ms`; nhánh `"spawning"` dùng
  `actions::refresh_created_ms` và ghi đè DB bằng `set_terminal_created_ms` khi giá trị đổi
  (`upsert_terminal` không sửa hàng đã có).
- src/store/v2.rs: thêm `Store::set_terminal_created_ms` (chỉ UPDATE một cột; không đổi
  `upsert_terminal`).
- Xanh: actions_test 7/7 test khớp `spawn_`, rotation_test 34/34.
- Commit: `fix(identity): stamp created_ms from herdr when spawning terminals`.
