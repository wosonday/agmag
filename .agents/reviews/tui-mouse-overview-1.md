model: agy/gemini-4-neon-high (--mode plan)

Đối tượng: feat/tui-mouse-overview @ 2204798
Kết luận: CHƯA ĐẠT

Tổng thể nhánh làm sạch, đúng anti-goal (không đường chuột nào có thể dispatch/gửi/xác nhận popup, giữ nguyên bàn phím và footer ≤ 100 cột, một trường `layout: LayoutOverride` duy nhất, `parse`/`Display` chặt, ánh xạ `BorderId::Horizontal(i)` khớp khung vẽ). Tuy nhiên vẫn còn **2 lỗi logic thực tế** trong luồng kéo viền và một lỗi viết thiếu bước trong unit test.

### Bắt buộc sửa

1. **Kéo viền không xóa `last_click` → kéo lại nhanh trong 400 ms bị nhầm thành double-click reset viền** ([global.rs:660-668](global.rs.txt#L660-L668), [global.rs:751-793](global.rs.txt#L751-L793))
   - Khi nhấn `Down(Left)` lên một viền ở `t = 1000`, `on_left_down` ghi `self.last_click = Some((Hit::Border(id), 1000))` rồi bật `self.drag`.
   - Sau đó `on_drag` (`t = 1100`) và `end_drag` (`t = 1200`) hoàn tất thao tác kéo và lưu tỉ lệ mới, nhưng **không xóa `self.last_click`**.
   - Nếu người dùng nhấn lại vào đúng viền đó ở `t = 1350` (trong cửa sổ 400 ms) để tinh chỉnh thêm 1 hàng, `on_left_down` thấy `*prev == target && 1350 - 1000 <= 400` và gọi `on_double_click` → **reset luôn viền vừa kéo**.
   - Cách sửa: xóa `self.last_click = None` trong `on_drag` khi có `self.drag` (ít nhất ngay khi `drag.changed` thành `true`, hoặc ngay khi nhận `Drag(Left)` trong phiên kéo) và bổ sung test cho chuỗi `Down → Drag → Up → Down (< 400 ms)` không được reset viền.

2. **`start_drag` chỉ kiểm tra sàn cho 1 nhóm → ở kích thước nhỏ (kể cả `40x10` trong brief) vẫn kéo được viền vô hiệu và ghi đè nhầm tỉ lệ của section đã bị ẩn** ([global.rs:719-738](global.rs.txt#L719-L738) vs [global_view.rs:390-394](global_view.rs.txt#L390-L394), [global_view.rs:573-582](global_view.rs.txt#L573-L582))
   - `apply_height_overrides` bỏ qua mọi override khi `total.saturating_sub(MESSAGES_MIN) < MIN_SECTION_H * groups` (với mặc định 3 nhóm ở chế độ hẹp là `main.height < 13`; với 2 nhóm ở chế độ rộng là `main.height < 10`).
   - Nhưng `start_drag` chỉ kiểm tra `area.height <= MESSAGES_MIN + MIN_SECTION_H` (`<= 7`, tức cố định 1 nhóm).
   - Hệ quả thực tế ở `40x10` (`main.height = 8`, `DEFAULT = [Quota, Terminals, Inbox]` + `Messages`): `draw_stacked` ẩn hoàn toàn `Inbox`, nên viền `Horizontal(3)` (biên trên của `Messages`, vốn khóa tỉ lệ của `Inbox`) nằm ngay dưới `Terminals`. `start_drag` vẫn nhận kéo `Horizontal(3)`, `on_drag` trả `changed = true` (ép vẽ lại dù khung hình không đổi), và `Up(Left)` lưu `h3=...` xuống DB + báo `"layout saved"`. Khi người dùng phóng to cửa sổ lại, `Inbox` trống bị phồng lên theo tỉ lệ rác vừa lưu.
   - Cách sửa: truyền `data` (hoặc số nhóm từ `visible_sections(&data.view)` + điều kiện `area.width >= 120`) vào `start_drag`, bỏ qua kéo ngang khi `area.height.saturating_sub(MESSAGES_MIN) < MIN_SECTION_H.saturating_mul(groups)` (khớp chính xác điều kiện của `apply_height_overrides`). Thêm test end-to-end với `draw_global` tại `40x10` chứng minh kéo viền không thay đổi/không lưu layout rác.

### Nên sửa

- **`layout reset` không đóng overlay `Output` đang mở** ([app.rs:398-405](app.rs.txt#L398-L405)): Các lệnh thường đi qua `apply_dispatched` đều xóa `data.output = None` (đóng panel `Output` trên lệnh kế tiếp), còn `layout reset` thoát sớm nên nếu người dùng vừa bấm `?` xem trợ giúp rồi gõ `layout reset`, bảng trợ giúp vẫn che màn hình. Nên đặt `data.output = None; state.output_scroll = 0;` và chấp nhận nhiều khoảng trắng (`line.split_whitespace().eq(["layout", "reset"])`).
- **Test `mouse_double_click_border_resets_only_that_boundary` thiếu cú nhấp thứ hai** ([global.rs:2260-2263](global.rs.txt#L2260-L2263)): Comment ghi kiểm tra double-click vào viền không có override, nhưng sau khi cặp trước đã tiêu thụ `last_click` ở `t = 1_200`, đoạn cuối chỉ gọi **một** `down(5, 10)` ở `t = 3_000` (vốn luôn trả `(false, None)` cho mọi viền). Cần gửi thêm `down(5, 10)` ở `t = 3_100` để thực sự phủ nhánh `!stored` của `on_double_click`.
- **Dọn trạng thái `drag`/`last_click` khi vào trạng thái modal** ([global.rs:554-558](global.rs.txt#L554-L558), [global.rs:571-586](global.rs.txt#L571-L586)): Khi `popup.is_some()` nên xóa thêm `self.drag = None`; khi `data.output.is_some()` nên xóa cả `self.last_click = None; self.drag = None;` để tránh drag/click cũ lọt lại sau khi đóng overlay/popup.
- **Tăng độ phủ test ở vài điểm biên**:
  - Thêm test render end-to-end (không chỉ gọi riêng `apply_height_overrides`) cho thu nhỏ cửa sổ `(140, 40) → (100, 17)` và `(40, 10)` khi có sẵn `LayoutOverride` (cả `w` lẫn `h`).
  - Trong `horizontal_borders_match_the_group_above_them` ([global_view_test.rs:1565-1596](global_view_test.rs.txt#L1565-L1596)), kiểm tra thêm bố cục hẹp `(100, 40)` với `Horizontal(1..=3)`.
  - Trong `mouse_ignored_while_popup_or_output_overlay` ([global.rs:2029-2046](global.rs.txt#L2029-L2046)), kiểm tra thêm `Down → Drag → Up` lên `BorderId::OutputTop` trả `(false, None)`.
  - Trong `overview_mouse_never_dispatches_or_confirms` ([app.rs:1851-1874](app.rs.txt#L1851-L1874)), kiểm tra thêm `GEffect` của chuỗi double-click và `Down → Drag → Up` (không chỉ check `Next`).
- **Lặt vặt**:
  - Sửa lỗi gõ comment ở [global_view.rs:383](global_view.rs.txt#L383): `"Horizontal(2) is above the top pair"` → `"below the top pair"` (code ở dòng 403 đúng).
  - Gộp nhánh xử lý `GEffect::SaveLayout` đang lặp giữa `handle_global_key` ([app.rs:438-451](app.rs.txt#L438-L451)) và `handle_mouse` ([app.rs:730-743](app.rs.txt#L730-L743)) thành helper dùng chung.
  - `CHANGELOG.md` dòng 8 ghi `` `reply #<id>` `` trong khi lệnh điền sẵn thực tế là `reply <id> ` (không có `#`, khớp phím `r` và `README.md`).

### Đối chiếu brief

- **Bước 1 — Chuột màn tổng (pure)** — **Đạt**
  - `GlobalState::on_mouse`, `on_wheel`, `on_left_down`, `click_row`, `on_double_click` ([global.rs:553-693](global.rs.txt#L553-L693), [global.rs:803-860](global.rs.txt#L803-L860)); ghi `HitPanel::CmdLine` ([global_view.rs:96](global_view.rs.txt#L96)).
  - Chặn mọi chuột khi `popup.is_some()`, chỉ cuộn `HitPanel::Output` khi `data.output.is_some()`, trả `(false, None)` khi không đổi, lọc đúng `Down/Drag/Up(Left)` + `ScrollUp/Down`, double-click 400 ms cùng đích tiêu thụ cặp.
- **Bước 2 — Layout overrides (pure)** — **Đạt**
  - `LayoutOverride` + `Display`/`parse` chặt ([global.rs:200-301](global.rs.txt#L200-L301)); một trường `pub layout: LayoutOverride` duy nhất trên `GlobalState` ([global.rs:426](global.rs.txt#L426)).
  - `pair_column_widths` clamp ≥ 20 cột ([global_view.rs:202-226](global_view.rs.txt#L202-L226)); `apply_height_overrides` áp dụng sau `fit_heights`, ánh xạ đúng `wide => boundary - 2`, `narrow => boundary - 1`, giữ sàn `MIN_SECTION_H = 3`, `MESSAGES_MIN = 4`, bỏ qua index cũ và cửa sổ quá thấp ([global_view.rs:262-263](global_view.rs.txt#L262-L263), [global_view.rs:386-459](global_view.rs.txt#L386-L459)); test vị trí biên vẽ thực tế ([global_view_test.rs:1565-1596](global_view_test.rs.txt#L1565-L1596)).
- **Bước 3 — Drag-resize & lưu `tui_layout`** — **Chưa**
  - Lưu/tải `tui_layout` khi khởi động và `layout reset` đã nối ([app.rs:119](app.rs.txt#L119), [app.rs:398-405](app.rs.txt#L398-L405), [app.rs:500-505](app.rs.txt#L500-L505)); `OutputTop` không kéo được ([global.rs:699-701](global.rs.txt#L699-L701)); test kéo viền đổi rect khung sau đạt ([global_view_test.rs:1599-1679](global_view_test.rs.txt#L1599-L1679)).
  - Chưa đạt do 2 lỗi trong mục **Bắt buộc sửa** tại [global.rs:719-785](global.rs.txt#L719-L785) (kéo viền không xóa `last_click`; `start_drag` không kiểm tra sàn theo số nhóm thực tế tại `40x10`).
- **Bước 4 — Nối vòng lặp & test anti-goal** — **Đạt**
  - `handle_mouse` nhánh `Screen::Global` ánh xạ `GEffect` đúng và bỏ qua mọi hiệu ứng gửi/thoát ([app.rs:709-753](app.rs.txt#L709-L753)); không đụng nhánh board; các test mức `app.rs` phủ redraw/skip, anti-goal không dispatch/không xác nhận popup, Output modal, lưu drag, double-click mở board ([app.rs:1731-2098](app.rs.txt#L1731-L2098)).
- **Bước 5 — Tài liệu & trợ giúp `?`** — **Đạt**
  - `?` chuyển sang `GEffect::ShowHelp` ([global.rs:1118](global.rs.txt#L1118)) mở `OVERVIEW_HELP` trong Output panel có đủ click/wheel/double-click/drag/`layout reset`/`Shift+drag`/`AGM_MOUSE=0` ([app.rs:97-101](app.rs.txt#L97-L101), [app.rs:452-457](app.rs.txt#L452-L457)), giữ nguyên `HELP`/`HELP_ASCII` ([global_view.rs:37-39](global_view.rs.txt#L37-L39)).
  - Cập nhật đúng 1 câu ngắn ở `README.md` ([change.diff:21](change.diff#L21)), `README_vi.md` ([change.diff:34](change.diff#L34)) và 1 mục tiếng Việt ở `CHANGELOG.md` (giữ CRLF, [change.diff:9](change.diff#L9)).
