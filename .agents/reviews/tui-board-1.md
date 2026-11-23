Đối tượng: HEAD a9dcbb5dcb24
Kết luận: CHƯA ĐẠT

## Bắt buộc sửa

- `src/board/mod.rs:331-340` (`run_loop`):
  Khi chuyển agent đang chọn trong panel `Orchestration` (`↑/↓/←/→`, `hjkl`), `state.on_key` trả về `Effect::None`, nên `run_loop` vẽ lại ngay bằng `data.conversation` cũ mà không tải lại `store.messages_involving`. Kết quả: tiêu đề `Messages · <agent mới>` hiển thị **tin nhắn của agent cũ** trong tối đa `refresh_s` giây (mặc định 2s, hoặc lâu hơn nếu đặt `--refresh`).
  Sửa: Lưu `prev_agent = state.sel_agent.clone()` trước `on_key`; nếu `state.sel_agent != prev_agent` trong nhánh `Effect::None` thì truy vấn lại `data.conversation` cho agent mới trước khi `terminal.draw`.

- `src/board/view.rs:171-184` (`draw_narrow`):
  Trong `Narrow` (`< 80` cột hoặc `< 20` dòng), `draw_narrow` vẫn gọi `draw_orchestration` vốn vẽ card cao 4 dòng (`CARD_H = 4`) + 2 dòng connector (`CONNECTOR_ROWS = 2`), trái với `plan.md` ("stacked orchestration list (one line per agent, indent by depth)"). Ở kích thước `70×20`, `rows[0]` chỉ còn ~7 dòng trong khung, khiến toàn bộ agent có `depth >= 1` bị `card_rows` bỏ qua (`rect.height < 2`) và biến mất hoàn toàn khỏi màn hình.
  Sửa: Viết nhánh render dạng danh sách 1 dòng/agent có thụt lề theo `depth` (và cuộn theo `sel_agent`) khi `LayoutMode::Narrow`.

- `src/board/view.rs:215-260, 1032-1080` (`draw_projects`, `draw_inbox`):
  Cả hai panel đều dùng `Paragraph::new(lines)` không cuộn theo mục đang chọn. Do `INBOX_H = 5` chỉ có **3 dòng nội dung** (và `Projects` có 6 dòng), khi có ≥ 4 inbox item (hoặc ≥ 7 project), nhấn `↓` đưa vùng chọn ra ngoài màn hình; người dùng nhấn `r`/`d` trên item không nhìn thấy được.
  Sửa: Tính `scroll_offset` giữ chỉ số đang chọn nằm trong `[offset, offset + inner.height)` và truyền `.scroll((offset, 0))` (tương tự cho `Mode::PickProject` ở `src/board/view.rs:1202-1231`).

## Nên sửa

- `src/board/view.rs:466-480` (`draw_orchestration`):
  Vòng lặp `rows.windows(2)` với điều kiện `next.depth > prev.depth` làm mất toàn bộ đường nối và nhãn edge (`KIND age`) khi một tầng bị wrap sang hàng thứ 2 (ví dụ ≥ 4 worker ở độ rộng 80–119 cột), đồng thời làm đứt nối xuống `depth = 2` nếu cha nằm ở hàng trước hàng wrap.
  Sửa: Nối các hàng theo quan hệ `parent_of` trên toàn bộ `rows` (hoặc vẽ connector giữa mỗi hàng chứa cha và các hàng chứa con của nó), không giả định cha và con luôn nằm ở hai `CardRow` kề nhau.

- `src/board/mod.rs:310-319` (`run_loop`):
  `state.note` không bao giờ được xóa sau khi đặt. Nếu `load_data` gặp lỗi thoáng qua rồi thành công lại ở chu kỳ sau, thông báo lỗi cũ vẫn bám cố định ở footer mãi mãi, che mất thanh phím tắt và gây hiểu nhầm hệ thống còn lỗi.
  Sửa: Xóa `state.note` khi `load_data` thành công (hoặc xóa các note không phải lỗi mới phát sinh sau phím điều hướng tiếp theo ở `Mode::Normal`).
