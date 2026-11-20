# Changelog

## [Unreleased]

### Thay đổi

- Bảng và kết quả CLI (`agm ls`, `quota`, `cost`, `ps`, `log`, `inbox`, `budget ls`, `cleanup`, `scan`) chuyển sang tiếng Anh; JSON không đổi.
- `agm cost`: hàng `TOTAL` thành **SUM** in đậm, cộng mọi cột kể cả USD; cột số căn phải.
- `agm tui`: lệnh gõ đầy đủ `reply`, `done`, `send`, `cleanup`, `interrupt`, `kill`, `policy`, `view`, `quit` (chữ cái đầu vẫn dùng được). `send` nhận loại tin tiếng Anh `ASSIGN|REPORT|ERROR|QUESTION|BLOCKED|PASS|FAIL` (mã tiếng Việt cũ vẫn nhận; CSDL và `agm send --type` giữ mã tiếng Việt). Nhãn loại tin trong RECENT MESSAGES hiện bằng tiếng Anh.
- Bảng TERMINALS của TUI: `name` (chấm `●` tô màu theo trạng thái), `agent`, `model`, `effort`, `where` (vị trí herdr `workspace / tab`), `task`, `tokens`, `ctx`, `last msg` (`→`/`←` + loại tin, có màu), `status`.
- TUI in đầy đủ (lướt chuột để xem phần trên), xoá màn hình và lịch sử cuộn trước mỗi lần vẽ nên không còn khung lặp; chỉ vẽ lại khi dữ liệu đổi hoặc sau khi gõ lệnh; tự nhận bề rộng pane (`COLUMNS` để ghi đè).
- Model OpenCode được lưu dạng `providerID/id` (lần quét đầu sau nâng cấp nạp lại `opencode.db` một lần, không đếm trùng). Billing tra theo model đã khai, nếu không thì theo tiền tố provider; thêm `[providers."opencode-go"] billing = "subscription"` vào cấu hình mặc định.
- Ghép terminal OpenCode với phiên mới nhất cùng thư mục, kể cả phiên bắt đầu muộn hơn 10 phút hoặc qua `/new`; liên kết thủ công không bị đổi.

- TUI chuyển sang giao diện tiếng Anh có màu ANSI; bảng co giãn theo `COLUMNS` (60–200, mặc định 110), đo đúng độ rộng Unicode (CJK/emoji) và lọc mọi chuỗi điều khiển CSI/OSC trong dữ liệu hiển thị.
- `agm cleanup` an toàn hơn: kiểm lại trạng thái mới nhất, app, thời điểm tạo và thư mục của cả phiên cũ lẫn phiên kế nhiệm trước khi dọn; luôn cần một lần gõ xác nhận, kể cả khi có `--yes`.
- Bàn giao phiên Claude/Codex chỉ hoàn tất khi đọc được trạng thái sống mới nhất; lỗi đọc không còn dựa vào trạng thái `done` đã lưu.
- Hạn mức gói hiển thị theo **phần còn lại** (giảm dần từ 100%, như Codex `/status` và agy `/usage`) trong `agm quota`, TUI (`PLAN QUOTA (left)`) và statusline Claude (`5h 76% left`). JSON của `agm quota` giữ `used_percent` và thêm `left_percent`. Ngưỡng `agm budget set provider:…` vẫn tính theo % **đã dùng**.

### Thêm mới

- **USD theo giá niêm yết** cho mọi model trong `agm cost`, `agm cost --summary` và dòng USAGE của TUI: bảng giá dựng sẵn `assets/prices.toml` (ghi đè bằng `[models."<id>".price]`), tự khớp `provider/id`, hậu tố ngày và hậu tố effort của agy; id `-free` tính $0; model chưa biết giá hiện `?`, tổng có phần chưa biết giá hiện `≥ $x`. JSON có `value_usd` và `unpriced_tokens`; `cost_usd` vẫn là tiền thực trả của model tính theo API (`billed (API)` trong `--summary`).
- `agm tui`: lệnh `view` chọn tối đa 3 bảng (`quota`, `usage`, `terminals`, `budgets`, `inbox`; mặc định `quota terminals inbox`, được nhớ); RECENT MESSAGES luôn hiện ngay trên thanh lệnh.
- Theo dõi effort của phiên (Codex `effort`, Claude Code `effort`, `variant` của OpenCode, hậu tố tên model agy); schema CSDL v4 thêm cột `sessions.effort`.
- Cửa sổ ngữ cảnh của model OpenCode: `providers.toml` (`[models."<providerID/id>"]` rồi `[models."<id>"]`) luôn thắng; nếu thiếu, `agm` đọc `limit.context` từ danh mục của OpenCode (`opencode api GET /api/model`, cache trong tiến trình). Nhờ đó `ctx %`, `ctx_warn` và `ctx_rotate` hoạt động với `opencode/*`, `opencode-go/*`; model không có ở đâu vẫn để trống, không đoán.

- Hạn mức gói của agy: ba provider `agy-gemini`, `agy-3p` (Claude/GPT qua agy, tách khỏi hạn mức Claude Code) và `agy-argon`, đọc qua `agy --print /usage` (không tốn token). Daemon đọc trên luồng riêng mỗi `[policy].agy_quota_poll_min` phút (mặc định 15, `0` = tắt); `agm quota --refresh` đọc ngay. Dùng được cho ngân sách, vd `agm budget set provider:agy-gemini:10080 --soft 80 --hard 95`.
- Hạn mức gói Codex từ nguồn tươi (0 token): `agm quota --refresh` và daemon (`[policy].codex_quota_poll_min`, mặc định 15 phút) đọc `codex app-server` (`account/rateLimits/read`), ghi provider `codex` (cửa sổ 300/10080 phút); lỗi/không đăng nhập/timeout không ghi dữ liệu giả. Thêm cấu hình `[policy].quota_stale_min` (mặc định 60 phút) và `[policy].codex_quota_poll_min`.

### Sửa lỗi

- Dữ liệu hạn mức gói cũ được đánh dấu `stale` (`agm quota --json`: `stale`, `observed_age_s`; TUI: `stale · seen … ago` xám) khi quá `[policy].quota_stale_min` hoặc khi có usage mới cùng gói sau thời điểm quan sát; policy/ngân sách `provider:` coi dữ liệu stale như chưa có dữ liệu.
- `agm send` gửi được cho agent đang ở trạng thái `done` (vừa xong lượt, không làm việc). Trước đây `hc send --if idle` luôn từ chối trường hợp này; agent đang `working`/`blocked` vẫn không bị gõ chen.
- Cửa sổ hạn mức đã reset mà chưa có số liệu mới: không còn hiển thị số cũ, và ngân sách `provider:` không còn báo động theo số cũ (vd 96% đã dùng trước khi reset).
- Phiên OpenCode chỉ có message assistant lỗi/0 token không còn bị gắn cảnh báo giả `unknown format?`; Codex/Claude/agy giữ nguyên cảnh báo.
- `agm spawn`: giữ nguyên từng đối số sau `--` (không gộp), chỉ gộp khi `<template>` là template của hc; trên Windows tự dự phòng khi `hc run <template>` lỗi `cannot start "sh"` bằng cách chạy thẳng `command:` của template và gửi thân template + nhiệm vụ vào terminal; xoay vòng kiểm tra mở được phiên kế nhiệm trước khi kill phiên cũ.

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
