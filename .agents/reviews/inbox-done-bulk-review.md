# Review độc lập: inbox-done-bulk

- Người duyệt: thợ cơ khí (reviewer độc lập); không sửa mã sản phẩm.
- Đối tượng: HEAD `686028efece835f193433bdbc61c95cb625b0349` (`686028e`), phạm vi `git diff main...HEAD` (nhánh đã merge main).
- Tài liệu: `.agents/tasks/inbox-done-bulk.md` (mục "Quyết định của CEO" là thiết kế bắt buộc) và `.agents/log/inbox-done-bulk.md`.
- Thang mức: **CHẶN** / **NÊN SỬA** / **NHỎ**. Mọi lần chạy thử dùng `AGM_HOME` tạm; không chạy `agm done` trên inbox thật.

## 1. Parser ID / khoảng / danh sách — CHẶN

- Đạt: `N`, `N-M`, `N,M-K` và nhiều đối số (`src/actions.rs:123-168`; test `tests/actions_test.rs:549-574`). Chạy tay với `AGM_HOME` tạm:
  - `done 9-3` → `agm: error: invalid id range: 9-3` (exit 1);
  - `done a` → `invalid id selector: "a"` (exit 1);
  - `done 1-` → `invalid id selector: ""` (exit 1);
  - `done ""` → `invalid id selector: empty` (exit 1);
  - `done 1-10002` → lỗi rõ `id range too large` (`src/actions.rs:140-143`, test `tests/actions_test.rs:582-583`).
- **CHẶN (F1)** — tràn số với số rất lớn: `src/actions.rs:140` `let count = end - start + 1;`. Với `start = 0`, `end = i64::MAX` (`9223372036854775807`) phép `+ 1` tràn i64. Chạy thử thực tế: `agm done 0-9223372036854775807 --json` → `thread 'main' panicked at src\actions.rs:140:25: attempt to add with overflow`, exit 101 (bản debug). Ở bản release phép cộng wrap sang số âm, vượt kiểm tra `count > 10_000` rồi vòng lặp `src/actions.rs:144-146` chạy ~9,2×10^18 lần (treo/DoS). Vi phạm trực tiếp yêu cầu "không tràn số/overflow với số rất lớn" và yêu cầu khoảng quá lớn phải "lỗi rõ". Gợi ý sửa: thay bằng `if end - start >= 10_000` (tương đương `count > 10_000` nhưng không cộng 1) hoặc dùng `checked_add`/`u128` trước khi sinh danh sách.
- ID đơn cực lớn xử lý đúng: `done 9223372036854775807` → lỗi `không có mục hộp thư #…`, `done 9223372036854775808` → `invalid id selector` (đều exit 1, không panic) — `src/actions.rs:114-121`.
- Test chưa phủ ca cực đại: `tests/actions_test.rs:577-584` mới chỉ có `1-10002`; nên thêm `0-9223372036854775807` vào danh sách `bad`.
