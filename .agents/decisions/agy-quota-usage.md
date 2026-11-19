# agy quota qua `agy --print /usage --output-format json` — đánh giá trước khi mở task

Ngày: 2026-11-19. Người viết: Claude Code (t1s), trả lời câu hỏi của agy t41.
Trạng thái: đề xuất, chưa có task, chưa sửa mã.

## 1. Kết quả chạy thử thật (1 lần, trong `%TEMP%\agy-usage-probe`)

- Thời gian: **3,6 giây** (khởi động tiến trình + gọi mạng), mã thoát 0.
- **0 token**: `usage` đều bằng 0, `num_turns: 0`, `conversation_id: ""`.
- **Không tạo hội thoại mới** trong `brain/`, nên không sinh phiên agy giả cho `ingest_agy`.
- Dữ liệu có cấu trúc nằm ở `command.data.groups[].buckets[]`. **Không** parse chuỗi `response` (dạng văn bản ngăn cách bằng tab):

```json
{"status":"SUCCESS","command":{"name":"usage","data":{"groups":[
  {"name":"Gemini Models","buckets":[
    {"id":"gemini-weekly","window":"weekly","remaining_fraction":0.1032,"reset_time":"2026-11-25T03:24:56Z"},
    {"id":"gemini-5h","window":"5h","remaining_fraction":0.1941,"reset_time":"2026-11-19T14:13:05Z"}]},
  {"name":"Claude and GPT models","buckets":[
    {"id":"3p-weekly","window":"weekly", ...}, {"id":"3p-5h","window":"5h", ...}]},
  {"name":"Gemini 4 Argon","buckets":[
    {"id":"gemini-argon-wtu-weekly","window":"weekly", ...}]}]}}}
```

## 2. Khớp với kiến trúc agm

Bảng `quota(provider, ts_ms, window_minutes, used_percent, resets_at_s)` dùng chung cho mọi nhà cung cấp, nên **không cần đổi schema**. `agm quota`, TUI và `policy::provider_percent` đọc bản ghi mới nhất theo từng cặp (provider, window). Chỉ cần thêm một nguồn ghi vào bảng, giống `hook.rs` (Claude) và `parse/codex.rs` (Codex).

| Điểm | Quyết định đề xuất |
|---|---|
| **Nhiều nhóm hạn mức** | agy có 3 nhóm độc lập. Không gộp thành một provider `agy`; mỗi nhóm là một provider riêng: `agy-gemini`, `agy-3p` (Claude/GPT qua agy), `agy-argon`. Nhóm `agy-3p` **khác** hạn mức `claude` của Claude Code, không được trộn hai nguồn này. |
| **Tên provider không chứa `:`** | Phạm vi ngân sách được tách bằng `split(':')` (`ops.rs:407`, `policy.rs:42`), nên phải dùng `-`. Ví dụ hợp lệ: `agm budget set provider:agy-gemini:300 --soft 80 --hard 95`. |
| **Ánh xạ tên nhóm** | Dựa vào tiền tố của `bucket.id` (`gemini-`, `3p-`, `gemini-argon-`), không dựa vào tên hiển thị `group.name`. Kiểm tiền tố dài nhất trước (`gemini-argon-` trước `gemini-`). Tiền tố lạ → đếm `unknown` và bỏ qua, không đoán. |
| **Cửa sổ** | `"5h"` → 300, `"weekly"` → 10080 (cùng số phút với Codex/Claude). Giá trị lạ → bỏ qua và đếm `unknown`. |
| **Dấu của số liệu** | agy trả về **phần còn lại**: `used_percent = (1 - remaining_fraction) * 100`, giới hạn trong 0–100. Đây là chỗ dễ sai nhất, phải có test riêng. |
| **Thời điểm reset** | Dùng lại `model::parse_rfc3339_ms` rồi chia 1000. Không thêm crate thời gian. |
| **Lỗi** | Các trường hợp `status != "SUCCESS"`, JSON lạ, agy chưa cài, chưa đăng nhập, mất mạng, hết thời gian chờ → **không ghi gì** và chỉ cảnh báo một lần. Không bao giờ ghi 0% hoặc 100% thay cho dữ liệu thiếu. |

## 3. Hiệu năng và chi phí

- **Không gọi trong mỗi chu kỳ 10 giây của daemon.** Mỗi lần mất khoảng 3,6 giây và sinh một tiến trình. Đề xuất:
  - cấu hình `[providers.agy] usage_poll_min = 10`; đặt 0 để tắt;
  - chỉ gọi khi có hoạt động agy (token mới trong vài phút gần nhất, hoặc có terminal app `antigravity` đang `working`);
  - thêm `agm quota --refresh` để người dùng chủ động làm mới.
