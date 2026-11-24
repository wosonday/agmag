model: agy/gemini-4-hydrogen-high (--mode plan)

Đối tượng: HEAD d13fd5d
Kết luận: CHƯA ĐẠT

### Bắt buộc sửa

- **`src/config.rs:Config::load` & `src/rotation.rs:tick` / `advance_interactive` — Thiếu kiểm tra giá trị ngưỡng trong `[policy]` dẫn đến vòng lặp `pending -> cancelled` mỗi tick:**
  - `Policy` không kiểm tra điều kiện `0.0 < warn <= rotate <= 100.0` (hay giá trị âm / `> 100` / `NaN`) khi parse trong `Config::load`.
  - **Kịch bản lỗi cụ thể:** Người dùng chỉ thêm `interactive_ctx_rotate = 35` vào `providers.toml` (khớp dải 35–45% được nhắc trong `docs/CONTEXT-ROTATION.md`) mà không đặt `interactive_ctx_warn`, nên `#[serde(default)]` giữ `interactive_ctx_warn = 40.0`. Khi một phiên Claude Code/Codex đã `managed` có ngữ cảnh từ 35% đến dưới 40%:
    1. `rotation::tick` thấy `pct >= rotate (35)` nên gọi `start(..., false, now)` tạo một dòng `rotations` mới ở trạng thái `'pending'`.
    2. Ngay trong cùng `tick`, `advance_interactive` thấy `r.ctx_pct.is_some()` và `pct < warn (40)` nên chuyển dòng đó sang `'cancelled'`.
    3. Ở `tick` tiếp theo, `start()` chỉ chặn khi có dòng thuộc `ACTIVE_STATES_SQL`, `'failed'`, hoặc `'needs_attention'` — không gồm `'cancelled'` — nên lại chèn thêm một dòng `'pending'` mới rồi hủy ngay, lặp lại vô hạn ở mỗi chu kỳ daemon cho tới khi `pct >= 40`.
  - Tương tự, giá trị âm (`<= 0.0`) làm `session_context_pct` = `0.0%` cũng kích hoạt cảnh báo/xoay vòng ngay khi mới ghép phiên; `> 100.0` âm thầm vô hiệu hóa ngưỡng; `NaN` làm `pct < rotate` trả về `false` khiến mọi phiên bị coi là đạt ngưỡng xoay vòng. Cần từ chối cấu hình không hợp lệ (`!is_finite()`, `< 0` hoặc `<= 0`, `> 100`, `warn > rotate`) khi nạp config, và/hoặc chặn an toàn ở bước hủy tự động (chỉ hủy khi `pct < warn.min(rotate)`).

### Nên sửa

- **`src/rotation.rs:advance_interactive` — Đọc `t.app.as_deref()` thay vì `app` đã phân giải ở `tick`:**
  - `tick` phân giải `app` theo thứ tự `r.app.clone().or_else(|| ... t.app)` rồi truyền `app: &str` vào `advance_interactive`, nhưng dòng 532 trong `advance_interactive` lại gọi `cfg.policy.ctx_thresholds(t.app.as_deref()).0`.
  - **Kịch bản lỗi cụ thể:** Nếu một vòng xoay của `claude-code`/`codex` được tạo với `r.app = Some("codex")` ở 52%, nhưng bản ghi `terminals.app` (`t.app`) bị cập nhật thành `None` hoặc giá trị khác không phải `"claude-code" | "codex"`, `tick` vẫn điều phối vào `advance_interactive`, còn dòng 532 lại rơi về `ctx_warn` chung (`50.0`) thay vì `interactive_ctx_warn` (`40.0`). Nếu ngữ cảnh giảm nhẹ xuống 45–49%, vòng xoay sẽ bị tự hủy sai dù vẫn trên 40%. Nên dùng `cfg.policy.ctx_thresholds(Some(app)).0`.

