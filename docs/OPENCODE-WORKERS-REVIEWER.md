# Hướng dẫn dùng OpenCode v2 — Thợ cơ khí, Audit, `gitnexus-sync` & Reviewer mặc định

OpenCode v2 (`@opencode/cli`) là động cơ thực thi chính cho các vai chạy tự động trong worktree: **Audit (`auditor`)**, **Tìm trong repo (`explore`)**, **Cập nhật GitNexus (`gitnexus-sync`)**, **Thợ cơ khí (`worker-fast/mid/strong`)**, và **Reviewer mặc định (`reviewer`)**.

## 1. Cài đặt & bẫy cần tránh trên Windows 11 (đã kiểm v2.3.2)

1. Cài bản v2 bằng:
   ```powershell
   npm install -g @opencode/cli
   opencode --version                 # phải là 2.x (gói "opencode-ai" là v1 cũ, đừng cài)
   ```
   Kiểm trong **cả PowerShell lẫn Git Bash** để chắc không gọi nhầm bản v1 cũ ở `~/.opencode/bin`.
2. Bật tự cập nhật: thêm `"update": "auto"` vào `~/.config/opencode/opencode.json`.
3. **Thoát hẳn OpenCode trước khi nâng cấp:** dịch vụ nền `opencode serve --service` giữ khóa tệp `.exe`, làm `npm i -g` / `opencode upgrade` treo và có thể làm mất lệnh `opencode`.
4. **Đăng nhập v2 lưu trong SQLite (`~/.local/share/opencode/opencode.db`), không còn `auth.json`:** xem provider đã đăng nhập bằng `opencode auth list`. `agm` chỉ đọc hai bảng phiên/tin nhắn (`READ_ONLY`), không bao giờ chạm bảng `credential`/`account`.

---

## 2. Phân tầng model trong OpenCode (ưu tiên miễn phí trước, tránh OpenAI khi không cần)

| Vai trò / Template | Nguồn model khuyên dùng | Mục đích & Giới hạn |
|---|---|---|
| **`auditor.md`** (Audit & cổng ESLint/CI) | **Nhẹ / Miễn phí:** `opencode/deepseek-v4.1-flash-free`, `openrouter/thinkingmachines/inkling-2-small:free`, hoặc **Ollama cục bộ** (`num_ctx ≥ 64k`) | Chạy `eslint --max-warnings 0`, quét diff cấm `eslint-disable`/`@ts-ignore`/sửa cấu hình lint ([ESLINT-CI-GUARDRAILS.md](ESLINT-CI-GUARDRAILS.md)), chạy test/build, đối chiếu `git diff` với `.agents/log/<ID>.md`. Không sửa code |
| **`explore`** (Tìm mã & tóm tắt log dài) | **Nhẹ / Miễn phí:** `:free` hoặc Ollama cục bộ | Đọc nhiều tệp / rút gọn log test dài nghìn dòng trước khi đẩy vào BOT-CEO / Claude |
| **`gitnexus-sync.md`** (Cập nhật chỉ mục GitNexus) | **Nhẹ / Miễn phí:** `:free` hoặc Ollama cục bộ | Chạy `gitnexus status`, `gitnexus analyze --index-only`, kiểm tra MCP HTTP server. Không sửa repo |
| **`worker-fast.md`** (Thợ bậc Nhanh — **mặc định ưu tiên**) | **Nhẹ / Miễn phí / Rẻ:** `opencode/deepseek-v4.1-flash-free`, OpenRouter `:free` (dự phòng 2–3 slug), Ollama cục bộ 7B–14B, hoặc `gpt-6.1-terra-fast` | Nhờ có hàng rào ESLint + typecheck + test tự kiểm trong worktree, nhận **mặc định mọi task rõ đặc tả ≤ 2–3 tệp** (CRUD, UI component, API handler, unit test, fixture, refactor cục bộ). Chỉ nâng lên `worker-mid` khi không qua được cổng lint/test sau 2 lần tự sửa |
| **`worker-mid.md`** (Thợ bậc Vừa) | **Trả phí vừa (non-OpenAI):** OpenRouter / Ollama Cloud | Task liên kết 3–5 tệp, logic nghiệp vụ nhiều nhánh hoặc vừa fail 2 lần ở `worker-fast` |
| **`worker-strong.md`** (Thợ bậc Mạnh) | **Trả phí mạnh trên OpenRouter** hoặc `openai/gpt-6.1-sol` *(chỉ khi `agm quota` cho thấy `codex` còn > 20–30%)* | Logic phức tạp, đa tệp, cứu hộ task fail ở `worker-mid` |
| **`reviewer.md`** (Reviewer mặc định) | **Model mạnh non-OpenAI** (OpenRouter / Ollama Cloud) | Duyệt độc lập không ăn vào hạn mức ChatGPT/Codex & Claude; chủ động gọi GitNexus MCP (`detect_changes`, `impact`) |