- **Không chặn vòng lặp daemon.** Chạy agy có thời hạn và kill tiến trình khi quá hạn: dùng lại cơ chế của `ask.rs` (timeout ngắn, khoảng 20 giây) hoặc chạy trên luồng riêng. Không để sự kiện `hc` bị trễ vài giây mỗi lần gọi.
- **Số dòng lưu trữ:** mỗi lần gọi ghi 5 dòng. Gọi mỗi 10 phút → khoảng 720 dòng/ngày, chấp nhận được. Có thể bỏ qua dòng giống hệt lần trước (cùng `used_percent` làm tròn và cùng `resets_at_s`).
- **Dữ liệu cũ:** ngân sách theo provider đọc bản ghi mới nhất bất kể đã cũ bao lâu. Nên để `agm quota`/TUI hiển thị tuổi dữ liệu (đã có `observed_ms`), và cân nhắc để policy bỏ qua bản ghi cũ hơn khoảng 2 chu kỳ poll.

## 4. Kết quả kiểm chứng (2026-11-20, thêm 4 lần gọi)

| # | Câu hỏi | Kết quả | Hệ quả cho thiết kế |
|---|---|---|---|
| 1 | `/usage` có tiêu hạn mức không | **Không.** Gọi 3 lần liên tiếp, `3p-*` và `argon` giữ nguyên, `gemini-*` thậm chí **tăng** nhẹ (0,002252 → 0,002661) | Không tốn hạn mức. Rate limit phía máy chủ vẫn chưa rõ, nên giữ poll thưa |
| 1b | Số liệu có mới không | **Có thể trễ một lần gọi.** `cli.log` ghi `Cache(retrieveUserQuotaSummary): Singleflight refresh failed: … context canceled`: agy trả số liệu từ **cache**, còn lượt làm mới chạy nền bị hủy khi tiến trình thoát. Lần 1 khác lần 2, lần 2 bằng lần 3 | Coi thời điểm quan sát (`observed_ms`) là gần đúng. Khi cần số mới (`agm quota --refresh`), gọi 2 lần cách nhau vài giây và lấy lần sau |
| 2 | Có ghi tệp của agy không | Mỗi lần gọi thêm khoảng 1,1 KB vào `cli.log`, nhưng tệp chỉ khoảng 44 KB nên có xoay vòng/cắt. Không tạo thư mục `brain/`, `conversation_id` rỗng | Không làm bẩn số liệu token. Không cần lo tệp phình |
| 3 | Chạy song song với phiên agy tương tác | Đạt khi `t41` (agy) đang mở và rảnh. **Chưa thử** khi phiên đang `working` | Vẫn cần timeout và kill khi quá hạn |
| 4 | Mất mạng | Giả lập bằng proxy hỏng: **mã thoát 1**, khoảng 7,5 giây, stdout vẫn là JSON `{"status":"ERROR","error":"/usage failed: …"}`; stderr có thêm dòng `error: …` | Chỉ parse **stdout**. Lỗi khi mã thoát ≠ 0 **hoặc** `status != "SUCCESS"`. Timeout ≥ 15 giây |
| 4b | Chưa đăng nhập | **Chưa thử** (cần HOME giả, có thể mở luồng đăng nhập tương tác) | Xử lý như lỗi chung. Không bao giờ ghi 0% hoặc 100% |
| 5 | Gói khác (Ultra/Free) | **Chưa thử** (chỉ có một tài khoản). Ghi chú của chính agy: "Higher Argon quota (included as part of the larger Gemini quota bucket) is available on Ultra plans", nên gói Ultra có thể **không có** bucket `gemini-argon-*` riêng | Ánh xạ theo tiền tố `id`, bucket lạ thì bỏ qua và đếm `unknown`. Thiếu bucket không phải lỗi |

Thời gian mỗi lần gọi: 2,7–3,6 giây khi có mạng.

## 5. Test cần có (không gọi agy thật)

- Fixture JSON lấy từ mục 1. Kiểm:
  - đúng 5 dòng;
  - `used_percent` = 89,68 / 80,59 / 94,37 / 87,05 / 51,65;
  - cửa sổ 10080/300;
  - `resets_at_s` đúng.
- Fixture có `status: "ERROR"`, JSON hỏng, `window` lạ, `id` lạ → không ghi dòng nào (hoặc chỉ ghi các bucket hợp lệ) và đếm `unknown`.
- Trình chạy agy đặt sau một trait (giống `Hc`/`Git`) để test: không gọi khi `usage_poll_min = 0`; không gọi lại trong khoảng chờ; khi quá thời hạn thì vòng lặp vẫn tiếp tục.
- `agm budget set provider:agy-gemini:300 ...` kích hoạt đúng mức mềm/cứng từ dữ liệu mới.
