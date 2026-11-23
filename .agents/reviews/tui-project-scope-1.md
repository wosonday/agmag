- Đối tượng: HEAD `74cd928e71bc`, phạm vi git diff 5e93e85..HEAD (Task 1-3)

## Đối chiếu Giao diện Task 0, Global Constraints & Review Focus

- **Giao diện Task 0 & Global Constraints:**
  - Chữ ký hàm và kiểu dữ liệu trong `src/project.rs`, `src/store/project.rs`, `src/project_render.rs` khớp hoàn toàn với khung Task 0.
  - Không thêm dependency mới; giá trị chưa biết hiển thị `-` (kể cả khi `context_pct = Some(f64::NAN)` hoặc `INFINITY`); mọi chuỗi không tin cậy đều qua `sanitize_one_line`; mọi dòng xuất ra đều qua `bounded_styled_line(..., width.saturating_sub(1))`; giới hạn dòng wrap inbox ≤ 4, message ≤ 3 kèm `…`.
- **Review Focus (1–5):**
  1. *Chuẩn hóa đường dẫn Windows & worktree (`roots_are_normalized_and_worktrees_join_main`):* **Đạt.** `RootCache::root_of` chuẩn hóa qua `correlate::norm_cwd` cả đầu vào lẫn đầu ra của closure `resolve`, gom worktree về main repo và cache cả trường hợp `resolve` trả `None`.
  2. *Khớp theo tên hoặc địa chỉ terminal (`membership_matches_address_or_name`, `messages_involving_matches_any_listed_party`):* **Đạt** về mặt chức năng chính (còn một điểm nhỏ khi tên rỗng hoặc trùng tên với terminal đã thoát, nêu ở mục *Nên sửa*).
  3. *Chu trình `parent` và `parent` ngoài dự án (`tree_survives_cycles_and_foreign_parents`):* **Đạt** trong `tree_order` (không treo, không rơi agent, phá chu trình theo `created_ms`), nhưng giá trị `last_sibling = false` ở `depth = 0` khi ghép với `agent_lines` của Task 3 gây lỗi hiển thị cây (xem Blocker 1).
  4. *Chuỗi escape độc hại (`untrusted_text_is_inert`):* **Đạt.** Tất cả trường (`key.name`, `label`, `app`, `model`, `effort`, `status`, `stage`, `doing.summary`, `last_msg`, `inbox`, `messages`) đều được lọc qua `sanitize_one_line` trước khi tô màu và chặn lần cuối bởi `bounded_styled_line`.
  5. *Tràn độ rộng ở 40/60/110 cột với CJK/emoji (`no_line_exceeds_width_at_40_60_110`):* **Đạt** về giới hạn `< width` nhờ `bounded_styled_line` tính theo `UnicodeWidthStr::width` trên từng grapheme; tự bỏ sparkline khi `width < 70` và bỏ thanh `ctx` khi `width < 50`.

---

## Task 1: `src/project.rs` & `tests/project_test.rs`

- **Đánh giá:** Logic nhận diện gốc dự án (`RootCache`, `git_common_root`, `project_name`), gom nhóm (`list_projects`, `members`, `parties`), khớp lệnh `cd` (`match_project`), dựng thứ tự cây (`tree_order`) và suy luận mục `BRANCHES.md` (`branch_entry_for`) đúng theo đặc tả. Không có lỗi Blocker trong phạm vi riêng của Task 1.
- Các góp ý cho Task 1 nằm ở phần **Nên sửa** và **Nhỏ** bên dưới.

---

## Task 2: `src/store/project.rs` & `tests/project_store_test.rs`

