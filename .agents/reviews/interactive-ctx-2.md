model: agy/gemini-4-hydrogen-high (--mode plan)

Đối tượng: HEAD 154148b
Kết luận: ĐẠT

### Bắt buộc sửa

Không có.

### Nên sửa

- **`tests/config_test.rs:invalid_ctx_thresholds_are_rejected` — Khẳng định lỗi còn lỏng và chưa kiểm chứng ngữ cảnh đầy đủ:**
  - Test dùng `format!("{err:#}").contains("ctx_")`, nên về nguyên tắc một lỗi parse TOML chứa tên khóa `ctx_*` cũng thỏa điều kiện này; nên kiểm tra thêm `"Invalid [policy]"` hoặc thông điệp/tên khóa cụ thể của `Policy::validate` (vd `must be a number in (0, 100]` / `must not exceed`).
- **Độ phủ kiểm thử tích hợp cho biến thể còn lại (không chặn đạt):**
  - `tests/policy_test.rs:interactive_ctx_warn_boundary_and_text` mới kiểm tra `claude-code` ở 35%/40% với `Config::builtin()`, còn `tests/rotation_test.rs:interactive_cancel_uses_rotation_app_and_covers_claude` kiểm tra giữ `pending` ở 45% cho cả `claude-code` và `codex`; chưa có test tích hợp riêng kiểm tra nhánh hủy `< 40%` cho `claude-code` hoặc truyền ngưỡng `interactive_ctx_*` tùy biến từ `Config` vào `policy::tick` / `rotation::tick`.

### Lỗi phát sinh mới

Không có.

- **`Policy::validate()` (`src/config.rs`):** Kiểm tra đủ cả hai cặp (`ctx_warn`/`ctx_rotate` và `interactive_ctx_warn`/`interactive_ctx_rotate`) với `!value.is_finite() || value <= 0.0 || value > 100.0` (chặn `NaN`, `±inf`, số âm, `0`, `> 100`, cho phép `100.0`) và `warn > rotate` (cho phép `warn == rotate`, không tạo vòng lặp `pending -> cancelled` vì `pct >= rotate && pct < warn` luôn sai). Thông điệp lỗi nêu rõ tên khóa, giá trị và ràng buộc.
- **`Config::load()` (`src/config.rs`):** Giữ nguyên hành vi trả `Ok(Self::builtin())` khi tệp chưa tồn tại (`ErrorKind::NotFound`), bọc ngữ cảnh `"Invalid [policy] in ..."` khi `validate()` thất bại; các lệnh cần cấu hình và daemon sẽ từ chối `providers.toml` sai ngay khi nạp trong khi các lệnh nạp cấu hình lười (`agm policy off/kill/inbox/done`) không bị chặn bởi lỗi này.
- **`Some(app)` trong `advance_interactive` (`src/rotation.rs`):** `tick` chỉ gọi `advance_interactive` khi `app.as_deref()` là `Some("claude-code" | "codex")` (ưu tiên `r.app`, fallback `t.app`), và lệnh gọi đệ quy trong nhánh `"requested"` truyền tiếp cùng `app`. Dùng `cfg.policy.ctx_thresholds(Some(app)).0` bảo đảm đồng bộ ngưỡng cảnh báo với toàn bộ phần còn lại của `advance_interactive` (`build_successor_prompt`, gán `row.app`, nhánh `awaiting_link` cho `codex`/`claude-code`) ngay cả khi `terminals.app` bị xóa hoặc đổi sau khi vòng xoay đã tạo.

### Đối chiếu các mục ở vòng 1

- **[Bắt buộc sửa] `src/config.rs:Config::load` & `src/rotation.rs` — Thiếu kiểm tra ngưỡng `[policy]` gây vòng lặp `pending -> cancelled`:** **Đã sửa.** Đã thêm `Policy::validate()` gọi trong `Config::load()` và test `invalid_ctx_thresholds_are_rejected` trong `tests/config_test.rs`.
- **[Nên sửa] `src/rotation.rs:advance_interactive` — Đọc `t.app.as_deref()` thay vì `app` đã phân giải:** **Đã sửa.** Đã đổi sang `cfg.policy.ctx_thresholds(Some(app)).0` và thêm test `interactive_cancel_uses_rotation_app_and_covers_claude` trong `tests/rotation_test.rs`.
- **[Nên sửa] `tests/config_test.rs` — Chưa test `providers.toml` cũ thiếu khóa `interactive_ctx_*`:** **Đã sửa.** Đã thêm `old_policy_without_interactive_keys_keeps_defaults` trong `tests/config_test.rs`.
- **[Nên sửa] `tests/rotation_test.rs` & `tests/policy_test.rs` — Thiếu test phiên `unmanaged`, biên/nội dung cảnh báo 40%, phủ `claude-code` và ngưỡng tùy biến:** **Đã sửa phần lớn (phần còn lại chấp nhận được).**
  - Phiên `unmanaged` (45% vs 55% cho cả `claude-code` và `codex`): **Đã sửa** bằng `unmanaged_claude_and_codex_warn_at_interactive_rotate`.
  - Test cũ `ctx_warn_uses_interactive_threshold_for_claude_and_codex` ở mức 45% vẫn chỉ assert số lượng: **Không sửa trực tiếp — Chấp nhận được**, vì test mới `interactive_ctx_warn_boundary_and_text` đã kiểm tra biên 35%/40% và nội dung chứa `"40%"`.
  - Phủ `claude-code` ở nhánh hủy interactive: **Đã sửa một phần — Chấp nhận được** (`interactive_cancel_uses_rotation_app_and_covers_claude` phủ cả `claude-code` và `codex` giữ `pending` ở 45%; chỉ còn trường hợp `claude-code` tự hủy `< 40%` và ngưỡng tùy biến trong `tick` chưa có test riêng).
- **[Ghi chú] Comment cũ trong `src/policy.rs`:** **Đã sửa** trong `fix.diff` (cập nhật doc comment đầu tệp và hàm `policy::tick`).
- **[Ghi chú] Các điểm đọc `ctx_warn`/`ctx_rotate` khác trong `src/` & `assets/providers.default.toml`:** **Không sửa — Chấp nhận được** (theo kết quả grep toàn `src/` và xác nhận `assets/providers.default.toml` không có khóa `ctx` nào).
