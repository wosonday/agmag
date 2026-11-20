# Review độc lập — opencode-metrics (feat/opencode-metrics)

- Reviewer: reviewer độc lập; ngày 2026-11-20.
- Đối tượng: HEAD `0fa4093b9c4c5bd86cbe7a358164b2730bef3e48` (diff `main..HEAD`), worktree `D:/Project/agmag.wt/opencode-metrics`.
- Không sửa mã sản phẩm; đã đọc task, log thực thi và Global Constraints của plan.
- Bỏ qua lệch `cargo fmt` trên các tệp có sẵn ở baseline (theo chỉ dẫn).

## Kiểm theo yêu cầu / tiêu chí nghiệm thu

| # | Yêu cầu | Kết luận |
|---|---|---|
| (1) | Thứ tự `context_window`: full key > bare key > danh mục > None, chỉ dùng danh mục cho `Source::OpenCode` | **Đạt** — `src/config.rs:163-174` gọi `self.model(name)` (full key rồi bare id, `src/config.rs:156-159`) trước, rồi mới tra danh mục khi `source == Source::OpenCode`; test `catalog_context_respects_config_and_provider_identity` phủ cả 4 mức + phân biệt provider cùng id + `Source::Claude -> None`. |
| (2) | `opencode_models.rs`: timeout thật, không treo daemon, không gọi lại mỗi vòng, lỗi/JSON hỏng → None không panic, không đọc auth/credential | **Chưa đạt** — xem phát hiện 1. Các vế còn lại đạt: `OnceLock` (`src/opencode_models.rs:66-69`) → tải tối đa 1 lần/process, không có lời gọi eager nào ngoài `config.rs:172`; `from_json`/`load` trả `Err` → `cached()` → `None`; `from_value` bỏ entry hỏng không panic; chỉ chạy `opencode api GET /api/model`, không đọc `auth.json`/bảng credential. |
| (3) | `ctx_warn`/`ctx_rotate` chạy được cho thợ OpenCode, không xoay vòng sai | **Đạt (có chú thích)** — cửa sổ lấy từ `limit.context` (token) và `used` từ `ctx_used` (token) → `context_pct` (`src/config.rs:229-232`) cùng đơn vị, không lẫn output limit; `policy.rs:76-95` lọc đúng 4 nguồn như `report.rs:123-131`; `ctx_rotate` (`src/rotation.rs:208`) dùng chung `session_context_pct` nên đi cùng ngưỡng với `ctx_warn`. Việc `used` gồm cache là mã có sẵn từ trước (`src/opencode.rs:127`, không nằm trong diff này) và task xác nhận số liệu token đã đúng — không phát sinh rủi ro mới từ thay đổi này. |
| (4) | Bỏ `unknown format?` chỉ cho phiên OpenCode 0 token | **Đạt** — `src/report.rs:187` thêm `&& source != Source::OpenCode`; nhánh `bad > 0` vẫn cảnh báo; test `zero_token_opencode_session_has_no_warning_but_codex_still_does` kiểm hồi quy Codex. |
| (5) | `.agents/decisions/opencode-go-quota.md` kết luận rõ, an toàn | **Đạt (nhỏ)** — kết luận có nguồn (`/api/experimental/usage/limits`) rõ ràng; nêu rõ không đọc auth/cookie/bảng `credential`/`account`/`control_account`; ánh xạ rolling→300, weekly→10080, percent không đảo, `data:null` → không ghi; cửa sổ tháng để ngỏ "dừng và hỏi". Xem phát hiện 3 về định dạng. |
| (6) | Test kiểm thật các điều trên | **Một phần** — xem phát hiện 2. |

## Phát hiện

### 1. Timeout cleanup treo daemon vô hạn — mức: **chặn**

