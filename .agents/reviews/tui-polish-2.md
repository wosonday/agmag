Đối tượng: HEAD c4fd9ddfbb4e
Kết luận: ĐẠT

*(Đánh giá tĩnh qua [review-1.md](file:///C:/Users/thanh/AppData/Local/Temp/claude/cecfd803e5a33a03/a1543fad-168d-409a-b373-aac310cebd66/scratchpad/review3/review-1.md), [fix.diff](file:///C:/Users/thanh/AppData/Local/Temp/claude/cecfd803e5a33a03/a1543fad-168d-409a-b373-aac310cebd66/scratchpad/review3/fix.diff) và [polish.diff](file:///C:/Users/thanh/AppData/Local/Temp/claude/cecfd803e5a33a03/a1543fad-168d-409a-b373-aac310cebd66/scratchpad/review3/polish.diff); không dùng shell/chạy test.)*

### Bắt buộc sửa
1. **`handle_board_key` (`Tab` từ thanh lệnh)**: **Đã sửa.** Đổi `Panel::Projects` thành `Panel::Orchestration`, kèm test `tab_from_board_command_bar_goes_to_orchestration`.
2. **`run_board_dispatch` (`Outcome::OpenBoard`)**: **Đã sửa.** Thêm `data.output = None; state.output_scroll = 0;`, kèm test `open_board_from_board_bar_clears_output`.

### Nên sửa
1. **`handle_board_key` (`BackTab` từ `Panel::Orchestration`)**: **Đã sửa.** Chặn `KeyCode::BackTab` ở `Mode::Normal` khi ở `Panel::Orchestration` để đặt `*board_cmd_focus = true`, kèm test `backtab_from_orchestration_focuses_board_command_bar`.
2. **`handle_board_key` (`KeyEventKind::Release`)**: **Không sửa.** Chấp nhận được vì chỉ là tối ưu hiệu năng vẽ lại thừa khi nhả phím trên Windows, không làm sai trạng thái (`Next` hiện chưa có biến thể bỏ qua redraw).
3. **`draw_block` (`no CEO`) & `draw_orchestration` (`ascii`)**: **Đã sửa.** Bỏ vẽ `"no CEO"` khi `area.height < 2`, dùng `"v"` khi `ctx.ascii == true`, kèm test `ascii_more_indicator_has_no_unicode_arrow`.
4. **`terminals_gets_remaining_width`**: **Đã sửa.** Tách token theo khoảng trắng và `'│'` trên dòng tiêu đề và kiểm tra `!tokens.contains(&"status")` (chỉ bỏ `"last"` khỏi danh sách kiểm tra).
5. **`single_child_connector_is_straight` & `workers_stay_under_their_ceo`**: **Đã sửa / Không sửa.** `single_child_connector_is_straight` đã assert `'│'` ngay dưới `'┬'`. `workers_stay_under_their_ceo` không sửa nhưng chấp nhận được vì chỉ là độ chặt test; trường hợp nhiều worker đã được `two_ceos_share_top_row` và `ceo_blocks_stack_when_narrow` kiểm tra.

### Lỗi phát sinh mới
Không phát hiện lỗi mới trong [fix.diff](file:///C:/Users/thanh/AppData/Local/Temp/claude/cecfd803e5a33a03/a1543fad-168d-409a-b373-aac310cebd66/scratchpad/review3/fix.diff).
