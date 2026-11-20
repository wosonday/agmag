# Hạn mức gói Codex qua `codex app-server` — nguồn tươi, 0 token

Ngày: 2026-11-20. Người viết: Iceman Free (task `codex-quota-stale`).
Trạng thái: **đã cài đặt** (`src/codex_quota.rs`, `agm quota --refresh`, daemon poller `[policy].codex_quota_poll_min`, mặc định 15 phút).

## 1. Phương thức và lệnh chính xác

- Binary: `codex` (CLI, đã kiểm trên máy: `codex-cli 0.170.0`, 2026-11-20 13:20 +07). Có thể ghi đè bằng biến môi trường `AGM_CODEX_BIN`.
- Chạy app-server JSON-RPC qua stdio: `codex app-server --stdio` (không mở lượt hội thoại, không prompt).
- Chuỗi gọi (JSON-RPC 2.0, mỗi message một dòng JSON trên stdin):
  1. `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"agm","version":"0.1"}}}` → chờ phản hồi `id=1`.
  2. `{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":{}}` → chờ phản hồi `id=2`, lấy `result`.
- `agm quota --refresh` gọi một lần theo chuỗi trên rồi ghi vào bảng `quota`. Daemon gọi nền theo `[policy].codex_quota_poll_min`.

## 2. Chi phí token và tác động lên log phiên

- **0 token, không mở lượt hội thoại:** `account/rateLimits/read` chỉ đọc hạn mức tài khoản; không gửi prompt, không tiêu quota hội thoại.
- **Không tạo rollout mới** trong `~/.codex/sessions`: kiểm tra 2026-11-20 13:20 +07, không có thư mục `~/.codex/sessions/2026/11/20`; rollout gần nhất là `2026/11/19` (cuối `2026-11-19T19-53-01-*.jsonl`). CEO đã chạy thật `AGM_HOME=<tạm> cargo run -- quota --refresh` và nhận số tươi (codex 7d `used 92%`, `stale: false`, khớp `/status`) mà không sinh phiên mới.
- Thời gian: một lần gọi app-server + đọc hạn mức thường hoàn tất trong vài giây (phụ thuộc mạng/đăng nhập); `fetch_usage` hiện chờ tối đa 10 giây cho mỗi phản hồi id (tổng hai lượt) — mục 3 của task yêu cầu siết lại thành hạn chót chung và diệt cây tiến trình khi quá hạn.

## 3. JSON mẫu (đã xóa định danh)

Phản hồi `result` (`GetAccountRateLimitsResponse`, codex-cli 0.170.0), `rateLimits` là single-bucket view; `usedPercent` là phần **đã dùng** (cùng nghĩa với log `token_count.rate_limits`):

```json
{
  "rateLimits": {
    "primary": {
      "usedPercent": 42,
      "windowDurationMins": 300,
      "resetsAt": 1795000000
    },
    "secondary": {
      "usedPercent": 92,
      "windowDurationMins": 10080,
      "resetsAt": 1795400000
    }
  }
}
```

`agm` cũng chấp nhận phản hồi JSON-RPC đầy đủ có bọc `{"jsonrpc":"2.0","id":2,"result":{...}}` khi parse.

## 4. Ánh xạ cửa sổ

| Trường | Ý nghĩa | Ghi vào `quota` |
|---|---|---|
| `provider` | Cố định | `codex` |
| `windowDurationMins == 300` | Cửa sổ 5 giờ (`primary`) | `window_minutes = 300` |
| `windowDurationMins == 10080` | Cửa sổ 7 ngày (`secondary`) | `window_minutes = 10080` |
| `usedPercent` | % **đã dùng** (0–100), lưu nguyên | `used_percent` (CLI/TUI hiển thị phần còn lại) |
| `resetsAt` | Mốc reset (epoch giây) | `resets_at_s` |
| thời điểm đọc | ms hiện tại lúc ghi | `ts_ms` / `observed_ms` |

Chỉ hai cửa sổ 300 và 10080 phút được chấp nhận; cửa sổ/cặp giá trị khác bị bỏ qua.

## 5. Xử lý lỗi

- `agm quota --refresh` và poller daemon: lỗi khi chạy `codex app-server`, không đăng nhập, JSON/`result` lỗi, không có cửa sổ hợp lệ, hoặc quá thời hạn → **không ghi gì** vào bảng `quota`, **không bao giờ ghi 0% hay 100%** thay cho dữ liệu thiếu.
- `agm quota --refresh` in cảnh báo ra stderr nhưng vẫn in các hạn mức Codex/Claude/agy khác. Poller daemon cảnh báo một lần cho mỗi thông điệp lỗi mới (tránh lặp).
- Bucket/cửa sổ không hợp lệ (window không phải 300/10080, `usedPercent` ngoài 0..100, thiếu trường) bị bỏ qua; nếu không còn cửa sổ hợp lệ nào → coi như lỗi, không ghi.
- Quá hạn: phải có hạn chót chung cho toàn bộ lần đọc và diệt cả cây tiến trình khi quá hạn (cùng lớp lỗi `opencode-metrics`, xem `src/opencode_models.rs::load_with`, `kill_tree`) — được siết trong mục 3 của `codex-quota-stale`.
