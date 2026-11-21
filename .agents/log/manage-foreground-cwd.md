# Log — manage-foreground-cwd

Nhánh: `fix/manage-foreground-cwd`, worktree `D:/Project/agmag.wt/manage-foreground-cwd`.

## Bước 1 — test đỏ (2026-11-21)

Đã thêm test (chưa có code):

- `tests/daemon_test.rs::effective_cwd_prefers_foreground_then_root` — `foreground.cwd` thắng
  `cwd`; rỗng/thiếu/không phải đối tượng → `cwd`; cả hai thiếu hoặc rỗng → `None`.
- `tests/rotation_test.rs::rotation_start_uses_launch_spec_cwd_over_terminal_cwd` — terminal có
  `launch_spec.cwd = D:\Project\other`, `terminals.cwd = D:\Project\shell-root` →
  `rotations.cwd = D:\Project\other` sau `rotation::start`.
- `tests/rotation_test.rs::rotation_start_without_launch_spec_keeps_terminal_cwd` — không có
  `launch_spec` → giữ `terminals.cwd` (`D:\w`) như cũ.
- `tests/rotation_test.rs::managed_handoff_file_lands_in_launch_spec_cwd` — tick tới `requested`,
  tệp bàn giao phải nằm trong thư mục `launch_spec`, không nằm trong thư mục shell.

Trạng thái đỏ: `cargo test --offline --test daemon_test --test rotation_test` →
`error[E0432]: unresolved import agmag::hc::effective_cwd` (đúng như mong đợi).

Ghi chú phạm vi: không có test CLI/fixture `hc get` giả cho `manage` (test CLI hiện chỉ trỏ
`AGM_HC_BIN` vào đường dẫn không tồn tại; không có exe giả để chạy `hc get` trên Windows), nên
`manage` được phủ gián tiếp qua `effective_cwd`; xem mục "Đề xuất" ở cuối log.
