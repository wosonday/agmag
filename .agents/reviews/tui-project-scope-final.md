- Đối tượng: HEAD `8ea1982`, phạm vi git diff main..HEAD (toàn bộ tính năng TUI cd dự án)

## 1. Luồng đầu–cuối (`cd` → `load` → `render` → `prompt`)

- **Quản lý trạng thái trong vòng lặp TUI ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L263-L319)):**
  - Giữ đúng 3 trạng thái phiên trong bộ nhớ: `scope: Option<ProjectKey>`, `roots_cache: RootCache`, `shown: Option<String>`.
  - Mỗi chu kỳ vẽ vẫn dựng `Snapshot` toàn cục để cập nhật header (`cpu/ram/disk`, số terminal, inbox, policy) và lấy các trường phiên (`model`, `effort`, `tokens`, `context_pct`, `last_msg`) cho các agent đang `live`.
  - Khi `scope` là `Some(key)`, `scoped_text` ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L192-L209)) nạp `store.terminals(false)`, phân giải `compute_roots` qua `roots_cache` + `git_common_root`, gọi `load_project_view`, vẽ thân qua `render_project_body` và bọc header/footer bằng `render_scoped` ([src/dashboard.rs](file:///D:/Project/agmag.wt/tui-review/src/dashboard.rs#L1166-L1177)).
  - Khối `shown` (từ `cd` không đối số hoặc `show <#>`) được nối ngay sau footer và nằm phía trên bảng `cleanup_list`, dòng `note` và dấu nhắc `agm[...]>` ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L291-L301)). Khi timeout làm mới tự động (`RecvTimeoutError::Timeout` tại [src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L312)), `shown` và `note` được giữ nguyên; khi người dùng nhập lệnh tiếp theo ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L317-L318)), cả hai được xóa đúng đặc tả.
  - Dấu nhắc `render_frame` ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L168-L176)) hiển thị `agm[<project>]> ` khi đang ở trong dự án và `agm> ` ở chế độ toàn cục; các lệnh `reply`, `done`, `send`, `cleanup`, `interrupt`, `kill` giữ nguyên `#ID` và địa chỉ toàn cục.

## 2. Đối chiếu Review Focus (1–5) trong kế hoạch

