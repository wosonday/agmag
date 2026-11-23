Đối tượng: HEAD 389677aa66e5
Kết luận: CHƯA ĐẠT

## Previous blockers

**Bắt buộc sửa (vòng 1):**
1. **Panic tràn số dưới ở thanh lệnh (`draw_cmdline`)** — **Đã sửa** phần panic (thoát sớm khi `available == 0` và dùng `cursor_cells.saturating_sub(scroll)`), nhưng cách tính `scroll` mới gây lỗi hồi quy nặng (xem mục Bắt buộc sửa #1).
2. **Văn bản chưa khử escape/control qua thanh lệnh** — **Đã sửa**: `on_key_cmdline`, `on_key_popup`, `on_key_panel` chặn `c.is_control()`; prefill `s` chạy `sanitize_one_line` (rơi về `addr` nếu rỗng); `draw_cmdline` chạy `sanitize_one_line(&state.cmd.text)` trước khi tính cột và vẽ.
3. **Dữ liệu và con trỏ chọn cũ sau khi chạy lệnh từ Global** — **Đã sửa**: `handle_global_key` trả `AppAction::Refresh` qua `effect_requests_refresh` với mọi `Outcome` trừ `Quit`/`OpenBoard`, chạy `global.refresh` + `state.sync` ngay mà vẫn giữ `note`/`output`/`popup`.
4. **`PgUp`/`PgDn` không cuộn được Output khi focus ở `CmdLine`** — **Đã sửa**: `on_key_cmdline` đã xử lý `PageUp`/`PageDown` cập nhật `output_scroll` khi `data.output.is_some()`.

**Nên sửa (vòng 1):**
1. **Chọn lại dòng sau khi xoá mục nhảy về cuối danh sách** — **Đã sửa**: `GlobalState` lưu `prev_terminal_idx`/`prev_inbox_idx` để `sync` kẹp về phần tử kề cận.
2. **Panel Output mất màu SGR** — **Đã sửa**: `draw_output` dùng `ansi_to_text` kèm `Wrap`.
3. **Lệch danh sách panel giữa `Tab` và phần vẽ khi `view` rỗng/chỉ có `Messages`** — **Đã sửa**: dùng chung `visible_sections`.
4. **Chạy thừa `global.refresh` khi ở `Screen::Board`** — **Đã sửa**: `refresh_targets` bỏ `global` ở `Board` và refresh lại khi về `Global`.

## Bắt buộc sửa

1. **`draw_cmdline` cắt mất ký tự đầu của mọi lệnh ngắn (`1 <= cursor_cells < available`)** — `src/board/global_view.rs` (`draw_cmdline`)
   - *Nguyên nhân:* `fix1.diff` đổi phép tính thành `cursor_cells.saturating_sub(available).saturating_add(1).min(cursor_cells)`. Khi `cursor_cells < available` (ví dụ màn hình 80 cột, `available = 75`, gõ `"ls"` có `cursor_cells = 2`), `2usize.saturating_sub(75) + 1 = 1` và `1.min(2) = 1`, khiến `scroll = 1`.
   - *Hậu quả:* Mọi chuỗi ngắn đều mất ký tự đầu (`"ls"` hiện thành `"agm> s"`, gõ 1 ký tự `"q"` chỉ hiện `"agm> "`) và con trỏ đặt lệch trái 1 cột.
   - *Sửa:* Giữ `if cursor_cells >= available { cursor_cells - available + 1 } else { 0 }` sau guard `available == 0`.

2. **Lệch thứ tự cột `task` giữa tiêu đề/constraint và hàng dữ liệu trong bảng `Terminals`** — `src/board/global_view.rs` (`draw_terminals`)
   - *Nguyên nhân:* `fix1.diff` đẩy `"task"` (`Constraint::Min(6)`) xuống **cuối** `columns` (sau `tokens`, `last`, `place`) để tính `header` và `constraints`, nhưng vòng lặp tạo `cells` vẫn giữ `t.task` ở **trước** `tokens`, `last`, `place`.
   - *Hậu quả:* Trên màn hình đủ rộng hiện `tokens`/`last`/`place`, nội dung `task` bị nhét vào cột `tokens` (6 ký tự), còn `tokens`, `last`, `place` bị đẩy lệch sang phải dưới sai tiêu đề cột.
   - *Sửa:* Chuyển `cells.push(Cell::from(dash_value(&t.task)))` xuống sau `show_place` để khớp `columns`.

3. **Panic ranh giới UTF-8 trong `ansi_to_text` khi `\x1b` đứng trước ký tự non-ASCII** — `src/board/ansi.rs:44, 103` (`ansi_to_text`, `skip_escape`)
   - *Nguyên nhân:* Nhánh `Some(_) => i + 2` của `skip_escape` nhảy cố định 1 byte sau `\x1b`. Khi chuỗi đầu vào (kể cả nội dung đưa vào `draw_output`) chứa `\x1b` ngay trước ký tự nhiều byte (ví dụ `"\x1bá"`), `i` rơi vào giữa ký tự UTF-8 và `&s[start..i]` panic.
   - *Sửa:* Chỉ bỏ qua 1 byte theo sau khi là ASCII (`0x20..=0x7e`), hoặc không cắt qua ranh giới byte UTF-8.

## Nên sửa

Không có.
