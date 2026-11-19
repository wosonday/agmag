# Hướng dẫn dùng Claude Code — Vai Kiến trúc sư & BOT-CEO

> Chỉ mở Claude Code cho **Kiến trúc sư / BOT-CEO** và khi **leo thang ca rủi ro cao** (bảo mật, dữ liệu, migration DB, kiến trúc lớn, hoặc bị `CHƯA ĐẠT` 2 lần liên tiếp). Không dùng cho Reviewer thường, viết code hàng loạt hay việc cơ khí.

## 1. Model và cách khởi chạy

```powershell
claude --model claude-opus-5-5     # Opus 5.5 (hoặc claude-fable-5-2 cho Kiến trúc sư tổng / lane phức tạp)
```

- Nên mở thẳng phiên Claude trong **HERDR** (tên terminal ngắn ≤ 32 ký tự `a-z 0-9 - _`, ví dụ `arch`, `ceo-tap-hoa`, `ceo-platform`) để bạn chat trực tiếp nhiều lượt.
- Cài hook đo hạn mức chính xác **một lần cho cả máy**:
  ```powershell
  agm hook install                 # thêm statusLine (context %, 5h %, 7d %); không ghi đè nếu đã có statusLine riêng
  ```
- Bật tự quản lý xoay vòng ngữ cảnh ngay sau khi mở phiên:
  ```powershell
  agm manage ceo-tap-hoa           # chỉ người dùng mới bật được; không bao giờ tự kill phiên Claude cũ
  ```

## 2. Quy tắc bắt đầu phiên theo khung Lauren Tan (`Goal first · Process second · Then routines`)

Khi nhận đề bài mới từ người dùng (kể cả dạng ghi chú thô / voice dump lộn xộn), Kiến trúc sư / BOT-CEO **làm chủ mục tiêu cuối (`you own the goal, not the steps`)** và bắt buộc tuân thủ 5 nguyên tắc trước khi sinh thợ (chi tiết tại [BOT-CEO.md §5.0](../BOT-CEO.md)):

1. **Làm rõ mục tiêu (`goal.first`) & giá trị (`serves: revenue | cost`):** hỏi đến khi người dùng chốt *"Đúng, chính là nó"*; mỗi lượt chỉ hỏi **đúng 1 câu** về quyết định ẩn (`/ask one question at a time`).
2. **Gom quy trình & Tạm dừng (`process.dump` + `hold`):** gom đủ `steps`, `watch_outs` (điểm nóng), `tools`, `check_work` (`eslint --max-warnings 0` + test), `info_from`, `feedback_from`; diễn đạt lại (`/restate`) ngắn gọn và **chờ xác nhận — tuyệt đối không `agm spawn` ngay khi chưa chốt kế hoạch**.
3. **Đi qua 3 ví dụ thực tế (`/walk 3 real examples`):** bắt buộc gồm cả **ca lỗi / biên thất bại** và chỉ rõ nguồn nào thắng khi dữ liệu mâu thuẫn (`conflict`).
4. **Định nghĩa `done` (`"sent" is not done`):** giao thợ hoặc code xong chưa phải là xong; chỉ `done` khi **`saved`** (commit + `.agents/log/` + test đúng trên nhánh) và **`confirmed`** (đã qua `auditor` ESLint/CI, `Reviewer`, `Hydrogen docs`, `copilot-git`). Việc đang `waiting` (`blocked`/`depends_on`) thì theo dõi và nối lại sau, không tạo nhánh trùng.
5. **Xuất khối chốt kế hoạch:** kết thúc bước chuẩn bị bằng `return { goal, process, rules, routine, next_step }` rồi hỏi 1 câu xác nhận để bắt đầu chạy tự động.

## 3. Nhiệm vụ nên làm & không nên làm

| Nên làm (đúng giá trị của Claude) | Không nên làm (đẩy sang model khác) |
|---|---|
| Hiểu đề bài, chốt `{ goal, process, rules, routine, next_step }`, phân tích rủi ro, chia nhỏ nhánh (< ~400 dòng diff) | Tự grep/đọc cả chục tệp khi chưa tra `GitNexus` (`query`, `context`, `impact`) |
| Trao đổi trực tiếp với người dùng trong lane của mình (mỗi lượt hỏi tối đa 1 câu quyết định ẩn) | Tự viết code/test dài thay thợ (`worker-fast` / `mid` / Codex) |
| Chốt tiêu chí đạt (`saved` + `confirmed`), viết brief / `.agents/tasks/<ID>.md` | Đọc cả log test dài hàng nghìn dòng (nhờ `explore` model `:free` tóm tắt trước) |
| Giữ chỗ `wip` / chốt `ready` trên `BRANCHES.md` (giữ khóa `.agents/.runtime/locks/branches`) | Tự duyệt mọi nhánh thường (đã có Reviewer OpenCode non-OpenAI; ca khó dùng `agy` Opus High) |
| Leo thang chẩn đoán khi thợ + Codex sửa 2 lần chưa xong | Tự merge vào `main`, push hoặc deploy (giao `copilot-git`) |

## 4. Tiết kiệm token & tránh phình ngữ cảnh

1. **Một lane = một BOT-CEO riêng** (xem [BOT-CEO.md](../BOT-CEO.md)): tuyệt đối không đọc sâu mã của lane khác nếu không có điểm chạm trên `BRANCHES.md`.
2. **Đọc qua GitNexus MCP trước (`GITNEXUS_MCP_READ_ONLY=1`):** đọc `gitnexus://repo/<tên>/context` → `query` → `context` → `impact` (`summaryOnly: true`), sau đó mới `Read` đúng đoạn cần xem với giới hạn dòng.
3. **Nạp 1–3 skill theo giai đoạn:** `brainstorming` lúc làm rõ yêu cầu, `system-design` / `architecture` lúc thiết kế; không nhồi cùng lúc.
4. **Dùng phiên tương tác `agy` Opus High để gánh tải trực tiếp:** khi hạn mức 5h/7d của Claude cao, mở phiên `agy --model claude-opus-5-5-high --mode plan --sandbox` ngay trong repo (giữ `conversation_id` để tận dụng `cache_read_tokens`, không gọi `agm ask` one-shot) để bàn thiết kế, chia `.agents/tasks/<ID>.md` hoặc phân xử review ca khó thay cho Claude.
5. **Xoay vòng ở 70% (`ctx_rotate`):** khi `agm daemon` yêu cầu bàn giao, ghi trạng thái vào `.agents/handoff/<id>-<ts>.md` kèm `rotation_id` và dòng cuối `<!-- agm:handoff-done -->`, rồi chuyển sang phiên kế nhiệm mới mở; dọn phiên cũ bằng `agm cleanup` sau khi kiểm tra xong.

## 5. Quy tắc an toàn cứng

- Không tự chạy `git merge`, `git push`, `nix`/deploy trên nhánh chung.
- `agm kill` / `agm interrupt` lên phiên Claude **bị khóa với mọi agent** — chỉ bạn (người dùng ở terminal có TTY, gõ `y`) mới dừng được phiên Claude.
