model: agy/gemini-4-neon-high (--mode plan)

Đối tượng: feat/tui-mouse-overview @ aba0f99
Kết luận: ĐẠT

Toàn bộ các mục **Bắt buộc sửa** (2/2) và **Nên sửa** (kể cả các mục test biên và lặt vặt) từ vòng 1 đều đã được sửa đầy đủ, sạch và có test hồi quy thực chất. Anti-goal giữ vững (không đường chuột nào có thể dispatch/gửi/xác nhận popup), sàn kéo trong `start_drag` đã khớp công thức `apply_height_overrides`, không phát sinh lỗi mới chặn merge, các gate CEO (`fmt`, `clippy -D warnings`, `cargo test` 779 passed / 0 failed) đều xanh.

### Bắt buộc sửa

Không có.

### Nên sửa

- **Xóa `last_click` trước guard `self.drag` trong `on_drag`** ([global.rs:766-772](global.rs.txt#L766-L772)): Hiện `self.last_click = None` nằm sau `let Some(drag) = self.drag.as_mut() else { return ... };`. Nếu người dùng đã có override lưu sẵn rồi thu nhỏ cửa sổ xuống `40x10` (nơi `start_drag` từ chối arm `drag`), thao tác thử kéo liên tiếp `Down → Drag → Up → Down (< 400 ms)` vào viền bị khóa vẫn có thể thành double-click và reset tỉ lệ đã lưu (tương tự `Down → Drag → Up → Down` trên một hàng trong 400 ms). Chuyển `self.last_click = None` lên đầu `on_drag` sẽ khép kín hẳn trường hợp biên này.
- **Biên bằng sàn (`budget == MIN_SECTION_H * groups`) và trần kéo trên của `on_drag`** ([global.rs:734-737](global.rs.txt#L734-L737), [global.rs:780-786](global.rs.txt#L780-L786)): Điều kiện `budget < MIN_SECTION_H * groups` đã khớp đúng `apply_height_overrides`, nhưng khi `budget == MIN_SECTION_H * groups` (0 hàng dư, ví dụ `main.height == 13` hẹp / `10` rộng) mọi nhóm đều chạm sàn nên viền không thể dịch chuyển; ngoài ra `on_drag` vẫn chặn trần theo `area.height - MESSAGES_MIN` thay vì `budget - MIN_SECTION_H * (groups - 1)`, khiến việc kéo quá trần thực tế vẫn ghi tỉ lệ cao hơn mức `apply_height_overrides` có thể áp dụng (điểm tồn từ vòng 1, không gây vỡ layout).
- **Làm chặt thêm tọa độ/phạm vi ở vài test mới**:
  - `border_drag_is_ignored_in_a_window_too_short_for_the_floors` ([global_view_test.rs:1723-1728](global_view_test.rs.txt#L1723-L1728)) dùng `.find_map` nên chỉ kiểm tra viền ngang đầu tiên (`Horizontal(1)`) thay vì duyệt toàn bộ các viền ngang đang vẽ (gồm cả `Horizontal(3)` phía trên `Messages`).
  - `mouse_ignored_while_popup_or_output_overlay` ([global.rs:2074-2083](global.rs.txt#L2074-L2083)) và vòng `Down → Drag → Up` trong `overview_mouse_never_dispatches_or_confirms` ([app.rs:1959-1997](app.rs.txt#L1959-L1997)) giữ nguyên tọa độ `(column, row)` ở cả `Drag` và `Up`; đổi tọa độ ở bước `Drag`/`Up` sẽ mô phỏng thao tác kéo thực tế tốt hơn (dù với viền trên nền layout rỗng, `Drag` tại chỗ vẫn set `drag.changed = true` nên vẫn phủ đường `GEffect::SaveLayout`).
  - `save_layout_note_matches_the_stored_text` ([app.rs:1010-1028](app.rs.txt#L1010-L1028)) khá sát việc soi trực tiếp helper `save_layout`, nhưng đã có các test end-to-end (`layout_reset_with_extra_spaces_closes_the_output_overlay`, `overview_drag_persists_through_handle_mouse`) bảo vệ hành vi thật nên chấp nhận được.

### Lỗi phát sinh mới

Không phát hiện lỗi mới trong [fix.diff](fix.diff):
- **Anti-goal**: `handle_mouse` nhánh `Screen::Global` ([app.rs:716-749](app.rs.txt#L716-L749)) không nhận `dispatch`, bỏ qua mọi `GEffect::Run/SaveTheme/ShowHelp/Quit`; `GlobalState::on_mouse` ([global.rs:553-603](global.rs.txt#L553-L603)) chặn hoàn toàn chuột khi `popup.is_some()` và chỉ cho cuộn `HitPanel::Output` khi `data.output.is_some()`. Nhánh board không bị đụng.
- **Sàn kéo vs `apply_height_overrides`**: `WIDE_MIN_COLS = 120` được dùng chung giữa `section_layout` ([global_view.rs:34-35](global_view.rs.txt#L34-L35), [global_view.rs:239](global_view.rs.txt#L239)) và `start_drag` ([global.rs:719-737](global.rs.txt#L719-L737)); cách tính `groups` và `budget < MIN_SECTION_H * groups` khớp chính xác với [apply_height_overrides](global_view.rs.txt#L388-L396).
- **Trạng thái double-click / drag**: `on_drag` xóa `last_click` khi đang kéo; `popup` và `output` overlay đều xóa cả `last_click` và `drag`; `CHANGELOG.md` giữ đúng CRLF ([fix.diff:5-10](fix.diff#L5-L10)).

### Đối chiếu vòng 1

**Bắt buộc sửa:**
1. **Kéo viền không xóa `last_click` → kéo lại nhanh trong 400 ms bị nhầm thành double-click reset viền** — **Đã sửa**. `on_drag` đặt `self.last_click = None` ([global.rs:772](global.rs.txt#L772)) và có test hồi quy `mouse_drag_is_not_the_first_click_of_a_double_click` ([global.rs:2281-2303](global.rs.txt#L2281-L2303)) phủ đúng chuỗi `Down → Drag → Up → Down (350 ms)`.
2. **`start_drag` chỉ kiểm tra sàn cho 1 nhóm → kéo được viền vô hiệu ở kích thước nhỏ (`40x10`)** — **Đã sửa**. `start_drag` nhận `data`, tính `chosen`/`wide`/`groups` theo `WIDE_MIN_COLS` và kiểm tra `budget < MIN_SECTION_H.saturating_mul(groups as u16)` khớp `apply_height_overrides` ([global.rs:702-737](global.rs.txt#L702-L737)), kèm test end-to-end tại `40x10` `border_drag_is_ignored_in_a_window_too_short_for_the_floors` ([global_view_test.rs:1716-1754](global_view_test.rs.txt#L1716-L1754)).

**Nên sửa:**
1. **`layout reset` không đóng overlay `Output` / khoảng trắng thừa** — **Đã sửa**. Dùng `line.split_whitespace().eq(["layout", "reset"])`, xóa `data.output = None; state.output_scroll = 0;` ([app.rs:414-422](app.rs.txt#L414-L422)), kèm test `layout_reset_with_extra_spaces_closes_the_output_overlay` ([app.rs:940-991](app.rs.txt#L940-L991)).
2. **Test `mouse_double_click_border_resets_only_that_boundary` thiếu cú nhấp thứ hai** — **Đã sửa**. Đã gửi đủ cặp `down(5, 10)` tại `t = 3_000` và `t = 3_100` phủ nhánh `!stored` ([global.rs:2376-2386](global.rs.txt#L2376-L2386)).
3. **Dọn `drag`/`last_click` khi vào trạng thái modal (`popup` và `output`)** — **Đã sửa**. Cả hai nhánh đều xóa `self.last_click = None; self.drag = None;` ([global.rs:554-560](global.rs.txt#L554-L560), [global.rs:573-577](global.rs.txt#L573-L577)), kèm test `mouse_popup_and_output_overlay_drop_a_pending_drag_and_click` ([global.rs:2306-2357](global.rs.txt#L2306-L2357)).
4. **Tăng độ phủ test ở các điểm biên**:
   - Render end-to-end khi thu nhỏ `(140, 40) → (100, 17)` và `(40, 10)` với cả `w` và `h`: **Đã sửa** ([global_view_test.rs:1757-1808](global_view_test.rs.txt#L1757-L1808)).
   - Bố cục hẹp `(100, 40)` với `Horizontal(1..=3)` trong `horizontal_borders_match_the_group_above_them`: **Đã sửa** ([global_view_test.rs:1599-1629](global_view_test.rs.txt#L1599-L1629)).
   - `Down → Drag → Up` lên `BorderId::OutputTop` trong `mouse_ignored_while_popup_or_output_overlay`: **Đã sửa** ([global.rs:2074-2083](global.rs.txt#L2074-L2083)).
   - Kiểm tra `GEffect` của chuỗi double-click và `Down → Drag → Up` trong `overview_mouse_never_dispatches_or_confirms`: **Đã sửa** ([app.rs:1926-1997](app.rs.txt#L1926-L1997)).
5. **Lặt vặt**:
   - Lỗi gõ comment `"above the top pair"` → `"below the top pair"`: **Đã sửa** ([global_view.rs:385](global_view.rs.txt#L385)).
   - Gộp nhánh `GEffect::SaveLayout` thành helper `save_layout`: **Đã sửa** ([app.rs:380-392](app.rs.txt#L380-L392), [app.rs:420](app.rs.txt#L420), [app.rs:456](app.rs.txt#L456), [app.rs:738](app.rs.txt#L738)).
   - `CHANGELOG.md` sửa `` `reply #<id>` `` → `` `reply <id> ` `` (đồng thời sửa cả comment test ở [global.rs:1957](global.rs.txt#L1957)): **Đã sửa** ([fix.diff:9-10](fix.diff#L9-L10)).