1. **Chuẩn hóa đường dẫn Windows & gộp worktree về repo chính (`roots_are_normalized_and_worktrees_join_main`):** **Đạt.** `RootCache::root_of` ([src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L90-L100)) chuẩn hóa cả đầu vào lẫn kết quả từ `resolve` qua `correlate::norm_cwd` (chữ thường trên Windows, đổi `/` thành `\`, bỏ `\\?\` và `\` cuối), đồng thời cache cả trường hợp `resolve` trả `None`.
2. **Khớp tin nhắn và hộp thư theo cả địa chỉ lẫn tên terminal (`membership_matches_address_or_name`, `messages_involving_matches_any_listed_party`):** **Đạt** ở tầng hàm thuần và truy vấn store (`parties` thu thập cả `address` và `name` khác rỗng; `list_projects` ánh xạ cả `by_address` và `by_name` theo thứ tự ưu tiên `live` rồi `created_ms` mới nhất). *(Lưu ý lỗi lọc `kept` trong `load_project_view` đã được ghi nhận sửa riêng; xem thêm một điểm biên liên quan ở mục Nên sửa #2).*
3. **Chu trình `parent` và `parent` ngoài dự án (`tree_survives_cycles_and_foreign_parents`):** **Đạt.** `tree_order` ([src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L283-L362)) chỉ tra `parent` trong phạm vi `members`, bỏ tự tham chiếu (`p != i`), duyệt từ các gốc thực theo `created_ms` với mảng `visited`, sau đó phá các chu trình còn sót tại nút cũ nhất; `agent_lines` ([src/project_render.rs](file:///D:/Project/agmag.wt/tui-review/src/project_render.rs#L258-L274)) đã `.skip(1)` ở tầng 0 nên không còn vẽ cột `│` thừa ở gốc.
4. **Chuỗi escape độc hại trong dữ liệu (`untrusted_text_is_inert`):** **Đạt.** Mọi chuỗi hiển thị đều qua `sanitize_one_line` / `one_line` trước khi bọc màu và được chặn lớp cuối bởi `bounded_styled_line`.
5. **Không tràn khung ở 40 / 60 / 110 cột với CJK và emoji (`no_line_exceeds_width_at_40_60_110`):** **Đạt.** Đo độ rộng theo `UnicodeWidthStr::width` trên từng grapheme, tự ẩn sparkline khi `width < 70` (`NARROW_NO_SPARK`) và ẩn thêm thanh `ctx` khi `width < 50` (`NARROW_NO_CTX`), giới hạn mọi dòng ở `width.saturating_sub(1)`.

## 3. Tuân thủ đặc tả (Spec Compliance)

- **Giá trị chưa biết hiển thị `-`:** Đúng trong toàn bộ renderer (`ctx -` khi `context_pct` là `None` hoặc `NaN`/`Infinite`, `-` cho sparkline khi không có phiên ghép, `-` cho tổng token, `doing: -` khi chưa có lượt `assistant`/`tool`, `? -` khi thiếu status, `-` khi thiếu cả `app` lẫn `model`).
- **20 tin nhắn mới nhất & giới hạn dòng wrap 4 / 3:** `load_project_view` truy vấn `store.messages_involving(&ps, 20)` sắp theo `id DESC`; `inbox_lines` giới hạn 4 dòng, `message_lines` giới hạn 3 dòng kèm `…` ở dòng cuối khi bị cắt; `render_inbox_item` cho lệnh `show <#>` mở toàn bộ (`usize::MAX`).
- **Sparkline 12×5 phút không tính `cache_read`:** `usage_buckets` ([src/store/project.rs](file:///D:/Project/agmag.wt/tui-review/src/store/project.rs#L86-L129)) chia 12 khoảng `300_000 ms` trong cửa sổ `[now_ms - 3_600_000, now_ms)`, tính `SUM(input_uncached + cache_write + output + reasoning)`. Phiên có ghép nhưng chưa có dòng usage vẫn trả đủ 12 số `0` (vẽ `▁`×12).
- **Stage từ `BRANCHES.md`:** `branch_entry_for` ([src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L365-L382)) tách `<lane>--<slug>` từ thành phần cuối của `cwd` (hoặc `task`), kiểm tra qua `branches::validate_entry`, đọc chỉ-đọc bằng `branches::get` và lấy khóa `status` cuối cùng.
- **Agent đã thoát và hàng `(no project)`:** Agent không còn `live` chỉ được giữ lại khi có mục hộp thư đang mở; `list_projects` đếm riêng các mục hộp thư đang mở không có `terminal` hoặc không khớp terminal nào có `root` vào `no_project`, hiển thị dòng `(no project)` màu xám và không thể `cd` vào.
- **Tài liệu:** `README.md`, `README_vi.md` và `CHANGELOG.md` đã được cập nhật khớp hành vi và cú pháp lệnh.

## 4. Bảo mật (Security)

- **Lọc chuỗi không tin cậy (ANSI/OSC injection):** `pv.key.name`, tên dự án trong dấu nhắc `agm[...]>` ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L171)), thông báo lỗi `note` khi `cd <q>` ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L153)), `label`, `app`, `model`, `effort`, `status`, `stage`, `doing.summary`, `last_msg`, `inbox` (`source`, `terminal`, `text`), `messages` (`from`, `to`, `kind`, `status`, `text`) đều đi qua `sanitize_one_line` / `one_line` và chốt cuối bằng `bounded_styled_line`.
- **Ràng buộc tham số SQL:** Cả 3 truy vấn trong [src/store/project.rs](file:///D:/Project/agmag.wt/tui-review/src/store/project.rs#L12-L129) chỉ ghép chỉ số `?1, ?2, …` từ `usize` (`numbered_placeholders`) và bind toàn bộ giá trị qua `ToSql` / `params_from_iter`.
- **Bảo vệ quyền người gọi (`Caller` guard) & tính chỉ-đọc của `cd`/`show`:** `tui::run` kiểm tra `Caller::User` ngay khi khởi động ([src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L229-L235)) và từ chối nếu không phải người dùng trên TTY. `TuiCmd::Cd` và `TuiCmd::Show` chỉ đọc CSDL / gọi `git rev-parse` / đọc `BRANCHES.md` mà không tạo khóa, không ghi tệp và không kích hoạt bất kỳ hành động can thiệp nào.

## 5. Độ vững chắc (Robustness)

- **CSDL rỗng / không có git / tràn số:** Truy vấn store với danh sách `parties` hoặc `sessions` rỗng thoát sớm an toàn; `usage_buckets` dùng `saturating_add` / `saturating_mul`; `git_common_root` thất bại sẽ rơi về `norm_cwd(cwd)` và được cache lại; `width = 0` không gây panic (`zero_width_does_not_panic`).
- **Độ rộng 40 / 60 / 110 & CJK / emoji:** Không cắt đứt cụm ký tự Unicode (grapheme clusters) và không vượt quá `width - 1` cột hiển thị.
- Còn một vài điểm biên với tên agent/trường rất dài, dự án trùng tên trong bảng `cd` và khi CSDL hoàn toàn chưa có dự án nào được nêu ở mục **Nên sửa**.

## 6. Chất lượng kiểm thử (Tests)

- Các bài test trong `tests/project_test.rs`, `tests/project_store_test.rs`, `tests/project_render_test.rs`, `tests/dashboard_test.rs` và `src/tui.rs` bao phủ tốt các luồng chính và 5 mục Review Focus.
- **Điểm test khẳng định còn thiếu (giải thích vì sao 2 lỗi đã biết trong `load_project_view` lọt qua test):**
  - Trong `load_project_view_joins_snapshot_and_store` ([tests/project_test.rs](file:///D:/Project/agmag.wt/tui-review/tests/project_test.rs#L545-L619)):
    1. Không có terminal `live` nào ở trạng thái `status = Some("blocked"), kind = Some("question")` hoặc `handoff_state = "handed_off"`, nên test chỉ kiểm `a1.status == "working"` (trùng giữa `TerminalRow.status` và `TermView.status`) mà không phát hiện `load_project_view` lấy `t.status` thay vì `view.status`.
    2. Tin nhắn của terminal đã thoát được gán giữa `"t3"` (terminal đã thoát **nhưng có inbox mở** nên nằm trong `kept`) và `"ceo"` (đang `live`, cũng nằm trong `kept`), không có tin nhắn nào giữa `"t4"` (đã thoát, không có inbox mở) với một terminal ngoài dự án (`"x9"`), nên không phát hiện việc `messages_involving` chỉ lọc theo `parties(&kept)` thay vì toàn bộ `members`.

---

## Blockers

Không có Blocker mới (ngoài 2 lỗi đã biết trong `project::load_project_view` đang được sửa riêng).

---

## Nên sửa

1. **[src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L456-L483) & [src/project_render.rs](file:///D:/Project/agmag.wt/tui-review/src/project_render.rs#L161-L177) — Lưu ý khi sửa `AgentView.status` và `stage` trong `load_project_view`:**
   - Khi lấy `status` từ `view` (`TermView.status` của `Snapshot`), nếu một terminal `live` có `handoff_state == "handed_off"`, `TermView.status` sẽ có hậu tố `" [HANDED OFF]"` (ví dụ `"idle [HANDED OFF]"` hoặc `"working [HANDED OFF]"`). Trong `status_parts` ([src/project_render.rs](file:///D:/Project/agmag.wt/tui-review/src/project_render.rs#L161-L177)), các phép so sánh `status == "working" || status == "running"`, `status == "idle"`, `status == "done"` đang so khớp tuyệt đối nên sẽ rơi vào nhánh `?` (`? idle [HANDED OFF]`) thay vì giữ ký hiệu `●` / `○` / `✓`. Nên tách hoặc `strip_suffix(" [HANDED OFF]")` khi chọn ký hiệu trong `status_parts`.
   - Với terminal **đã thoát** (không có `view` trong `snap.terminals`), nếu trong CSDL `t.status` còn lưu `Some("blocked")` (chữ thường), cần định dạng qua cùng quy tắc `(t.status, t.kind)` như `dashboard::snapshot` ([src/dashboard.rs](file:///D:/Project/agmag.wt/tui-review/src/dashboard.rs#L124-L129)) để `status_parts` và `status_color` nhận diện được tiền tố `"BLOCKED"` hoa.
   - Ở dòng 456 (`branch_entry_for(t.cwd.as_deref(), t.task.as_deref())`), nếu terminal `live` không có `t.cwd` mà lấy `cwd` từ phiên ghép (`view.map(|v| v.cwd.as_str())` như trong `compute_roots` tại [src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L390-L398)), `branch_entry_for` nên dùng cùng giá trị `cwd` đã fallback đó để không bị mất `stage`.

2. **[src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L425) — Điều kiện giữ terminal đã thoát trong `load_project_view` chưa bỏ qua `name` rỗng:**
   - `t.name.as_deref() == Some(term)` ở dòng 425 chưa lọc `.filter(|n| !n.is_empty())` như `list_projects`, `parties` và `tree_order`. Nếu có một terminal đã thoát mang `name: Some("")` và một mục inbox mồ côi cũng mang `terminal: Some("")`, terminal đã thoát đó sẽ bị giữ lại nhầm trong danh sách AGENTS của dự án.

3. **[src/project_render.rs](file:///D:/Project/agmag.wt/tui-review/src/project_render.rs#L283-L305) & [src/project_render.rs](file:///D:/Project/agmag.wt/tui-review/src/project_render.rs#L370) — Giới hạn độ rộng của tên agent (`agent.label`) và `item.source` / `message.kind` rất dài:**
   - Trong `agent_identity_line` (dòng 283), `agent.label` và `model_effort` chưa được giới hạn độ rộng tối đa (khác với bảng TERMINALS toàn cục giới hạn `NAME_CELLS`). Nếu tên terminal hoặc tên model rất dài (ví dụ 80–100 ký tự), phần đầu sẽ đẩy toàn bộ `status`, `ctx`, `sparkline`, `tokens`, `stage` ra ngoài lề phải và bị `bounded_styled_line` cắt mất.
   - Trong `inbox_lines` (dòng 370), `source` mới chỉ qua `sanitize_one_line` mà chưa giới hạn qua `one_line(&item.source, (max_w / 5).max(3))` (như `dashboard.rs:1536` đã làm); tương tự `message.kind` trong `message_lines` (dòng 396, 410). Nếu `source` hoặc `kind` chứa chuỗi dài bất thường, `first_prefix` sẽ chiếm hết dòng đầu của mục hộp thư / tin nhắn.

4. **[src/project_render.rs](file:///D:/Project/agmag.wt/tui-review/src/project_render.rs#L62-L101) & [src/tui.rs](file:///D:/Project/agmag.wt/tui-review/src/tui.rs#L531-L550) — Hiển thị lệnh `cd` khi trùng tên dự án hoặc khi danh sách dự án rỗng:**
   - Khi hai gốc dự án khác nhau có cùng tên thư mục cuối (ví dụ `D:\a\shop` và `D:\b\shop`), `match_project` đã phân biệt bằng `shop (d:\a\shop)`, nhưng `render_project_list` (khi gõ `cd`) mới chỉ in `p.key.name` nên hai dòng đều hiện `shop` giống hệt nhau (đặc tả mục *Project identity*: *"If two roots share a name, show the root path to disambiguate"*).
   - Khi CSDL rỗng (`list.is_empty() && no_project == 0`), `render_project_list` trả về chuỗi rỗng `""` và `TuiCmd::Cd(None)` không đặt `note`, khiến người dùng gõ `cd` không thấy bất kỳ phản hồi nào trên màn hình. Nên hiển thị dòng xám `  (no projects)` (hoặc đặt `note`).

---

## Nhỏ

1. **[src/project.rs](file:///D:/Project/agmag.wt/tui-review/src/project.rs#L291-L295) — Thứ tự ưu tiên khi trùng tên terminal trong `tree_order`:**
   - `by_key.entry(name).or_insert(i)` giữ phần tử xuất hiện đầu tiên theo `created_ms ASC` (có thể là một terminal đã thoát còn inbox mở) thay vì ưu tiên terminal `live` rồi `created_ms` mới nhất như `list_projects` và `Store::terminal`.
2. **[tests/project_render_test.rs](file:///D:/Project/agmag.wt/tui-review/tests/project_render_test.rs#L271-L297) — Mở rộng `untrusted_text_is_inert`:**
   - Test hiện mới bơm chuỗi escape vào `doing.summary`, `inbox.text` và `message.text`; có thể bổ sung chuỗi escape vào `key.name`, `agent.label`, `agent.stage`, `inbox.source` và `render_project_list` để khóa chặt toàn bộ bề mặt đầu vào.

## KẾT LUẬN: ĐẠT
