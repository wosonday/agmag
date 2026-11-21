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

## Bước 2 — code xanh (2026-11-21)

- `src/hc.rs`: thêm `pub fn effective_cwd(&Value) -> Option<String>` — `foreground.cwd` không
  rỗng thắng `cwd` gốc; cả hai thiếu/rỗng → `None`. Không đổi `terminal_row_from`.
- `src/ops.rs::manage`: thay `fresh.cwd` bằng `effective_cwd(v)` khi cập nhật `t.cwd` trước khi
  dựng `launch_spec` → câu hỏi xác nhận in `cwd=` đúng thư mục foreground, spec lưu đúng.
  Quy tắc xác nhận (`caller`, `set_managed_launch`) **không** đổi.
- `src/rotation.rs::start`: `rotations.cwd` = `launch_spec.cwd` (parse được) nếu terminal có
  `launch_spec`, ngược lại `terminals.cwd` như cũ; spec hỏng → rơi về `terminals.cwd`.
  Bước `handoff_ready` vốn ghi đè `spec.cwd = r.cwd` nên giờ nhất quán (r.cwd == spec.cwd).
- `CHANGELOG.md` `[Unreleased] → ### Sửa lỗi`: thêm mục (giữ LF, diff +1/−0).
- Sửa tên test thứ ba thành `managed_handoff_targets_launch_spec_cwd`: tệp bàn giao do agent
  ghi (agm chỉ gửi lệnh), nên test kiểm `rotations.cwd`, vị trí `handoff_path` dưới
  `<spec.cwd>/.agents/handoff/`, không tạo `.agents/` trong thư mục shell, và nội dung lệnh gửi
  chứa đường dẫn tương đối đó.

Kiểm: `cargo test --offline` → tất cả bộ test ok (rotation_test 30 passed, daemon_test 13 passed,
không lỗi); `cargo clippy --offline --all-targets -- -D warnings` sạch; `cargo fmt --check` sạch.

## Đề xuất (KHÔNG sửa trong đầu việc này — theo quyết định của CEO)

Các chỗ vẫn dùng `terminals.cwd` (thư mục lúc tạo shell) và có thể lệch khi người dùng `cd`
rồi mở agent:

- `src/correlate.rs::link_all` — ghép terminal ↔ phiên theo `t.cwd`; với terminal đã `cd`,
  `sessions.cwd` là thư mục mới nên có thể không ghép được. Muốn sửa thì phải lưu thư mục
  hiệu lực vào CSDL lúc nhận sự kiện (thay đổi daemon) — ngoài phạm vi.
- `src/dashboard.rs` — cột `cwd`/`where` hiển thị `terminals.cwd`; hiển thị thư mục shell thay
  vì thư mục agent đang làm.
- `src/rotation.rs::advance_opencode` — dùng `t.cwd` cho OpenCode (không có `launch_spec`);
  đúng như hành vi cũ được giữ, nhưng cùng hạn chế nếu người dùng `cd` trong terminal OpenCode.
- `src/ops.rs::manage` khi `hc get` lỗi: vẫn dùng `t.cwd` đã lưu (có thể là thư mục shell);
  không có nguồn tốt hơn vì không đọc được foreground.

## Bước 3 — kiểm lần cuối (2026-11-21)

- `cargo test --offline`: **318 passed, 0 failed** (31 bộ test; `rotation_test` 30, `daemon_test` 13).
- `cargo clippy --offline --all-targets -- -D warnings`: không cảnh báo/lỗi.
- `cargo fmt --check`: sạch.
- `git status`: sạch sau commit; không có thay đổi ngoài phạm vi tệp của tệp tác vụ.
- Commit: `c57e007` (test đỏ) → `fd97620` (code xanh + CHANGELOG + log).
- Không chạy `agm manage`/`agm rotate` trên terminal thật; không push/merge.



