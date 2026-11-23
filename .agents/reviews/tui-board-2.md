Đối tượng: HEAD c9e7e738542e
Kết luận: ĐẠT

## Previous blockers

- `src/board/mod.rs:119-129, 366-380` (`run_loop` / `reload_conversation`): **Đã sửa.** Lưu `prev_agent` trước `state.on_key`; ở nhánh `Effect::None`, nếu `state.sel_agent != prev_agent` thì gọi `reload_conversation` (gọi `data.conversation.clear()` trước khi truy vấn `store.messages_involving`) rồi mới `terminal.draw`. Có test `conversation_follows_selected_agent`.
- `src/board/view.rs:171-180, 320-430` (`draw_narrow` / `draw_narrow_orchestration`): **Đã sửa.** `draw_narrow` chuyển sang `draw_narrow_orchestration`, render dạng danh sách 1 dòng/agent thụt lề theo `depth` và cuộn theo `sel_agent` bằng `scroll_offset`. Có test `narrow_lists_every_agent`, `narrow_scrolls_to_selected`, `narrow_nothing_lost_at_70x24`.
- `src/board/view.rs:183-191, 266-267, 1241-1246, 1427-1436` (`draw_projects`, `draw_inbox`, `PickProject`): **Đã sửa.** Cả 3 chỗ đều tính `scroll_offset` theo mục đang chọn và truyền `.scroll((offset, 0))` cho `Paragraph`. Có test `projects_scroll_to_selected`, `inbox_scrolls_to_selected_item`, `pick_project_scrolls_to_selected`.
- `src/board/view.rs:463-488, 591-629, 785-899` (`card_rows`, `draw_orchestration`, `draw_connectors`): **Đã sửa.** `card_rows` chừa `CONNECTOR_ROWS` trước mọi hàng sau hàng đầu (kể cả hàng wrap); `draw_orchestration` gom card theo `parent` và tra `slot_in_rows` trên toàn bộ `rows` thay vì `rows.windows(2)`. Có test `wrapped_children_keep_edge_labels`.
- `src/board/mod.rs:171-194` (`apply_refresh`) & `src/board/state.rs:114-116` (`on_key`): **Đã sửa.** `apply_refresh` xóa `state.note` khi refresh `Ok` nếu trùng `refresh_note` (giữ nguyên note của action); `on_key` xóa `self.note` khi nhấn phím bất kỳ (trừ `Release`) trong `Mode::Normal`. Có test `refresh_error_note_clears_on_success`, `normal_key_clears_note`.

## Bắt buộc sửa

- Không có. (Các phần `TermGuard` + panic hook, bỏ qua `KeyEventKind::Release` trên Windows, đồng bộ `resume_tx`/`resume_rx` trong `src/tui.rs`, `require_human`, `sanitize_one_line` trên mọi chuỗi untrusted, xử lý khung nhỏ/dữ liệu rỗng, `sync_popup`, và SQL bound parameters đều đạt.)

## Nên sửa

- `src/board/state.rs:87, 542-556` & `src/board/view.rs:155-180`: Trong `LayoutMode::Medium`/`Narrow`, panel `Projects` bị ẩn nhưng `BoardState::new(None, _)` vẫn đặt `panel = Panel::Projects` và `Tab`/`BackTab` vẫn xoay qua `Panel::Projects`. Kịch bản: chạy `agm board` (không có `<project>`) ở pane 90 cột sẽ thấy màn hình toàn `-`, không thấy danh sách project hay phím `p`, còn `↑/↓/⏎` thao tác lên panel vô hình. Sửa: bỏ qua `Panel::Projects` khi ẩn (hoặc hiện `p project` ở footer và tự mở `Mode::PickProject` nếu `project.is_none()` ở `Medium`/`Narrow`).
- `src/board/view.rs:878-898` (`draw_connectors`): `next_x` chỉ nhìn `kids.get(i + 1)` trong nhóm con của cùng một parent. Kịch bản: một hàng có con của parent A đứng cạnh con của parent B → nhãn edge của con A lấy `area.right()` làm giới hạn, vẽ đè lên nét `│` ở `drop_y` của con B. Sửa: tính `next_x` dựa trên `CardSlot` kế tiếp trong toàn `row.cards`.
- `src/board/view.rs:1427-1436` (`draw_popup` cho `Mode::PickProject`): Dùng `.wrap(Wrap { trim: true })` kèm `scroll_offset(*idx, inner.height, data.projects.len())`. Kịch bản: ≥ 19 project với tên dài > 48 cột bị wrap thành nhiều dòng khiến offset đếm theo số item không cuộn hết tới dòng chọn cuối. Sửa: cắt ngắn từng tên bằng `fit_text(..., inner.width as usize)` và bỏ `.wrap` cho `PickProject`.
