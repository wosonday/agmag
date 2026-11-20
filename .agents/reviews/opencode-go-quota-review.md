# Review độc lập — opencode-go-quota

- Nhánh: `feat/opencode-go-quota`
- Task: `.agents/tasks/opencode-go-quota.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (1 commit squash, đã giải xung đột với `fix/codex-quota-stale` ở `src/main.rs quota_table` và `README`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện và commit riêng (tiền tố `opencode-go-quota-review:`). Không sửa mã sản phẩm.

## Tiến độ kiểm

- [x] (1) Parse: percent = phần đã dùng; 300/10080/43200; kẹp 0–100; null/thiếu/meter lạ → không ghi, không giả 0%/100%
- [ ] (2) Fetch: hạn chót chung mọi bước, kill cả cây, không join luồng đọc khi quá hạn, test treo có biên dư
- [ ] (3) Bảng + TUI: nhãn 5h/7d/mo, limited đỏ, stale, dòng stale không hiện như số tươi, 60/110 cột
- [ ] (4) Stale opencode-go theo usage `opencode-go/*`
- [ ] (5) Ngân sách `provider:opencode-go:300`, stale = không có dữ liệu
- [ ] (6) Không có định danh tài khoản (org_..., tên workspace, usageURL) trong tệp diff
- [ ] (7) Docs/decision khớp mã
- [ ] Chạy `cargo test --offline`

## Phát hiện

### (1) Parse — ĐẠT (1 phát hiện nhỏ)

Đối chiếu `src/opencode_quota.rs:31-97` với task/decision:

- `src/opencode_quota.rs:74` — `used_percent = percent.clamp(0.0, 100.0)`: lưu nguyên phần **đã dùng**, không đảo. Khớp decision `...:10,30` và task `:17,27`.
- `src/opencode_quota.rs:50` — ánh xạ đúng `rolling→300`, `weekly→10080`, `monthly→43200`; `:78` `resets_at_s = resetsAt / 1000`.
- `src/opencode_quota.rs:36-38` (`data: null`), `:39-45` (thiếu `go`), `:52-56` (meter lạ), `:61-73` (meter null / thiếu `percent`/`resetsAt`/`limited`) → đều `continue` hoặc trả default, **không tạo snapshot giả**. `src/opencode_quota.rs:32-33` — JSON hỏng trả `Err`. `tests/opencode_quota_test.rs:16-99` phủ cả 5 nhóm; `cargo test --offline` xanh.

**Phát hiện 1.1 — mức: nhỏ.** `src/opencode_quota.rs:36-38`: nhánh `data.is_null()` trả `UsageParse::default()` nên `unknown = 0`, trong khi decision `.agents/decisions/opencode-go-quota.md:31` ghi `data: null` → "đếm unknown". Hệ quả: `agm quota --refresh` (`src/main.rs:533-536`) không in cảnh báo "skipped … meter(s)" khi chưa đăng nhập. Không có ghi 0%/100% giả, nên không chặn.
- Cách tái hiện: `parse_usage(r#"{"data":null}"#, 1)` → `p.unknown == 0`, `p.snapshots.is_empty() == true`. Kỳ vọng theo decision: `unknown == 1`.

## KẾT LUẬN

_(chưa xác định)_
