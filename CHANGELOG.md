# Changelog

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
