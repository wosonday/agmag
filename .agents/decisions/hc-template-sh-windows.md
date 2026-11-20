# hc template `sh` trên Windows — báo cáo lên herdr-core

## Cách tái hiện tối giản

1. Cài herdr-core (hc) trên Windows 11, tạo hoặc dùng template có sẵn, ví dụ `auditor`:
   `%APPDATA%\herdr-core\templates\auditor.md` với front matter:
   ```md
   ---
   command: opencode --model test
   ready: quiet
   ---
   Bạn là kiểm toán.
   ```
2. Chạy:
   ```powershell
   hc run --json --bg --name tpl-probe auditor "noop"
   ```
3. hc trả:
   ```json
   {"error":{"kind":"spawn_failed","message":"cannot start \"sh\": sh: not found on PATH"}}
   ```
4. Cùng lỗi với các template `worker-mid`, `worker-fast`, `worker-strong`, `auditor`.

## Lỗi thực tế

- hc chạy `command:` của template qua `sh -c` (giống Unix). Windows không có `sh` trên PATH mặc định, nên mọi template đều không mở được.
- Template thực sự nằm ở `%APPDATA%\herdr-core\templates\<tên>.md` trên Windows, không phải `~/.config/herdr-core/templates` như tài liệu cũ ghi.
- `agm` gọi `hc run <template>` và bị lỗi này; trước đó `agm` còn gộp mọi đối số sau `--` thành một chuỗi, nên `agm spawn ... opencode -- --prompt "<brief>"` bị OpenCode báo `Unrecognized flag`.

## Hành vi mong đợi

- hc nên chạy `command:` trực tiếp (không qua `sh`) trên Windows, hoặc tìm `sh` trong Git Bash/MSYS nếu có.
- Nếu bắt buộc qua shell, hc nên báo lỗi rõ ràng và cho phép chỉ định shell.
- `hc templates --json` nên trả đường dẫn thư mục template đang dùng để client biết đọc fallback.
- (Tuỳ chọn) hỗ trợ argv dạng mảng trong front matter để tránh phân tích shell.

## Cách agm xử lý tạm thời

- Khi `hc run <template>` trả `spawn_failed` có `"sh"` trong thông điệp, `agm` đọc template từ `AGM_HC_TEMPLATES` hoặc `%APPDATA%\herdr-core\templates`, parse `command:` (chỉ tách theo khoảng trắng; từ chối dấu nháy/ký tự shell), chạy thẳng argv đó qua `hc run`, rồi gửi thân template + nhiệm vụ vào terminal khi agent sẵn sàng.
- Không thay đổi hành vi trên Linux/macOS, nơi hc chạy được.