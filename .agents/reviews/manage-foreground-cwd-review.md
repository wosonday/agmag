# Review — manage-foreground-cwd

- Đối tượng: HEAD `bffff7ed9e3e005ee388940384d34da8cb9856a6`, phạm vi `git diff main...HEAD`.
- Reviewer: độc lập. Ngày 2026-11-21. Không sửa mã sản phẩm; chỉ ghi review và commit review.
- Thang đánh giá: **CHẶN** (phải sửa mới nghiệm thu) / **NÊN SỬA** (nợ kỹ thuật đáng xử lý) / **NHỎ** (nit, không ảnh hưởng nghiệm thu).

---

## Mục 1 — `src/hc.rs::effective_cwd`: đúng 3 ca của brief, xử lý rỗng/khoảng trắng

**Mức: không có vấn đề (ĐẠT).**

- `src/hc.rs:261-266`: hàm thuần `pub fn effective_cwd(&Value) -> Option<String>`.
  - `foreground.cwd` qua `as_str()` → `non_blank` (`!s.trim().is_empty()`) → `or_else` → `cwd` gốc. Đúng 3 ca brief: có foreground → foreground (`src/hc.rs:265`); foreground rỗng/thiếu → cwd; cả hai thiếu → `None`.
  - Chuỗi chỉ toàn khoảng trắng bị loại ở **cả hai** nguồn nhờ `trim()` dùng chung trong closure `non_blank` (`src/hc.rs:262-264`), không chỉ riêng foreground.
  - `foreground` không phải đối tượng (vd chuỗi `"pwsh"`) → `v["foreground"]["cwd"].as_str() == None` → rơi về cwd; không panic (Index của `serde_json::Value` trả `Null` an toàn).
- Test phủ đủ: `tests/daemon_test.rs:175` `effective_cwd_prefers_foreground_then_root` — foreground thắng (`181-185`), foreground `"  "` → cwd (`187-190`), thiếu foreground (`192-195`), cả hai thiếu → `None` (`197`), cả hai rỗng → `None` (`199-202`), foreground sai kiểu (`204-207`).
- `terminal_row_from` (`src/hc.rs:268+`) không bị động đến trong diff — đúng quyết định tách hàm riêng.

Kết luận mục 1: đúng brief, không có ghi chú.

---

## Mục 2 — `src/ops.rs::manage`: cwd từ foreground, câu hỏi `[y/N]`, không nới quy tắc xác nhận

**Mức: không có vấn đề (ĐẠT).**

- `src/ops.rs:318`: vẫn gọi `terminal_row_from(v)` để lấy `app` (`fresh.app`); thay `fresh.cwd` bằng `effective_cwd(v)` tại `src/ops.rs:324-326` → `t.cwd` trong bộ nhớ là thư mục foreground (là `cwd` gốc nếu foreground rỗng). Đây đúng nguồn "thư mục hiệu lực" của brief.
- `src/ops.rs:339`: vẫn có bước chặn `t.cwd.as_deref().filter(|c| !c.trim().is_empty())` → nếu `effective_cwd` trả `None` (cả hai thiếu/rỗng) thì báo lỗi "không biết cwd", không dựng spec với cwd rỗng. `src/ops.rs:366`: `parse_launch_spec(app, &argv, cwd)` — cwd đã được kiểm tra chắc chắn không rỗng (xem thêm `src/launch.rs:128-130`).
- Câu hỏi xác nhận in đúng cwd: `src/ops.rs:367-370` format `cwd={}` từ `spec.cwd` (chính là cwd hiệu lực), gọi qua `caller(store, false, Some(&question))` (`src/ops.rs:372`); `caller` in `"{question} [y/N] "` tại `src/ops.rs:46`. Người dùng thấy đúng thư mục trước khi gõ `y`.
- **Không nới quy tắc xác nhận**: diff `main...HEAD` trên `src/ops.rs` chỉ có 2 hunk — đổi import (`src/ops.rs:15`) và đổi nguồn cwd (`src/ops.rs:322-326`). `caller` (`src/ops.rs:33-56`) và `actions::set_managed_launch` (`src/actions.rs:562-...`) **không đổi**: vẫn `caller(store, false, ...)` (không nhận `--yes`, `src/ops.rs:371`), vẫn chỉ `Caller::User { confirmed: true }` mới lưu spec (`src/actions.rs:574-580`); agent bị từ chối (`src/actions.rs:585+`).
- Không đổi cách daemon lưu `terminals.cwd`: `t.cwd = Some(cwd)` ở `src/ops.rs:325` chỉ sửa bản sao `TerminalRow` trong bộ nhớ; `set_terminal_launch` (`src/store/v2.rs:280-291`) chỉ `UPDATE terminals SET managed = ?2, launch_spec = ...`, **không** đụng cột `cwd`. Nên thư mục gốc lưu trong CSDL vẫn nguyên như cũ — đúng quyết định của CEO.
- Lưu ý phạm vi (đã ghi trong log, không phải lỗi): không có test CLI giả `hc get` cho `manage` trên Windows; `manage` được phủ gián tiếp qua `effective_cwd` + các test rotation. Chấp nhận theo log `.agents/log/manage-foreground-cwd.md:22-24`.

Kết luận mục 2: đúng brief, quy tắc xác nhận nguyên vẹn.
