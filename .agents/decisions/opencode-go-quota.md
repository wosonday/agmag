# Hạn mức OpenCode Go — khảo sát nguồn máy đọc được

Ngày: 2026-11-20. Người viết: thợ opencode-metrics (model miễn phí). Trạng thái: **chỉ khảo sát, chưa cài đặt** (không sửa schema, không thêm crate).

## 1. Nguồn đã thử

### 1.1 `opencode api GET /api/experimental/usage/limits` (nguồn duy nhất có hạn mức)

- Lệnh chính xác: `opencode api GET /api/experimental/usage/limits`
- Thời gian chạy: **~0,84 giây** (lần đo: 837,6 ms), mã thoát 0.
- Token tiêu tốn: **0 token model** (chỉ là GET tới server OpenCode đang chạy; không sinh phiên, không gọi LLM).
- Yêu cầu: server OpenCode đang chạy (cùng cơ chế với `opencode api GET /api/model`). Không đọc `auth.json`, không mở Console bằng cookie/phiên trình duyệt, không chạm bảng `credential`/`account`/`control_account` của `opencode.db`.
- Kết quả (đã xóa thông tin định danh):

```json
{
  "location": { "directory": "C:\\Users\\<user>" },
  "data": {
    "workspace": {
      "id": "org_<redacted>",
      "name": "<redacted>",
      "usageURL": "https://opencode.ai/console/<redacted>/activity/overview"
    },
    "go": {
      "rolling": { "percent": 100, "limited": true,  "resetsAt": 1795156077000 },
      "weekly":  { "percent": 40,  "limited": false, "resetsAt": 1795392000000 },
      "monthly": { "percent": 20,  "limited": false, "resetsAt": 1797728748000 }
    },
    "budgets": []
  }
}
```

- Schema OpenAPI: `UsageLimits.Go` gồm ba `UsageLimits.Meter` (`rolling`, `weekly`, `monthly`), mỗi meter có `percent` (0–100), `limited` (bool), `resetsAt` (ms epoch). `data` có thể là `null` khi chưa đăng nhập → không ghi gì.
- OpenAPI endpoint này được mô tả: "Return the OpenCode Go allowance and Console budgets of the OpenCode Console workspace the location is signed in to, or null when it isn't signed in."

### 1.2 Các nguồn đã loại

| Nguồn | Kết quả |
|---|---|
| `opencode stats --json` | Có token/cost/tool, **không** có hạn mức 5h/tuần/tháng. Không dùng. |
| `opencode models --verbose` / `~/.cache/opencode/models.json` | Chỉ có cửa sổ ngữ cảnh (`limit.context`), không có hạn mức. Dùng cho khoảng trống 1 (ctx), không cho quota. |
| `opencode.db` (`session_v2`, `session_message`) | Chỉ token usage và metadata phiên; không có hạn mức. Các bảng `credential`/`account`/`control_account` **không** được đọc (ràng buộc bảo mật). |
| Go Console (web) | Có đầy đủ, nhưng là giao diện web, không có nguồn máy đọc được khác ngoài endpoint trên. Không cào bằng cookie/phiên trình duyệt. |

## 2. Kết luận

**Có** nguồn máy đọc được đáng tin cho hạn mức Go 5 giờ / tuần / tháng: `GET /api/experimental/usage/limits`, trả về `go.rolling`, `go.weekly`, `go.monthly` với `percent` (phần đã dùng) và `resetsAt`. Đây là cùng cơ chế `opencode api` đã dùng cho danh mục model, nên không thêm crate, không cần cookie.

Lưu ý:
- `percent` là **phần đã dùng** (khác agy trả phần còn lại), nên ánh xạ thẳng vào cột `used_percent`, không đảo.
- `limited == true` cho biết cửa sổ đang bị chặn; có thể dùng để cảnh báo bổ sung, nhưng không thay thế `used_percent`.
- Endpoint có thể trả `data: null` (chưa đăng nhập) → không ghi gì, không ghi 0% hay 100%.
- Thời gian chạy ~0,8 s; không gọi trong mỗi chu kỳ 10 giây của daemon; đề xuất poll theo cấu hình (ví dụ 10–15 phút) hoặc `agm quota --refresh`.

## 3. Đề xuất ánh xạ vào bảng `quota`

Provider: `opencode-go` (không chứa `:`, vì `policy`/`ops` tách phạm vi bằng `:`). Ba bản ghi mỗi lần gọi, bỏ qua trùng theo `UNIQUE(provider, ts_ms, window_minutes)`:

| Meter API | Cửa sổ (phút) | Ghi chú |
|---|---|---|
| `go.rolling` | 300 | Cửa sổ lăn ~5 giờ (khớp Go Console "5 giờ"). Độ dài chính xác không có trong API; dùng 300 theo Console. |
| `go.weekly` | 10080 | 7 ngày. |
| `go.monthly` | (tháng dương lịch) | Không có số phút cố định; đề xuất lưu `window_minutes = 0` hoặc thêm quy ước riêng? Hiện schema `window_minutes: u32`. Cần quyết định trước khi cài: hoặc dùng 43200 (30 ngày) như xấp xỉ, hoặc mở rộng schema. **Dừng và hỏi** nếu muốn lưu đúng tháng. |

- `used_percent = meter.percent`
- `resets_at_s = meter.resetsAt / 1000`
- `ts_ms` = thời điểm gọi (ms epoch)
- `observed_ms` đã có trong `QuotaRow`, nên tuổi dữ liệu hiển thị được.

Ngân sách theo provider: `agm budget set provider:opencode-go:300 --soft 80 --hard 95` (và tương tự 10080). Ngân sách tháng cần cửa sổ đồng nhất với bản ghi.

## 4. Việc chưa làm (cần task riêng)

- Thêm nguồn ghi `quota` cho `opencode-go` (tương tự `agy_quota.rs`), với timeout/kill khi quá hạn, poll theo cấu hình, `agm quota --refresh`.
- Test fixture từ JSON trên (3 meter, `limited`, `data: null`, lỗi).
- Quyết định lưu cửa sổ tháng (xem mục 3).