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