### Mẹo dùng model nhẹ / miễn phí không bị phản tác dụng
- **Khóa bằng hàng rào ESLint & CI ([ESLINT-CI-GUARDRAILS.md](ESLINT-CI-GUARDRAILS.md)):** mọi thợ `worker-fast` bắt buộc chạy `npx eslint . --max-warnings 0 --report-unused-disable-directives` (hoặc `ruff` / `clippy -D warnings`) đạt `0 error, 0 warning` trước khi commit, và tuyệt đối không được chèn `eslint-disable` hay `@ts-ignore`. Linter đóng vai "người giám sát 0 đồng" giúp model nhẹ tự sửa lỗi cú pháp/kiểu/độ phức tạp ngay trong worktree.
- **Kiểm tra năng lực gọi công cụ trước:** trước khi đưa một model `:free` hoặc Ollama local mới vào `worker-fast.md`, cho làm thử 1 bài mẫu (sửa 1 tệp trong worktree → chạy lint + test → ghi `.agents/log/` → commit). Model gọi công cụ lỗi thì loại.
- **Ollama cục bộ:** bắt buộc đặt cửa sổ ngữ cảnh tối thiểu **`64k` (`num_ctx`)**, nếu thấp hơn OpenCode dễ lỗi tool calling.
- **Dự phòng `429`:** các model `:free` trên OpenRouter có giới hạn tốc độ theo phút; khai báo sẵn 2–3 slug trong `opencode.json` để đổi nhanh khi nghẽn.
- **Theo dõi hiệu quả thật:** chạy `agm cost --by source` để đo tỷ lệ token đã đẩy sang `opencode` / `:free` so với `claude` và `codex`.

---

## 3. Khai báo quyền & vòng đời trong `.opencode/agent/` (hoặc `.opencode/agents/`)

*(Kiểm tra xem bản OpenCode đang dùng đọc thư mục số ít hay số nhiều; trong `MATHY-AI` và `agmag` đang dùng `.opencode/agent/`.)*

- **Khóa quyền chặt cho từng vai:**
  - `worker-*`: `edit: allow` (nhưng prompt chỉ cho sửa tệp trong `.agents/tasks/<ID>.md`), `bash` chặn `git push`, `git merge`, `git rebase`, `git reset --hard`, `git worktree`, `rm -rf`, đọc `*.env*` / `*opencode.db*`.
  - `auditor` / `reviewer` / `gitnexus-sync`: cấm sửa mã nguồn sản phẩm.
- **Xoay vòng tự động bằng `agm daemon`:**
  - OpenCode worker sinh bằng `agm spawn ... --role worker` khi chạm **`ctx_rotate` (mặc định 70%)** sẽ được `agm` tự động yêu cầu ghi bàn giao vào `.agents/log/<ID>.md` + commit, kiểm tra worktree sạch rồi mới `kill` phiên cũ và mở phiên mới cùng template/worktree để làm tiếp.
- **Số lượng đồng thời:** bị giới hạn cứng bởi `[policy].max_workers` trong `~/.agmag/providers.toml` (không vượt quá giới hạn đồng thời của gói Ollama Cloud nếu có).