- **Đánh giá:**
  - **SQL Injection:** An toàn tuyệt đối. `numbered_placeholders` chỉ sinh `?1, ?2, …` từ độ dài slice `usize`; toàn bộ giá trị được bind qua `ToSql` / `params_from_iter`.
  - **Xử lý đầu vào rỗng / biên:** `parties.is_empty()`, `sessions.is_empty()`, `n == 0`, `bucket_ms <= 0` đều thoát sớm an toàn, không sinh SQL `IN ()` lỗi hay chia cho 0.
  - **Toán chia bucket (`usage_buckets`):** Điều kiện `WHERE ts_ms >= ?{since_idx} AND ts_ms < ?{end_idx}` đảm bảo `ts_ms - since_ms >= 0` trước khi chia nguyên cho `bucket_ms > 0` (đúng cả khi `since_ms` hoặc `ts_ms` âm); tổng token gồm `input_uncached + cache_write + output + reasoning` và loại trừ `cache_read` đúng yêu cầu; mọi session được yêu cầu đều có mặt với vector `n` phần tử theo thứ tự cũ nhất trước.
  - Không có lỗi Blocker trong Task 2.

---

## Task 3: `src/project_render.rs` & `tests/project_render_test.rs`

### Blockers

1. **`src/project_render.rs:256-272` — Vẽ thừa cột `│ ` (hoặc thụt lề sai) ở lề trái toàn bộ cây agent do tính cả `stack[0]` của nút gốc `depth = 0`**
   - **Hậu quả:** Ở `depth = 0`, `agent_lines` không vẽ nhánh `├ ` hay `└ ` (`if depth > 0`), nhưng vẫn đẩy `stack[0] = !agent.last_sibling` rồi duyệt cả `stack[0]` khi tạo `prefix` cho các nút con (`for more in stack.iter().take(depth)`) và `cont` cho dòng `doing:` (`for more in stack.iter().take(depth + 1)`). Vì `tree_order` ở Task 1 (`src/project.rs:326`) **luôn** trả `last_sibling = false` cho mọi nút gốc `depth = 0` (`vec![(0, 0, false), ...]`), `stack[0]` luôn bằng `true`. Khi ghép Task 1 với Task 3 (hoặc ngay trong fixture `no_line_exceeds_width_at_40_60_110` tại `tests/project_render_test.rs:228`), dòng `doing:` của root bị in thành `" │   doing: ..."` và mọi nút con bị in thành `" │ ├ t7"`, `" │ └ t9"` với một đường kẻ dọc `│` treo lơ lửng ở cột 1 suốt chiều cao danh sách agent. Ngay cả khi truyền tay `root.last_sibling = true` như trong `agent_lines_show_tree_status_doing_and_unknowns` (`tests/project_render_test.rs:85`), `stack[0] = false` vẫn chèn dư 2 khoảng trắng vào trước mọi nút con (`"   ├ t7"` thay vì `" ├ t7"` như bản thiết kế).
   - **Cách sửa tối thiểu:** Bỏ qua tầng `0` (`skip(1)`) trong hai vòng lặp dựng cột nối dọc ở `src/project_render.rs:256` và `src/project_render.rs:269`:
     ```rust
     for more in stack.iter().take(depth).skip(1) {
         prefix.push_str(if *more { "│ " } else { "  " });
     }
     ```
     và:
     ```rust
     for more in stack.iter().take(depth + 1).skip(1) {
         cont.push_str(if *more { "│ " } else { "  " });
     }
     ```

2. **`src/project_render.rs:77,94` — `render_project_list` truyền chuỗi đã bọc mã màu ANSI vào `pad_cells`, làm lệch cột hàng `(no project)`**
   - **Hậu quả:** Ở dòng 94, `render_project_list` gọi `row(paint(GRAY, &one_line("(no project)", name_w)), ...)`, rồi bên trong `row` (dòng 77) lại gọi `paint(WHITE, &pad_cells(&name, name_w))`. Vì `pad_cells` đo độ rộng bằng `text.width()` (`UnicodeWidthStr::width`) trên chuỗi đã chứa `\x1b[90m(no project)\x1b[0m`, 7 ký tự ASCII của mã SGR (`[90m` và `[0m`) bị tính vào độ rộng hiển thị (19 ô thay vì 12 ô). Khi danh sách có bất kỳ dự án nào tên dài từ 13 đến 24 ký tự (`name_w > 12`), hàng `(no project)` bị thiếu tới 7 khoảng trắng căn cột so với các hàng dự án phía trên.
   - **Cách sửa tối thiểu:** Căn độ rộng `pad_cells` trên chuỗi thuần trước khi gọi `paint`:
     ```rust
     let row = |style: &str, name: &str, live: String, inbox: String| {
         format!(
             "  {}  {} {} {}",
             paint(style, &pad_cells(&one_line(name, name_w), name_w)),
             live,
             paint(GRAY, "·"),
             inbox,
         )
     };
     ```
     (truyền `WHITE, &p.key.name` cho dự án và `GRAY, "(no project)"` cho hàng không thuộc dự án).

