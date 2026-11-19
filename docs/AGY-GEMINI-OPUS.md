# Hướng dẫn dùng `agy` — Gemini (Argon, Hydrogen, 3.8 Flash) & Opus High

Antigravity CLI (`agy`) gánh phần lớn các vai **phản biện, cố vấn kỹ thuật, tối ưu harnessing, research và cập nhật docs/báo cáo** nhằm giảm tải hạn mức cho cả Claude Code lẫn Codex.

> **Lưu ý chung về `agy`:**
> - Kiểm tra danh sách model khả dụng theo gói Google hiện tại bằng `agy models`.
> - `agy` **không** đọc quy tắc quyền trong `.opencode/agent/*.md`. Mọi lượt chạy **chỉ đọc** phải dùng `--mode plan --sandbox` ở thư mục tạm ngoài repo (hoặc gọi thẳng qua `agm ask`, vốn tự chạy trong `agmag-ask` ngoài repo và ghi lại token vào `agm.db`). **Tuyệt đối không dùng `--dangerously-skip-permissions`.**
> - Không đưa bí mật (`.env*`), mật khẩu hay dữ liệu thật của khách vào prompt.

---

## 1. Bốn nhóm model qua `agy` và nhiệm vụ cụ thể

### 1.1. Opus High qua `agy` — Đồng Kiến trúc sư & Reviewer cấp cao (gánh tải cho Claude Code)
- **Mục đích:** thay thế trực tiếp phiên Claude Code ở hai việc tốn suy luận sâu nhất khi cần bảo toàn hạn mức 5h/7d của Claude:
  1. **Đồng Kiến trúc sư / Bàn thiết kế & Chia việc:** trao đổi nhiều lượt để chốt phương án (`.agents/decisions/<ID>.md`) hoặc chia `.agents/tasks/<ID>.md` cùng người dùng / BOT-CEO.
  2. **Reviewer cấp cao / Phân xử ca khó:** duyệt các nhánh phức tạp, rủi ro cao hoặc phân xử khi Reviewer mặc định đánh `CHƯA ĐẠT` / phân vân (nhánh thường **không** gọi Pre-review bằng Opus High để tránh tốn 2 lần model lớn).
- **Cách chạy tối ưu (giữ phiên trong repo để ăn `cache_read_tokens`, KHÔNG dùng `agm ask` one-shot):**
  - Mở phiên tương tác ngay tại gốc repo hoặc worktree (chế độ chỉ đọc `--mode plan --sandbox`) bằng template `sparring-opus` hoặc lệnh:
    ```powershell
    agy --model claude-opus-5-5-high --mode plan --sandbox
    # Hoặc nối tiếp phiên đang bàn dở:
    agy --model claude-opus-5-5-high --mode plan --sandbox --conversation <conversation-id>
    ```
    *(Đã kiểm `agy models` ngày 2026-11-20: Opus High là `claude-opus-5-5-high`; còn có `claude-opus-5-5-medium` / `-low`.)*
  - **Vì sao không dùng `agm ask` cho Opus High:** `agm ask` tạo phiên mới trong thư mục tạm ngoài repo (`agmag-ask`) cho mỗi câu hỏi → `cache_read_tokens = 0` (luôn đốt 100% input token mới) và bắt nhồi cả diff vào dòng lệnh (giới hạn ~32k ký tự trên Windows). Giữ phiên tương tác trong repo giúp `agy` tự đọc đúng tệp/dòng cần xem và tận dụng tối đa prompt cache, trong khi `agm` vẫn tự đo đầy đủ token từ `transcript.jsonl`.

### 1.2. Gemini Argon (`gemini-4-argon-medium` / `high`) — Cố vấn kỹ thuật & Tối ưu Harnessing
- **Hai trách nhiệm đặc thù:**
  1. **Cố vấn kỹ thuật & Review chuyên sâu:** đọc mã/diff (qua `agm ask --model gemini-4-argon-high ...`, hoặc chia nhóm tệp chạy song song theo mẫu `tools/argon_review.py` của `agmag` — lưu ý giới hạn độ dài dòng lệnh Windows ~32k ký tự khi nhét mã vào prompt). Cũng có thể dùng làm Reviewer chính hoặc review kép khi cần tách khỏi hạn mức Codex.
  2. **Tối ưu hóa Harnessing giữa các phiên:** cuối đợt/sau phiên, tổng hợp bằng chứng từ `.agents/reports/`, `.agents/log/`, `.agents/reviews/` và hộp thư `agm inbox` (các lỗi lặp lại ≥ 2 lần, model `:free` hay bị `CHƯA ĐẠT`, xung đột điểm nóng `BRANCHES.md`) để **cập nhật `AGENTS.md`, `CLAUDE.md`, `BOT-CEO.md`, template, skill, bảng phân loại `tier:`**.
