Đối tượng: HEAD d8714dfe86a8
Kết luận: CHƯA ĐẠT

## Bắt buộc sửa

1. **Panic tràn số dưới ở thanh lệnh khi màn hình hẹp** — `src/board/global_view.rs:883-907` (`draw_cmdline`)
   - *Kịch bản:* Khi `area.width <= 5` và `area.height >= 2` (ví dụ `5x10` hoặc `4x2`) lúc focus đang ở `Focus::CmdLine`, `available = (area.width as usize).saturating_sub(5) == 0`. Điều kiện `cursor_cells >= available` (`0 >= 0`) đúng nên `scroll = cursor_cells - available + 1 = 1`, làm phép tính `(cursor_cells - scroll)` (`0usize - 1usize`) bị underflow và panic trong debug (hoặc tràn `u16` trong release).
   - *Sửa:* Nếu `available == 0`, chỉ vẽ cắt gọn (hoặc trả về `None` sớm) và dùng `cursor_cells.saturating_sub(scroll)`.

2. **Văn bản chưa khử escape/control đi thẳng ra terminal qua thanh lệnh** — `src/board/global.rs:273-282, 357-373`; `src/board/global_view.rs:890-899`
   - *Kịch bản:* Bấm `s` trên dòng `Terminals` chèn thẳng `t.name` / `t.address` chưa lọc vào `cmd.text`; `on_key_cmdline` (và `on_key_popup`) nhận `KeyCode::Char(c)` kể cả `c.is_control()`; `draw_cmdline` đưa thẳng `visible` vào `Span::styled` mà không gọi `sanitize_one_line`. Tên terminal hoặc chuỗi dán chứa `\x1b]0;...` hay `\n` sẽ đẩy escape sequence thô ra terminal.
   - *Sửa:* Chặn `c.is_control()` khi nhập ký tự, chạy `sanitize_one_line` lên `name` khi prefill `send`, và khử `state.cmd.text` bằng `sanitize_one_line` trước khi tính cột/vẽ ở `draw_cmdline`.

3. **Dữ liệu và con trỏ chọn bị cũ sau khi chạy lệnh từ màn hình Global** — `src/board/app.rs:104-113, 242-253` (`run_app`, `handle_global_key`)
   - *Kịch bản:* Khi đang ở `Inbox`, bấm `d` (`done <id>`) hoặc gõ lệnh thay đổi trạng thái (`done`, `kill`, `policy`, `reply`), `GEffect::Run` trả về `AppAction::Stay` → `Next::Redraw` mà không gọi `global.refresh`. Trước chu kỳ timer kế tiếp, `data.snap` vẫn chứa mục vừa xử lý và `state.sel_inbox` vẫn giữ ID đó; nếu bấm tiếp `⏎` (`show`), `r` (`reply`) hoặc `d`, thao tác chạy lên ID đã đóng (khác với `--plain` và `BoardScreen` vốn refresh ngay sau action).
   - *Sửa:* Với `GEffect::Run` không `Quit`/`OpenBoard`, trả về `Next::Restart` để chạy `global.refresh` + `state.sync` ngay (giữ nguyên `state.note`/`data.output`/`state.popup`).

4. **`PgUp`/`PgDn` không cuộn được panel Output khi focus ở thanh lệnh** — `src/board/global.rs:193-285` (`on_key_cmdline`)
   - *Kịch bản:* Theo mục ràng buộc số 1 trong `spec.md`, lệnh `cd`, `cleanup`, `show` mở panel Output nhưng vẫn giữ focus ở thanh lệnh và cuộn bằng `PgUp`/`PgDn`. Hiện `KeyCode::PageUp`/`PageDown` chỉ xử lý trong `on_key_panel`, còn `on_key_cmdline` bỏ qua nên mở Output từ thanh lệnh xong không cuộn được.
   - *Sửa:* Xử lý `PageUp`/`PageDown` để cập nhật `output_scroll` khi `data.output.is_some()` cả trong `on_key_cmdline` (hoặc ngay trong `on_key` khi `popup.is_none()`).

## Nên sửa

1. **Chọn lại dòng sau khi mục bị xoá nhảy về cuối danh sách** — `src/board/global.rs:117-147` (`GlobalState::sync`)
   - *Kịch bản:* Khi `sel_terminal` hoặc `sel_inbox` biến mất khỏi snapshot (ví dụ vừa `d` mục `#1`), `sync` gán `len.saturating_sub(1)` nên nhảy chọn xuống phần tử cuối thay vì phần tử kề cận.
   - *Sửa:* Lưu chỉ số trước đó (hoặc truyền vào `sync`) và kẹp về `prev_idx.min(len - 1)`.

2. **Panel Output làm mất màu SGR từ `cleanup_block` / `render_project_list` / `render_inbox_item`** — `src/board/global_view.rs:872` (`draw_output`)
   - *Kịch bản:* `cleanup_block` (`src/tui_cmd.rs:426-445`) gắn mã màu ANSI (`\x1b[1;93m...`), nhưng `draw_output` gọi `sanitize_one_line` nên mất hết màu (và không `Wrap`).
   - *Sửa:* Dùng `ansi_to_text(text)` tương tự `draw_bridged`.

3. **Lệch danh sách panel giữa `Tab` và phần vẽ khi `view` chỉ gồm `Messages` hoặc rỗng** — `src/board/global.rs:539-548` vs `src/board/global_view.rs:181-189`
   - *Kịch bản:* `draw_sections` fallback về `Section::DEFAULT`, còn `visible_panels` chỉ trả `[Messages]`, làm `Tab` không đi qua các panel đang hiển thị.
   - *Sửa:* Dùng chung một hàm tính danh sách panel hiển thị.

4. **Chạy thừa `global.refresh` mỗi tick khi đang ở màn hình Board** — `src/board/app.rs:79-83`
   - *Kịch bản:* Lúc ở `Screen::Board`, mỗi chu kỳ vẫn chạy cả `global.refresh` (`compute_roots` + `git_common_root` + `sysmon`) lẫn `board.refresh`.
   - *Sửa:* Chỉ gọi `global.refresh` khi đang ở `Screen::Global` và lúc quay lại từ `Board`.
