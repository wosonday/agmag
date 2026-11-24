task: log-synthesis-2026-11-24
model: agy/gemini-4-hydrogen-medium
input-version: b66b8e0a9f75
captured-at: 2026-11-24T09:08:21+07:00
finished-at: 2026-11-24T09:08:50+07:00
findings:
(1) Lỗi lặp lại qua các vòng worker & review:
- Độ rộng hiển thị, tràn/underflow ở màn hình hẹp & nhầm cell vs scalar/SGR: `one_line` đếm scalar thay vì cell (.agents/reviews/tui-english-color-engineering.md:9); `pad_cells` nhận chuỗi đã bọc SGR làm lệch cột `(no project)` (.agents/reviews/tui-project-scope-1.md:53-54); `width - 1` có nguy cơ panic khi `width == 0` (.agents/reviews/tui-project-scope-1.md:73-74); `draw_cmdline` underflow khi `available == 0` (.agents/reviews/tui-app-1.md:6-7); `draw_block` vẫn in `"no CEO"` khi `height == 1` và dùng `'↓'` trong chế độ ASCII (.agents/reviews/tui-polish-1.md:26-27).
- Khử escape/control & UTF-8: không chặn `c.is_control()` hoặc thiếu `sanitize_one_line` ở thanh lệnh và prefill `s` (.agents/reviews/tui-app-1.md:10-12); `draw_output` gọi `sanitize_one_line` làm mất màu SGR (.agents/reviews/tui-app-1.md:28-29).
- Trạng thái UI lệch/cũ: không reload `data.conversation` khi đổi agent chọn (.agents/reviews/tui-board-1.md:6-7); `state.note` lỗi bám vĩnh viễn (.agents/reviews/tui-board-1.md:24-25); màn hình Global không refresh ngay sau lệnh đổi trạng thái (.agents/reviews/tui-app-1.md:14-15); `Tab` và `draw_sections` lệch danh sách panel (.agents/reviews/tui-app-1.md:32-33); `Panel::Projects` ẩn nhưng `Tab` vẫn đi qua (.agents/reviews/tui-board-2.md:18); `PgUp`/`PgDn` không cuộn Output khi focus ở `CmdLine` (.agents/reviews/tui-app-1.md:18-19).
- Thiếu cuộn & layout hẹp/wrap: thiếu `.scroll` ở Projects/Inbox/PickProject (.agents/reviews/tui-board-1.md:14-15); `draw_narrow` vẽ card 4 dòng thay vì list 1 dòng (.agents/reviews/tui-board-1.md:10-11); `rows.windows(2)` đứt connector khi wrap hàng (.agents/reviews/tui-board-1.md:20-21); `PickProject` dùng `.wrap` chung với `scroll_offset` theo item (.agents/reviews/tui-board-2.md:20).
- Test khẳng định lỏng nên lọt lỗi: chỉ test tên ngắn (.agents/reviews/tui-project-scope-1.md:81), thiếu ca `blocked`/exited không inbox (.agents/reviews/tui-project-scope-final.md:45-47), `has(&buf, "st")` trùng `"last"` (.agents/reviews/tui-polish-1.md:30-31).
- Lỗi mới sinh từ vòng sửa (`tui-app-fix1`): sửa underflow `draw_cmdline` bằng `saturating_sub(available).saturating_add(1).min(cursor_cells)` cắt mất ký tự đầu mọi lệnh ngắn (.agents/reviews/tui-app-2.md:7,20-22); dời `"task"` xuống cuối `columns` trong `draw_terminals` nhưng quên dời `cells.push(t.task)` gây lệch cột (.agents/reviews/tui-app-2.md:25-27); chuyển `draw_output` sang `ansi_to_text` làm lộ panic ranh giới UTF-8 khi `\x1b` đứng trước ký tự non-ASCII trong `skip_escape` (.agents/reviews/tui-app-2.md:30-31).

(2) Điểm mơ hồ trong plan/brief gây làm lại:
- Đóng băng Task 0 thiếu field cần thiết: `AgentView` thiếu `task` buộc worker C render `-` (.agents/log/tui-board-c.md:55-57), phải bổ sung ở `tui-p2a` (.agents/log/tui-p2a.md:3-5); `GlobalState` thiếu `prev_*_idx` khiến `sync` nhảy về cuối thay vì kề cận (.agents/log/tui-app-c.md:40), phải sửa ở `tui-app-fix1` (.agents/log/tui-app-fix1.md:47-51).
- Plan không chốt cơ chế khử trùng lặp `cleanup_list` giữa `Shown` và `render_frame` (.agents/log/tui-app-a.md:55-58), không nêu `SysSampler` cho `SYSTEM` hay tham số `theme` của `BoardScreen::new` (.agents/log/tui-app-e.md:70-75), và không chốt rõ nguồn `status`/`messages` cho `load_project_view` gây 2 blocker tích hợp (.agents/reviews/tui-project-scope-2-prep.md:25-38).
- Tiêu chí cột Terminals và Messages 1 dòng khi rộng không có trong brief đầu, phải bổ sung từ CEO preview vào `tui-app-fix1` (.agents/log/tui-app-fix1.md:68-81).

