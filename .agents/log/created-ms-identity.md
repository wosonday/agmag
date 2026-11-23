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

### Step 2: daemon sửa lại hàng đã ghi sai (RED → GREEN)
- tests/daemon_test.rs: thêm 9 test quanh `resync_created_ms`:
  - dương: `daemon_resyncs_created_ms_for_agm_owned_terminal` (DB 1000, ảnh chụp herdr 1500 → DB
    1500 + action `resync_created_ms` detail `1000 → 1500`), `daemon_resync_allows_missing_db_cwd`.
  - âm: không phải hàng agm mở, khác app, herdr không báo app, khác cwd, lệch âm, lệch > 60 s,
    herdr không còn `live`.
  - Đỏ: hai test dương giữ 1000 (7 test âm xanh sẵn vì `upsert_terminal` không sửa `created_ms`).
- src/daemon.rs: `apply_terminal` gọi `resync_created_ms(store, db, fresh, now)` trước upsert.
  Hàm mới chỉ UPDATE `created_ms` khi: cùng address, herdr `live`, cùng app khác rỗng, cùng cwd theo
  `correlate::norm_cwd` (hoặc DB chưa có cwd), hàng do agm mở (`managed`/`launch_spec`/`template`),
  và `0 < hc − db ≤ 60_000` ms; mỗi lần sửa ghi action `agm/resync_created_ms`.
  `upsert_terminal` giữ nguyên hành vi.
- Xanh: daemon_test 22/22.
- Commit: `fix(identity): resync wrong created_ms from herdr snapshots`.

### Step 3: test đầu-cuối cleanup (RED → GREEN)
- tests/actions_test.rs: thêm `cleanup_after_daemon_resync_fixes_legacy_created_ms`.
  Dựng rotation `t7p → t8c` (`done`, `handed_off`, t8c linked + managed); `created_ms` của t8c trong
  DB là `1795450665328`, herdr báo `1795450665482` (lệch 154 ms). Áp một ảnh chụp daemon → DB phải
  sửa thành giờ herdr; `eligible_cleanups` + `execute_cleanup_batch` phải `done=1`, `skipped` rỗng
  và `kill t7p`.
- Viết test này TRƯỚC khi code bước 2: đỏ ở assert `t8c` giữ 1795450665328 (đúng triệu chứng thật).
  Sau bước 2 xanh.
- Không nới điều kiện so khớp `created_ms` trong `fresh_cleanup_terminal` (actions.rs ~1031) hay
  điều kiện phiên kế nhiệm (~1118).
- Commit: `test(identity): cleanup works after daemon resyncs created_ms`.

### Gates (final, 2026-11-24)
- `cargo test`: tất cả test binary xanh (0 failed), gồm actions_test 36/36, rotation_test 34/34,
  daemon_test 22/22.
- `cargo fmt --check`: sạch.
- `cargo clippy --all-targets -- -D warnings`: sạch.
- Không push / merge / đổi branch; làm việc trên `fix/created-ms-identity` tại
  D:\Project\agmag.wt\created-ms.