- **Vị trí:** `src/opencode_models.rs:123-127`, đặc biệt dòng 126 `let _ = reader.join();` trong nhánh `Err(_)` (timeout). Liên quan: dòng 111 `child.wait()?` và dòng 120 cũng không có giới hạn.
- **Vấn đề:** `recv_timeout(5s)` chỉ giới hạn thời gian chờ *dữ liệu*, không giới hạn dọn dẹp. Khi quá hạn, `child.kill()` chỉ terminate tiến trình trực tiếp. Trên Windows `opencode` phân giải tới `opencode.cmd` (đã kiểm: `where.exe opencode` → `...\opencode`, `...\opencode.cmd`), tức `cmd.exe` → tiến trình con `node.exe` kế thừa stdout. Giết `cmd.exe` không đóng đầu ghi của pipe do `node.exe` giữ, nên luồng reader vẫn kẹt trong `stdout.read_to_string` và `reader.join()` chờ vô hạn. Hệ quả: `agm` treo ở lần tra cứu context đầu tiên — vi phạm trực tiếp yêu cầu "có giới hạn thời gian thật sự, không treo daemon".
- **Cách tái hiện (đã chạy, có kết quả):** đặt một `opencode.cmd` giả lên đầu `PATH` với nội dung `@echo off` + `ping -n 40 127.0.0.1` (tiến trình con giữ stdout và không trả EOF), rồi chạy một chương trình Rust tái tạo đúng chuỗi `spawn → reader thread → recv_timeout(5s) → kill → wait → join` của `load_with`. Kết quả đo được:
  - `TIMEOUT at 5.0157s; killing child`
  - `ABOUT_TO_JOIN at 5.0216s`
  - tiến trình **vẫn còn sống sau 12 s** (`STILL RUNNING after 12s => HANG`), không bao giờ in `JOINED`.
  Trong sản phẩm, đường đi tương đương là bất kỳ lệnh nào gọi `ModelCatalog::cached()` với model OpenCode vắng mặt trong `providers.toml` (vd `cargo run --offline -- ls --json`, hoặc một tick policy) khi `opencode api GET /api/model` treo quá 5 s.
- **Hướng sửa gợi ý:** không `join` luồng reader trên đường timeout (cho nó detach/kết thúc cùng process), hoặc diệt cả cây tiến trình (Job Object / `taskkill /T /F`), hoặc đọc output trên luồng riêng và bỏ luồng đọc khi hết hạn; đồng thời đặt giới hạn cho cả `child.wait()` ở nhánh thành công.

### 2. Test chưa phủ các tính chất được yêu cầu kiểm — mức: **nên sửa**

- **Vị trí:** `tests/opencode_models_test.rs` (toàn tệp, 5 test); đối chiếu `src/opencode_models.rs:109-129`, `src/rotation.rs:208`.
- **Vấn đề:** các test hiện phủ tốt precedence/parse/report/`ctx_warn`/zero-token/bad-JSON, nhưng không có test nào cho: (a) giới hạn thời gian và "không treo" khi CLI treo (chính là lỗi ở phát hiện 1); (b) `cached()` chỉ tải một lần/process; (c) `ctx_rotate` kích hoạt từ cửa sổ chỉ có trong danh mục (mới chỉ test `ctx_warn`); (d) hồi quy cảnh báo cho Claude/agy (mới chỉ test Codex). Tiêu chí nghiệm thu ghi trong task đều đã đạt, nên không chặn, nhưng các tính chất được yêu cầu rà soát ở mục (2)-(4) chưa được bảo vệ bởi test.
- **Cách tái hiện:** `Select-String -Pattern 'timeout|cached|ctx_rotate|Claude|Agy' tests/opencode_models_test.rs` → không có kết quả cho các trường hợp trên; chạy lại đoạn tái hiện ở phát hiện 1 → test suite vẫn xanh dù daemon treo.

### 3. Tệp quyết định thiếu ký tự xuống dòng cuối — mức: **nhỏ**

- **Vị trí:** `.agents/decisions/opencode-go-quota.md:77` (dòng cuối).
- **Vấn đề:** diff có marker `\ No newline at end of file`; log bước 4 ghi đã sửa typo/dòng trống nhưng phần xuống dòng cuối vẫn còn thiếu. Chỉ là định dạng, không ảnh hưởng nội dung/kết luận.
- **Cách tái hiện:** `git diff main HEAD -- .agents/decisions/opencode-go-quota.md` → dòng cuối hiển thị `\ No newline at end of file`.

## Kiểm chứng đã chạy

- `cargo test --offline`: **259 passed / 0 failed** (26 binary có test + doc-tests 0; riêng `opencode_models_test`: 5 passed / 0 failed). Không chạy clippy/fmt trong review này; theo log tác vụ clippy sạch và lệch fmt chỉ là tệp baseline.

## KẾT LUẬN: CHƯA ĐẠT

Lý do: phát hiện 1 là lỗi chặn — cơ chế timeout của `opencode_models.rs` không giới hạn được toàn bộ thao tác và đã được tái hiện là treo vô hạn, vi phạm trực tiếp yêu cầu "giới hạn thời gian thật sự, không treo daemon". Các yêu cầu còn lại (1), (3), (4), (5) đạt; (6) đạt một phần. Cần sửa phát hiện 1 (và bổ sung test tương ứng) trước khi nghiệm thu.

## Xem lại vòng sửa 1

