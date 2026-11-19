# Hướng dẫn dùng Codex — Kỹ sư chuyên sâu & Quản lý 1 luồng

> **Cảnh báo hạn mức quan trọng (đã xác minh trong `agmag`):** OpenCode gọi `openai/gpt-6.1-sol` **tiêu CHUNG hạn mức gói ChatGPT (5h / 7d) với Codex CLI** (hạn mức tính theo provider OpenAI, không tách theo công cụ). Vì vậy **không** dùng `gpt-6.1-sol` làm Reviewer mặc định hay viết báo cáo hành chính.

## 1. Khi nào dùng Codex (và khi nào KHÔNG dùng)

| Dùng Codex (`openai/gpt-6.1-sol`) | Không dùng Codex (chuyển sang công cụ khác) |
|---|---|
| **Engineering khó:** logic phức tạp, sửa đa tệp, migration DB, tối ưu hiệu năng | **Việc cơ khí 1–2 tệp có test sẵn:** giao `worker-fast` (`:free` / local) |
| **Gỡ lỗi sau 2 lần thất bại** ở `worker-fast` / `worker-mid` | **Audit & sàng lọc quy ước:** giao `auditor` (`:free`) |
| **Review chéo ca logic phức tạp** (second opinion) khi `agm quota` còn rộng | **Reviewer mặc định:** giao OpenCode non-OpenAI (OpenRouter/Ollama Cloud) hoặc `agy` (`Argon/Hydrogen High`) |
| **Quản lý điều phối ở dự án 1 luồng** (khi **không** dùng `BOT-CEO.md`) | **Điều phối trung gian trong chế độ `BOT-CEO.md`:** BOT-CEO gọi thẳng `agm spawn`, bỏ `codex-mgr` |
| — | **Viết báo cáo cuối `.agents/reports/<ID>.md`:** giao Hydrogen Medium hoặc model `:free` |

## 2. Hai cách chạy Codex

1. **Chạy trong OpenCode v2 theo đầu việc (khuyên dùng cho thợ bậc Mạnh):**
   ```powershell
   opencode run --model openai/gpt-6.1-sol ...
   ```
   Hoặc gắn vào template `worker-strong.md` khi cần nâng bậc cho một worktree cụ thể (`agm spawn worker-strong --task <ID> --role worker --cwd <worktree>`).
2. **Chạy Codex CLI tương tác (`codex-mgr` ở dự án 1 luồng hoặc thợ bậc Mạnh):**
   - Mở trong `herdr-core` (tên `codex-mgr` hoặc `w-<ID>-<n>`).
   - Nếu là phiên mở tay chạy lâu (như `codex-mgr`), bật `agm manage codex-mgr` để `agm` tự nhắc bàn giao sang `.agents/handoff/` khi ngữ cảnh đạt `ctx_rotate = 70%` (không kill phiên cũ).
   - Nạp tối đa 1–3 skill khi khởi tạo (`writing-plans`, `dispatching-parallel-agents`, `using-git-worktrees`).

## 3. Kiểm soát hạn mức ChatGPT 5h / 7d

- Trước khi mở phiên Codex nặng, kiểm tra:
  ```powershell
  agm quota
  ```
- **Ngưỡng điều phối đề xuất:** nếu cửa sổ 7 ngày (`codex 7d`) chỉ **còn < 20–30%** (đã dùng > 70–80%), tạm ngưng giao `gpt-6.1-sol` cho các task thường — đẩy `worker-strong` sang model mạnh trên **OpenRouter / Ollama Cloud** và chỉ giữ Codex cho ca bế tắc thật sự.
- Lưu ý trong `agm`: hạn mức OpenCode dùng `openai/gpt-6.1-sol` không hiện riêng trong log OpenCode mà phản ánh ở lần ghi `rate_limits` tiếp theo của Codex.

## 4. Kỷ luật worktree & git

- Khi đóng vai thợ engineering trong worktree: chỉ sửa đúng danh sách tệp trong `.agents/tasks/<ID>.md`, ghi mục vào `.agents/log/<ID>.md`, commit bắt đầu bằng `<ID>:`, không chạm `BRANCHES.md`, không merge/push.
- Khi đóng vai quản lý (dự án 1 luồng): không tự sửa code của thợ; nhận tin qua `agm inbox --json`, đổi model thợ khi fail 2 lần, kiểm `git status` / `git diff --stat` rồi bàn giao nhánh + SHA cuối cho `copilot-git`.
- Giống Claude, phiên Codex tương tác **không bị `agm` tự động kill** — chỉ người dùng (TTY + gõ `y`) mới dừng được.