- **`tests/config_test.rs:interactive_ctx_thresholds_default_and_parse` — Chưa kiểm thử tệp `providers.toml` cũ thiếu khóa mới:**
  - Test mới chỉ kiểm tra `Config::builtin()` và chuỗi TOML có đủ cả 4 khóa (`ctx_warn`, `ctx_rotate`, `interactive_ctx_warn`, `interactive_ctx_rotate`). Chưa có test parse một bảng `[policy]` cũ chỉ chứa `ctx_warn`/`ctx_rotate` để khẳng định `#[serde(default)]` giữ đúng mặc định `(40.0, 50.0)` không lỗi.

- **`tests/rotation_test.rs` & `tests/policy_test.rs` — Thiếu assertion kiểm chứng kịch bản phiên chưa quản lý (`unmanaged`) và chi tiết thông báo:**
  - Chưa có test kiểm tra phiên Claude Code/Codex chưa bật `agm manage` (`managed = false`) ở mức 45% vs 55% để chứng minh cảnh báo `"unmanaged"` trong `rotation::tick` thực sự dịch từ 70% xuống 50%.
  - `ctx_warn_uses_interactive_threshold_for_claude_and_codex` mới chỉ assert `store.inbox(true)?.len()`, chưa kiểm tra nội dung thông báo ghi `40%` hay biên dưới (vd 35% chưa cảnh báo).
  - `interactive_pending_cancels_only_below_interactive_warn` mới test `codex` với `Config::builtin()`, chưa phủ `claude-code` hay ngưỡng tùy biến từ `Config`.

### Ghi chú

- **Hành vi mặc định 40/50 không có vòng lặp `pending -> cancelled` hay trùng hộp thư:** Với cặp mặc định `(40, 50)`, nhánh `start()` chỉ chạy khi `pct >= 50` và `advance_interactive` chỉ hủy khi `pct < 40`, nên trong khoảng 40–50% trạng thái `pending` hoặc `cancelled` đều ổn định. `rotation::tick` dùng `open_inbox_keyed(..., "unmanaged", ...)` và `policy::context_tick` tạo chuỗi cố định theo `{warn:.0}%` (không nội suy `pct`). Vòng bị `failed`/`needs_attention` tiếp tục chặn `start(..., manual = false)` là hành vi có sẵn của `rotation.rs` để tránh nhắc bàn giao liên tục.
- **Tài liệu nhất quán với mã nguồn:** `README.md`, `README_vi.md`, `HERDR-CORE-BOT.md`, `AGENTS-TUTORIAL.md`, `docs/CONTEXT-ROTATION.md` và `CHANGELOG.md` mô tả đúng tên khóa `[policy].interactive_ctx_warn` (40) / `interactive_ctx_rotate` (50), phạm vi áp dụng (`claude-code`/`codex` so với OpenCode/app khác) và hành vi daemon chỉ hỏi bàn giao khi phiên đã đạt ngưỡng và ở trạng thái `idle`/`done`. (Lưu ý nhỏ: `policy.rs` vẫn còn comment cũ ở đầu tệp và hàm `tick` chỉ nhắc "xoay vòng thợ OpenCode / thợ").
- **Các phần nằm ngoài tệp được cung cấp (không thể kiểm chứng từ context hiện tại):**
  - Toàn bộ các tệp nguồn khác ngoài `config.rs.txt`, `policy.rs.txt`, `rotation.rs.txt` và `change.diff` (ví dụ `src/cli.rs`, `src/tui/`, `src/daemon.rs`, `src/report.rs`) để khẳng định chắc chắn không còn chỗ nào đọc trực tiếp `ctx_warn`/`ctx_rotate` thay vì `ctx_thresholds(...)`.
  - `assets/providers.default.toml` (không có trong `change.diff` và không nằm trong tệp đính kèm): `Config::builtin()` vẫn trả về `40.0/50.0` nhờ `#[serde(default)]`, nhưng chưa rõ tệp cấu hình mẫu gốc đã liệt kê hai khóa mới hay chưa.
  - `src/store.rs` và `src/hc.rs` / `src/correlate.rs`: chưa thể kiểm chứng trực tiếp cơ chế khử trùng lặp của `open_inbox` / `open_inbox_keyed` cũng như mọi đường điền giá trị `terminals.app` (nếu `t.app` là `None`, cả `context_tick` và `rotation::tick` sẽ rơi về ngưỡng chung 50/70).
