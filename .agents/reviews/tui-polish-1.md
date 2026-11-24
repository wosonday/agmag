Đối tượng: HEAD 429c38c8f03e
Kết luận: CHƯA ĐẠT

*(Phạm vi kiểm tra: [request.md](file:///C:/Users/thanh/AppData/Local/Temp/claude/cecfd803e5a33a03/a1543fad-168d-409a-b373-aac310cebd66/scratchpad/review3/request.md) và [polish.diff](file:///C:/Users/thanh/AppData/Local/Temp/claude/cecfd803e5a33a03/a1543fad-168d-409a-b373-aac310cebd66/scratchpad/review3/polish.diff); `src/board/layout.rs` và `src/project.rs` không nằm trong diff nên không thể kiểm chứng trực tiếp.)*

### Bắt buộc sửa

1. **src/board/app.rs:~631 (`handle_board_key`)**
   - **Tình huống**: Khi đang focus ở thanh lệnh trên màn hình board (`:` -> `*board_cmd_focus = true`) và người dùng nhấn `Tab`, code gán `board.state.panel = Panel::Projects`. Theo `request.md` mục 3, panel `Projects` đã bị gỡ khỏi board và `Tab` phải bỏ qua nó; lỗi này đưa focus vào panel không tồn tại (không có khung nào sáng, phím điều hướng tác động ngầm vào `sel_project`).
   - **Cách sửa**: Đổi `Panel::Projects` thành `Panel::Orchestration`.

2. **src/board/app.rs:~569 (`run_board_dispatch`)**
   - **Tình huống**: Trên board, nếu người dùng chạy một lệnh trả về `Outcome::Shown` (đặt `data.output = Some(...)`), sau đó chạy tiếp lệnh mở board khác (`Outcome::OpenBoard`), nhánh `else if let Some(key) = open_board` tạo `BoardScreen` mới và tắt `*board_cmd_focus` nhưng quên xóa `data.output` và `state.output_scroll` (khác với nhánh `Next::Board` ở ~211). Kết quả là overlay output cũ vẫn phủ lên board mới.
   - **Cách sửa**: Thêm `data.output = None; state.output_scroll = 0;` trong nhánh `if let Some(key) = open_board`.

### Nên sửa

1. **src/board/app.rs:~657 (`handle_board_key`)**
   - **Tình huống**: Nhấn `Tab` từ `Panel::Inbox` chuyển focus vào thanh lệnh (`*board_cmd_focus = true`), và `BackTab` từ thanh lệnh quay lại `Panel::Inbox`; nhưng nhấn `BackTab` từ `Panel::Orchestration` lại rơi vào `board.on_key` và nhảy thẳng về `Panel::Inbox`, bỏ qua thanh lệnh khi duyệt ngược.
   - **Cách sửa**: Khi `board.state.mode == Mode::Normal && board.state.panel == Panel::Orchestration`, chặn `KeyCode::BackTab` để bật `*board_cmd_focus = true`.

2. **src/board/app.rs:~592 (`handle_board_key`)**
   - **Tình huống**: Nhánh `if key.kind == KeyEventKind::Release { return Next::Redraw; }` khiến vòng lặp vẽ lại toàn bộ TUI ở mỗi sự kiện nhả phím trên Windows 11 dù không có thay đổi trạng thái.
   - **Cách sửa**: Bỏ qua `KeyEventKind::Release` trước khi tạo `Next::Redraw` (trả về action không redraw ở vòng lặp sự kiện).

3. **src/board/view.rs:~776 (`draw_block`) & ~742 (`draw_orchestration`)**
   - **Tình huống**: Khi block `NoCeo` xếp chồng ở cuối chỉ còn đúng `area.height == 1`, `draw_block` vẫn in nhãn `"no CEO"` dù `cards_area.height == 0` không chứa được thẻ nào. Ngoài ra, chỉ báo ` ↓ {hidden} more ` vẫn dùng ký tự Unicode `'↓'` khi `ctx.ascii == true`.
   - **Cách sửa**: Chỉ vẽ `"no CEO"` khi `area.height > 1` (hoặc có thẻ vừa khung); dùng `"v"` khi `ctx.ascii == true`.

4. **tests/global_view_test.rs:~583 (`terminals_gets_remaining_width`)**
   - **Tình huống**: Test gọi `has(&buf, hdr)` trên toàn buffer cho `"st"` và `"tok"`. Vì tiêu đề `"last"` đã chứa chuỗi con `"st"` (và `"status"` cũ cũng chứa `"st"`), `has(&buf, "st")` vẫn pass ngay cả khi cột `"st"` bị ẩn hoặc chưa rút gọn tiêu đề.
   - **Cách sửa**: Kiểm tra từng token trên dòng tiêu đề của bảng Terminals (`split_whitespace()`).

5. **tests/board_view_test.rs:~995 (`workers_stay_under_their_ceo`) & ~1190 (`single_child_connector_is_straight`)**
   - **Tình huống**: `workers_stay_under_their_ceo` chỉ có 1 worker mỗi CEO và kiểm tra `x_w1 < x_c2` cùng `x_w2 >= x_c2 - 4`, vốn vẫn pass kể cả khi không tách CEO block mà trải worker trên một hàng chung. `single_child_connector_is_straight` chỉ kiểm tra `!all.contains('┼')`, chưa chứng minh `'│'` được vẽ.
   - **Cách sửa**: Dùng ít nhất 2 worker cho một CEO hoặc kiểm tra tọa độ worker nằm trong biên block của CEO; assert thêm `'│'` trên dòng bus nối.
