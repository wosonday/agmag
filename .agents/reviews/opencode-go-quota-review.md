# Review độc lập — opencode-go-quota

- Nhánh: `feat/opencode-go-quota`
- Task: `.agents/tasks/opencode-go-quota.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (1 commit squash, đã giải xung đột với `fix/codex-quota-stale` ở `src/main.rs quota_table` và `README`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện và commit riêng (tiền tố `opencode-go-quota-review:`). Không sửa mã sản phẩm.

## Tiến độ kiểm

- [ ] (1) Parse: percent = phần đã dùng; 300/10080/43200; kẹp 0–100; null/thiếu/meter lạ → không ghi, không giả 0%/100%
- [ ] (2) Fetch: hạn chót chung mọi bước, kill cả cây, không join luồng đọc khi quá hạn, test treo có biên dư
- [ ] (3) Bảng + TUI: nhãn 5h/7d/mo, limited đỏ, stale, dòng stale không hiện như số tươi, 60/110 cột
- [ ] (4) Stale opencode-go theo usage `opencode-go/*`
- [ ] (5) Ngân sách `provider:opencode-go:300`, stale = không có dữ liệu
- [ ] (6) Không có định danh tài khoản (org_..., tên workspace, usageURL) trong tệp diff
- [ ] (7) Docs/decision khớp mã
- [ ] Chạy `cargo test --offline`

## Phát hiện

_(sẽ cập nhật sau mỗi mục)_

## KẾT LUẬN

_(chưa xác định)_