(3) Ghi chú theo model:
- `hy4-flash-free`: hoàn thành Track A `tui-board-a` sạch, 0 blocker (.agents/log/ceo-handoff-2026-11-23.md:37; .agents/log/tui-board-a.md:24-28).
- `iceman-free`: làm builder-check và Track B `tui-board-b` (.agents/log/ceo-handoff-2026-11-23.md:23,37), 16 test và gates đạt (.agents/log/tui-board-b.md:40-43), còn sót 1 điểm nhỏ ẩn `Panel::Projects` (.agents/reviews/tui-board-2.md:18).
- `deepseek-v4.1-flash` (`opencode-go`, trả phí rẻ): thay thế khi bản free chạm giới hạn (.agents/log/ceo-handoff-2026-11-23.md:37-38); gánh các track c/d/c2/fix1 và `tui-app-fix1` (.agents/log/ceo-handoff-2026-11-23.md:37,55-56), TDD tốt nhưng hay vướng lỗi biên UI/layout và tự tạo 2 lỗi hồi quy ở `tui-app-fix1` (.agents/reviews/tui-app-2.md:20-27).
- `agy gemini-4-hydrogen-high`: reviewer chính `tui-board-{1,2}` và `tui-app-1` (.agents/log/ceo-handoff-2026-11-23.md:38,55), bắt rất sát lỗi hồi quy, underflow và bảo mật escape.
- `nemotron` (`nemotron-3-ultra-550b-a55b`): probe không tool 4s OK, nhưng cả 2 lần tổng hợp ~160 KB đều treo quá hạn 15 phút (exit 124, không output), nghi do đụng `max-concurrent: 1` với phiên SpySWAT chạy song song (.agents/decisions/nvidia-log-synthesis-2026-11-24.md:3-12).

uncertain:
- `agy gemini-4-hydrogen-low` và `gemini-4-hydrogen-medium`: log chỉ nhắc chung `t7v` dùng `agy Gemini 4 Hydrogen` (.agents/log/ceo-handoff-2026-11-23.md:25-26), không ghi tier `low`/`medium` để đánh giá riêng.
- Các bản build chưa chạy kiểm chứng tương tác trên TTY thật do `Caller::User` chặn phiên agent (.agents/reviews/tui-project-scope-manual-run.md:6-10; .agents/log/ceo-handoff-2026-11-23.md:40-41).

proposals:
- **Brief template**: Chốt đủ field struct (`task`, `prev_*_idx`) trước khi đóng băng Task 0 (.agents/log/tui-board-c.md:55-57; .agents/log/tui-app-c.md:40). Với model lớn (`nemotron`), chạy 1 trial nhỏ trước, gộp 1 file ≤ 30 KB/chunk, kiểm tra không trùng lane `max-concurrent: 1` (.agents/decisions/nvidia-log-synthesis-2026-11-24.md:10-16).
- **Review-focus**: Thêm mục bắt buộc cho vòng fix: kiểm tra công thức `scroll` ở `0`/`1`/`< available`, khớp thứ tự `columns`/`cells` trong `Table`, và ranh giới UTF-8 khi nhảy byte trong parser ANSI (.agents/reviews/tui-app-2.md:20-32).
- **Test conventions**: Cấm `.contains()` toàn buffer cho token ngắn; kiểm từng token dòng tiêu đề hoặc theo vùng cột panel (.agents/reviews/tui-polish-1.md:30-35; .agents/reviews/tui-project-scope-1.md:79-81). Bắt buộc test tên dài > 12 ký tự, trạng thái `blocked`/exited, lệnh ngắn `1..available`, và escape đứng trước non-ASCII (.agents/reviews/tui-project-scope-final.md:45-47; .agents/reviews/tui-app-2.md:20-32).
- **Cleanup / line-ending hygiene**: Giữ nguyên CRLF cho file gốc CRLF như `src/board/mod.rs` (.agents/log/tui-app-e.md:58-59); xóa `tests/zz_preview_tmp.rs` trước khi commit (.agents/log/tui-board-c2.md:55; .agents/log/tui-board-fix1.md:65); không chạy 2 worker vào cùng 1 worktree (.agents/log/tui-project-g.md:41-43).

---
## CEO review of this synthesis (2026-11-24)
- Spot-checked citations (.agents/reviews/tui-app-2.md:20-22, .agents/log/tui-board-c.md:55-57,
  .agents/reviews/tui-polish-1.md:30-31, .agents/log/tui-app-e.md:58-59): accurate.
- **Correction — line endings:** the proposal "keep CRLF for files that were CRLF, like src/board/mod.rs" is wrong.
  src/board/mod.rs was created with LF (0 CRLF lines at creation); CRLF came from the CEO's Python edit scripts
  (text-mode writes on Windows) and was reverted in 179286e. Rule: keep each file's original ending; LF for
  src/board/*, src/store/project.rs, tests/board_*; CRLF only where it predates this work (src/main.rs, src/tui.rs,
  CHANGELOG.md, .ua/*). Check with `git ls-files --eol`.
- **Model notes the synthesis lacked** (from .agents/log/tui-p2a.md, tui-p2b.md, tui-w1.md, tui-w2.md):
  agy gemini-4-hydrogen-low did the mechanical AgentView task/role change cleanly; gemini-4-hydrogen-medium did
  layout.rs (9 tests) and both W1/W2 tracks in ~10 min each, one-line conflict at merge; review found 2 blockers.
  opencode deepseek-v4.1-flash stalled once for 46 min with no file change (tui-polish, killed). agy edits files and
  runs cargo non-interactively in default mode; `--mode auto` is not available for this account.
- Adopted: brief template must list every field the spec renders before freezing Task 0; fix-round review focus
  (scroll math at 0/1/<available, Table columns vs cells order, UTF-8 boundaries in byte parsers); tests check whole
  header tokens / panel regions, never short substrings over the whole buffer; cleanup removes only the exact files
  the CEO created (never `*.md` in a worktree).
