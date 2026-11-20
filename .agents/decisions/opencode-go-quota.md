# Hạn mức OpenCode Go — nguồn máy đọc được và cài đặt

Ngày khảo sát: 2026-11-20. Cập nhật: 2026-11-20. Trạng thái: **đã cài** trong `agm` (nhánh `feat/opencode-go-quota`).

## 1. Nguồn

- Lệnh: `opencode api GET /api/experimental/usage/limits` (cùng cơ chế `opencode api` với danh mục model).
- Thời gian chạy: ~0,8 s. Token model: **0** (GET tới server OpenCode đang chạy; không sinh phiên, không gọi LLM).
- Không đọc `auth.json`, không đọc bảng `credential`/`account`/`control_account` của `opencode.db`, không mở trình duyệt/cookie.
- Kết quả thực tế (đã ẩn định danh): `data.go.{rolling,weekly,monthly}` với `percent` (0–100, **phần đã dùng**), `limited` (bool), `resetsAt` (ms epoch). `data` có thể là `null` khi chưa đăng nhập.

```json
{
  "data": {
    "go": {
      "rolling": { "percent": 0,  "limited": false, "resetsAt": 1795177392862 },
      "weekly":  { "percent": 40, "limited": false, "resetsAt": 1795392000000 },
      "monthly": { "percent": 20, "limited": false, "resetsAt": 1797728748000 }
    }
  }
}
```

Nguồn loại trừ: `opencode stats --json` (không có hạn mức), `opencode models`/cache (chỉ context), `opencode.db` session tables (token usage, không hạn mức), cào Go Console web.

## 2. Ánh xạ và quy ước (đã chốt)

- Provider: `opencode-go` (không chứa `:`). Ba bản ghi mỗi lần đọc, bỏ trùng theo `UNIQUE(provider, ts_ms, window_minutes)`.
- Meter → cửa sổ: `rolling` → **300** phút (5h, nhãn `5h`), `weekly` → **10080** phút (7d, nhãn `7d`), `monthly` → **43200** phút (30 ngày, nhãn `mo`, dùng cho nhãn/khóa ngân sách; thời điểm reset lấy từ `resetsAt`).
- `used_percent = clamp(percent, 0..100)`, `resets_at_s = resetsAt / 1000`, `ts_ms` = thời điểm đọc.
- `data: null`, thiếu `go`, meter lạ, hoặc thiếu trường → bỏ qua phần đó, đếm unknown, **không ghi 0% hay 100%** thay dữ liệu thiếu. JSON hỏng → lỗi, không ghi.
- `limited: true` được lưu (setting `quota_limited:opencode-go:<window>`) và hiển thị nhãn `limited` (đỏ).

## 3. Cài đặt trong agm

- Module `src/opencode_quota.rs`: `parse_usage`, `record_usage`, `fetch_usage` (`opencode api GET /api/experimental/usage/limits`, đổi binary bằng `AGM_OPENCODE_BIN`, thử `opencode`/`opencode.cmd`, hạn chót chung 5 s, `kill_tree` khi quá hạn/lỗi, không `join` luồng đọc khi treo), `OpenCodeQuotaPoller`.
- Cấu hình: `[policy].opencode_quota_poll_min` (mặc định 15; 0 = tắt). Daemon đọc nền cạnh agy/codex; `agm quota --refresh` làm tươi `opencode-go`.
- Hiển thị: `agm quota` (bảng/JSON) và TUI PLAN QUOTA có 3 dòng `opencode-go` (`5h`, `7d`, `mo`), dùng chung stale/màu/reset; `limited` hiển thị đỏ.
- Stale: theo tuổi `quota_stale_min` **hoặc** có usage mới cùng gói sau `observed_ms`. Gói `opencode-go` ⇐ phiên OpenCode có model tiền tố `opencode-go/`. Khi không chắc biến thể `*-free` có tính vào Go hay không, **mọi `opencode-go/*` được coi là cùng gói** (an toàn, tránh bỏ sót stale).
- Ngân sách provider: `agm budget set provider:opencode-go:300 --soft 80 --hard 95` (và 10080/43200) hoạt động; dữ liệu stale xử lý như không có dữ liệu.
- Go Console vẫn là nơi xem Extra Usage/budget chi tiết.
## Giới hạn đã biết

- Diệt **cả cây** tiến trình khi quá hạn chỉ làm trên Windows (`taskkill /T /F`); Linux/macOS chỉ `kill` tiến trình trực tiếp — `agm` không treo (không `join` luồng đọc) nhưng tiến trình cháu có thể còn sống. Giống `src/opencode_models.rs` và `src/codex_quota.rs`; nếu cần thì sửa chung cả ba (process group) trong một việc riêng. (Review `opencode-go-quota`, phát hiện 2.1.)
