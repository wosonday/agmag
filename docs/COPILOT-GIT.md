# Hướng dẫn dùng GitHub Copilot CLI (`copilot`) — Bot quản lý phiên bản Git

GitHub Copilot CLI (`copilot-git`) là **agent duy nhất** được phép tích hợp (merge), xử lý lịch sử (`git revert`), và dọn worktree/nhánh đã tích hợp trên nhánh chung (`main`). Dùng tài khoản GitHub Copilot riêng, không ăn vào hạn mức Claude hay ChatGPT/Codex.

## 1. Khởi chạy & phạm vi hoạt động

- Chỉ chạy **đúng 1 phiên `copilot-git`** tại **checkout chính** (trên nhánh `main`), để hàng đợi tích hợp luôn **tuần tự** — mỗi lần merge chỉ chứa đúng một nhánh/tính năng.
- Cách mở phiên:
  ```powershell
  copilot
  # Yêu cầu mở đầu: Đọc AGENTS.md (và BOT-CEO.md / GIT_BOT.md nếu có). Bạn là copilot-git, chỉ xử lý hàng đợi theo đúng SHA đã duyệt.
  ```
- **Đã kiểm chứng (2026-11-18):** Copilot CLI 1.0.113 đạt 3/3 ca kiểm thử Git trên fixture độc lập (dừng khi checkout bẩn, từ chối SHA sai, merge đúng SHA — xem `COPILOT_GIT_SMOKE.md` trong `MATHY-AI`).

---

## 2. Điều kiện nhận bàn giao (trước khi merge)

Tùy mô hình dự án, `copilot-git` nhận việc từ:
- **Dự án đa lane (`BOT-CEO.md`):** đọc các mục có **`status: ready`** trong `BRANCHES.md` (lấy dưới khóa thư mục `.agents/.runtime/locks/branches`, nằm trong `.gitignore` để không làm bẩn `git status` trên checkout chính), kiểm tra `depends_on` đã `merged`/`deployed` chưa.
- **Dự án 1 luồng (`AGENTS-TUTORIAL.md`):** nhận gói bàn giao từ `codex-mgr` / Kiến trúc sư (tên nhánh, `head` SHA đầy đủ, lệnh kiểm chứng + kết quả thật, báo cáo Audit + Review + xác nhận docs của Hydrogen Medium).

**Từ chối merge ngay và trả về `needs-rebase` / `failed` nếu:**
1. Checkout chính đang bẩn (`git status --short` có thay đổi chưa dọn).
2. `head` SHA của nhánh không khớp với SHA đã duyệt ở bước Review/Docs (có commit chen ngang sau khi chốt `ready`).
3. Thiếu kết quả Audit/Review, hoặc thiếu commit cập nhật docs (nếu đầu việc có thay đổi ảnh hưởng tài liệu).
4. Nhánh phụ thuộc trong `depends_on` chưa vào `main`.

---

## 3. Quy trình 6 bước khi tích hợp một nhánh

1. **Đặt trạng thái `merging`** trong `BRANCHES.md` (giữ khóa `mkdir`) để các BOT-CEO khác biết.
2. **Xác minh đúng SHA và merge tuần tự** vào nhánh chung (`main`) tại checkout chính.
   - Nếu gặp xung đột merge, xung đột nghiệp vụ, hoặc có > 1 migration head: **dừng ngay, không tự sửa code để vượt cổng**; ghi lý do vào `BRANCHES.md` (`status: needs-rebase`) và báo cho BOT-CEO của lane qua `agm send <lane> --type LỖI --task <ID> "..."`.
3. **Chạy lại cổng kiểm tra (Audit)** sau merge trên checkout chính (test, build, lint). Nếu lỗi: ghi log, chuyển trạng thái `failed` (hoặc `git revert` nếu đã đẩy lên bước kế tiếp) và trả về quản lý/BOT-CEO.
4. **Kích hoạt làm mới GitNexus:** sau khi merge thành công, gọi **`gitnexus-sync`** (chạy `gitnexus analyze --index-only` ở checkout chính) hoặc hook `post-merge` để các BOT-CEO khác đọc được đồ thị mã mới nhất mà không bị `staleness: behind`.
5. **Cập nhật trạng thái cuối & dọn dẹp:**
   - Đổi trạng thái trong `BRANCHES.md` thành `merged` (hoặc `deployed` nếu dự án có bước deploy và đã được phép).
   - Chỉ xóa worktree và nhánh đã tích hợp (`cleanup-wt.sh`) sau khi kiểm tra worktree sạch và `agm ps` không còn phiên nào đang chạy trong thư mục đó.
6. **Báo hoàn tất** qua `agm send` để chuyển sang nhánh `ready` kế tiếp trong hàng đợi.

---

## 4. Hành động cần người dùng phê duyệt trực tiếp

`copilot-git` **không tự ý** thực hiện các thao tác sau khi chưa có lệnh cho phép của người dùng:
- `git push` lên remote, tạo `tag`, phát hành bản mới (`release`).
- Deploy lên môi trường thật (khi `MATHY_DEPLOY` hoặc cờ tương đương đang tắt).
- Thao tác viết lại / hủy lịch sử trên nhánh chung (`git reset --hard`, force-push). Khi cần hoàn tác trên nhánh chung, dùng **`git revert`** và ghi mục đính chính.