- **Quy tắc sửa harness:**
  - Khi chỉ cố vấn/review: chạy `agm ask` / `--mode plan --sandbox`.
  - Khi sửa tệp harnessing: mở phiên `argon-advisor` trên **nhánh/worktree riêng** (`harness/<việc>`), ghi rõ vấn đề lớn của phiên trước và kỳ vọng cải thiện vào `.agents/decisions/<ID>.md`, commit trên nhánh. Mọi nới lỏng quy tắc cứng / quyền / cổng kiểm (cấp B) phải được người dùng duyệt trước khi chốt.

### 1.3. Gemini 4 Hydrogen (`low` / `medium` / `high`) & 3.8 Flash — Cập nhật Docs, Báo cáo & Review theo bậc nhẹ trước
- **Ưu tiên bậc nhẹ cho Docs nhỏ & Báo cáo cuối (`.agents/reports/<ID>.md`):**
  - Với việc **tổng hợp báo cáo cuối `.agents/reports/<ID>.md`** (từ `log/` + `reviews/` có sẵn) hoặc **cập nhật 1–2 mục docs nhỏ**: dùng **`gemini-3.8-flash-medium` hoặc `gemini-4-hydrogen-low`** để nhanh và tiết kiệm nhất.
  - Chỉ dùng **`gemini-4-hydrogen-medium`** khi cập nhật tài liệu kiến trúc dài, sửa đồng bộ nhiều tệp `docs/*.md` sau bước Reviewer (trước khi gọi GitHub Copilot).
  - **Nhiệm vụ:** đọc `.agents/tasks/<ID>.md`, `.agents/log/<ID>.md`, `.agents/reviews/<ID>-*.md` và `git diff`; cập nhật docs bị ảnh hưởng + tổng hợp `.agents/reports/<ID>.md`, ghi một mục vào `.agents/log/<ID>.md`, commit lên cùng nhánh để tạo `head` SHA cuối trước khi bàn giao `copilot-git`.
- **`gemini-4-hydrogen-high` (Review gói diff tùy chọn):**
  - Chỉ gọi khi chạy review đối chiếu tài liệu/spec/diff lớn đóng gói ngoài repo.

### 1.4. Gemini 3.8 Flash (`gemini-3.8-flash-low` / `medium`) — Mặc định cho Tìm kiếm & Tóm tắt nhanh
- **Nhiệm vụ:** là **model mặc định đầu tiên** khi gọi `agm ask` để tra cứu tài liệu kỹ thuật/API ngoài repo (`search_web`) hoặc tóm tắt nhanh (~7 giây trên máy thật). Chỉ khi câu hỏi cần tổng hợp chéo nhiều nguồn phức tạp mà `3.8-flash` trả lời chưa sâu mới nâng lên `gemini-4-argon-low` / `medium`.
- **Quy tắc đối chiếu bắt buộc:** câu trả lời từ Gemini research **chỉ là gợi ý** (đã từng trả sai số phiên bản npm dù kèm đúng URL). BOT-CEO / Codex phải kiểm tra lại mọi số phiên bản, giá, tên cờ bằng lệnh gốc (`npm view`, `--help`, trang chính thức) trước khi ghi vào `.agents/tasks/<ID>.md`.

---

## 2. Đo token `agy` trong `agm`

- **Phiên `agm ask`:** tự lưu usage + tóm tắt vào `agm.db` và khử trùng lặp với log tệp.
- **Phiên `agy` CLI tương tác:** `agm` tự quét `transcript.jsonl` trong `~/.gemini/antigravity-cli/brain/` và ghép terminal `antigravity` theo thư mục làm việc.
- **Lưu ý:** các phiên chat trên Antigravity IDE không ghi `transcript.jsonl` nên chỉ hiện số bước tóm tắt (không đo được token trừ khi gọi qua `agm ask`).
