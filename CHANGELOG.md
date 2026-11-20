# Changelog

## [Unreleased]

### Thay đổi

- TUI chuyển sang giao diện tiếng Anh có màu ANSI; bảng co giãn theo `COLUMNS` (60–200, mặc định 110), đo đúng độ rộng Unicode (CJK/emoji) và lọc mọi chuỗi điều khiển CSI/OSC trong dữ liệu hiển thị.
- `agm cleanup` an toàn hơn: kiểm lại trạng thái mới nhất, app, thời điểm tạo và thư mục của cả phiên cũ lẫn phiên kế nhiệm trước khi dọn; luôn cần một lần gõ xác nhận, kể cả khi có `--yes`.
- Bàn giao phiên Claude/Codex chỉ hoàn tất khi đọc được trạng thái sống mới nhất; lỗi đọc không còn dựa vào trạng thái `done` đã lưu.
- Hạn mức gói hiển thị theo **phần còn lại** (giảm dần từ 100%, như Codex `/status` và agy `/usage`) trong `agm quota`, TUI (`PLAN QUOTA (left)`) và statusline Claude (`5h 76% left`). JSON của `agm quota` giữ `used_percent` và thêm `left_percent`. Ngưỡng `agm budget set provider:…` vẫn tính theo % **đã dùng**.

### Thêm mới

- Hạn mức gói của agy: ba provider `agy-gemini`, `agy-3p` (Claude/GPT qua agy, tách khỏi hạn mức Claude Code) và `agy-argon`, đọc qua `agy --print /usage` (không tốn token). Daemon đọc trên luồng riêng mỗi `[policy].agy_quota_poll_min` phút (mặc định 15, `0` = tắt); `agm quota --refresh` đọc ngay. Dùng được cho ngân sách, vd `agm budget set provider:agy-gemini:10080 --soft 80 --hard 95`.

### Sửa lỗi

- `agm send` gửi được cho agent đang ở trạng thái `done` (vừa xong lượt, không làm việc). Trước đây `hc send --if idle` luôn từ chối trường hợp này; agent đang `working`/`blocked` vẫn không bị gõ chen.
- Cửa sổ hạn mức đã reset mà chưa có số liệu mới: không còn hiển thị số cũ, và ngân sách `provider:` không còn báo động theo số cũ (vd 96% đã dùng trước khi reset).

## [0.2.0] - 2026-11-19

### Thêm mới

- Schema CSDL v3 (thêm cột idempotent, migration có rollback; chỉ mục theo thời gian cho thống kê).
- `agm manage <t>`: bật quản lý cho phiên Claude Code/Codex mở tay (lưu lệnh khởi chạy đã lọc; chỉ người dùng xác nhận mới bật được).
- Xoay vòng phiên Claude Code/Codex đã quản lý: yêu cầu bàn giao rồi mở phiên kế nhiệm; **phiên cũ không bao giờ bị kill tự động**, chỉ đánh dấu đã bàn giao. Xoay vòng thợ OpenCode giữ nguyên.
- `agm cleanup [t…] [--all]` và lệnh `c` trong TUI: dọn thủ công phiên đã bàn giao (cần người dùng xác nhận); TUI cảnh báo đỏ khi phiên cũ và mới cùng chạy một thư mục.
- Thống kê theo giờ máy: `agm cost --summary` (24h / hôm nay / tháng này + biểu đồ 4 tháng), `agm cost --by month`, `--window 24h|today|month|4months`, `agm scan --since YYYY-MM-DD`; TUI tự backfill tháng còn thiếu.
- Đọc phiên tương tác của Antigravity CLI (`agy`) từ `transcript.jsonl` trong mọi lần quét; ghép terminal `antigravity` theo thư mục; tránh đếm trùng với `agm ask`.

### Sửa lỗi

- Sửa việc bỏ sót token transcript agy và bổ sung backfill `cwd` đến muộn.
- Khi model tính phí API chưa có giá, tổng USD được ghi nhận là chưa xác định thay vì báo sai.
- Sửa mở cơ sở dữ liệu sau migration để không lấy write lock; lọc nhiễu Codex 0.170.

## [0.1.0]

Bản phát hành đầu tiên: đo token/hạn mức Codex, Claude, OpenCode và agy; daemon; hộp thư; gửi tin giữa agent; spawn; ngân sách; xoay vòng thợ OpenCode; TUI.