- Reviewer: reviewer độc lập; ngày 2026-11-20 (vòng xem lại).
- Đối tượng: HEAD `6824155` (merge `main`/spawn-fixes + fmt), diff `main..HEAD`. Không sửa mã sản phẩm; mọi thí nghiệm chạy ở thư mục temp ngoài worktree.

### Phát hiện 1 (chặn) — đã hết

Chạy lại đúng thí nghiệm tái hiện cũ với `opencode.cmd` giả `@echo off` + `ping -n 40 127.0.0.1` (tiến trình con giữ đầu ghi stdout, không trả EOF), gọi trực tiếp hàm `load_with` công khai mới (`src/opencode_models.rs:99-146`) thay vì bản sao chuỗi cũ:

- `NEW returned after 5.3879s` / `NEW Err: ... timed out after 5 s` (wall-clock 5.70 s) → có hạn chót thật, trả `Err`, không treo.
- Đối chứng, chạy lại chuỗi cũ (spawn → reader → `recv_timeout(5s)` → `kill` → `wait` → `join`) dưới đồng hồ 12 s: `TIMEOUT at 5.0274s; killing child`, `ABOUT_TO_JOIN at 5.0300s`, **không in `JOINED`, vẫn còn sống sau 12 s => HANG** — đúng kết quả review trước.
- Kết luận cơ chế: mọi nhánh timeout/giải mã lỗi (`src/opencode_models.rs:116-124`, `130-139`) gọi `kill_tree` rồi trả `Err`, **không join luồng đọc**; Windows dùng `taskkill /T /F /PID` diệt cả cây (`cmd.exe` + `ping.exe`/`node.exe`) trước khi `child.kill()`/`child.wait()` (chỉ chờ tiến trình trực tiếp, có giới hạn). Nhánh thành công dùng vòng `try_wait()` trong phần thời gian còn lại của cùng một `deadline` tính từ lúc spawn; `reader.join()` chỉ thực hiện sau khi đã nhận EOF và tiến trình thoát, nên không chặn. Không có lời gọi eager nào mới; `cached()` vẫn là `OnceLock` (`src/opencode_models.rs:66-69`).
- Ghi chú nhỏ (không chặn): `load()` thử lần lượt `opencode` rồi `opencode.cmd`, nên trường hợp cả hai cùng treo cho tổng ~10 s; task yêu cầu hạn chót 5 s **tính từ lúc spawn** cho mỗi lần chạy, và đây là đúng như vậy. Trên máy này `opencode` phân giải tới `opencode.ps1` (CreateProcess lỗi ngay), nên đường thực tế chỉ chịu một hạn chót 5 s.

### Phát hiện 2 (nên sửa) — đã hốt đủ phần task yêu cầu

- Bổ sung `load_with_returns_catalog_from_cli_json`, `load_with_bails_within_deadline_when_cli_holds_stdout` (fake CLI giữ stdout; assert `Err` chứa "timed out" và `elapsed <= 8s`) trong `tests/opencode_models_test.rs:216-241`. Đây chính là test sẽ **hỏng/treo nếu bỏ bản sửa**: với chuỗi cũ, `load_with` không bao giờ trả về (xem đối chứng ở trên), nên assert `<= 8s` bị vi phạm.
- Bổ sung `zero_token_claude_and_agy_keep_the_unknown_format_warning` (hồi quy Claude/agy) và `ctx_rotate_triggers_from_catalog_only_window` trong `tests/rotation_test.rs:924-963` (80% của cửa sổ chỉ có trong danh mục → `ctx_pct = 80.0`, trạng thái `pending`).
- Test "chỉ tải một lần/process" không thêm — đúng miễn trừ của task ("chỉ làm nếu không cần hack trạng thái toàn cục"); `OnceLock` vẫn hiện diện và được rà soát bằng mã.

### Phát hiện 3 (nhỏ) — đã hết

`.agents/decisions/opencode-go-quota.md` kết thúc bằng byte `0x0A`; `git diff main HEAD` không còn marker `\ No newline at end of file`.

### Kiểm chứng vòng này

- `cargo test --offline`: **280 passed / 0 failed** qua 28 binary (riêng `opencode_models_test`: 8/8, `rotation_test`: 8/8; timeout test chạy 5.36 s).
- `cargo fmt --all -- --check`: exit 0. `cargo clippy --offline --all-targets -- -D warnings`: exit 0.

## KẾT LUẬN MỚI: ĐẠT

Cả ba phát hiện của review trước đã được xử lý; phát hiện chặn (#1) được tái kiểm nghiệm bằng chính thí nghiệm cũ — `load_with` mới trả lỗi có hạn trong ~5.4 s trong khi chuỗi cũ vẫn treo vô hạn, và test mới phân biệt được sự khác biệt đó. Nghiệm thu.