---

## Nên sửa

1. **`src/project_render.rs:334` — Nguy cơ panic tràn số nguyên (`width - 1`) khi `width == 0`:**
   - Dòng 334 viết `(width - 1).saturating_sub(...)` trong khi mọi nơi khác trong file dùng `width.saturating_sub(1)`. Nếu `render_project_body` được gọi với `width = 0` và agent có `doing: Some(...)`, phép `width - 1` sẽ panic dưới chế độ debug/test. Nên đổi thành `width.saturating_sub(1)`.
2. **`src/project.rs:149-151, 265-270, 287-290` — Bỏ qua `name` rỗng và ưu tiên terminal `live` khi trùng tên:**
   - Nếu `TerminalRow.name` là `Some("")`, `by_name`, `parties` và `tree_order` hiện vẫn nạp chuỗi rỗng `""`. Nên lọc `.filter(|n| !n.is_empty())` giống `src/dashboard.rs:108`.
   - Trong `list_projects`, vì `store.terminals(false)` sắp theo `created_ms ASC`, `by_name.entry(name).or_insert(root)` sẽ giữ terminal cũ nhất (có thể đã `exited` ở dự án khác) thay vì terminal `live` / mới hơn (không nhất quán với `Store::terminal` trong `src/store/v2.rs:328`).
3. **`tests/project_render_test.rs:85, 120-121, 263-284, 288-311` — Test khẳng định còn lỏng ở 3 điểm:**
   - `agent_lines_show_tree_status_doing_and_unknowns` khởi tạo root với `last_sibling = true` (ngược với `false` từ `tree_order`) và chỉ kiểm `plain.contains("├ t7")` thay vì kiểm tra đầu dòng (`"\n ├ t7"`, `"\n └ t9"` và dòng `doing:` không có `"│ "` ở root).
   - `no_line_exceeds_width_at_40_60_110` đã kiểm tra ẩn sparkline ở `width = 60`, nhưng chưa kiểm tra thanh `ctx` vẫn còn ở `width = 60` và bị ẩn ở `width = 40` (`< NARROW_NO_CTX`).
   - `project_list_and_show` chỉ test tên dự án ngắn (`"agmag"`, `"shop"`) có độ dài `<= "(no project)".len()`, nên không phát hiện lỗi lệch cột của `pad_cells` khi có tên dự án dài 13–24 ký tự.

---

## Nhỏ

1. **`src/store/project.rs:105` — Phép tính `end` trong `usage_buckets`:**
   - `let end = since_ms + bucket_ms * n as i64;` có thể tràn `i64` nếu đầu vào biên truyền `n` hoặc `bucket_ms` rất lớn; dùng `since_ms.saturating_add(bucket_ms.saturating_mul(n as i64))` sẽ an toàn hơn.
2. **`src/project.rs:232-235` — So sánh trùng tên trong `ambiguous_entries`:**
   - `other.key.name == p.key.name` đang so sánh phân biệt hoa thường, trong khi `match_project` khớp tên không phân biệt hoa thường (`to_lowercase()`). Dùng `other.key.name.to_lowercase() == p.key.name.to_lowercase()` sẽ đồng nhất hơn khi `ProjectKey` không đi qua `norm_cwd` trên Windows.

## KẾT LUẬN: CHƯA ĐẠT
