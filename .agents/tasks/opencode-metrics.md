---
id: opencode-metrics
tier: mid
skills: test-driven-development
branch: feat/opencode-metrics
worktree: D:/Project/agmag.wt/opencode-metrics
---

# Bổ sung số liệu OpenCode trong `agm`

## Hiện trạng (Claude CEO kiểm ngày 2026-11-20)

- **Token theo model: đã đúng, không sửa.** `agm cost --by model --window 4months` khớp từng token với tổng `session_message` (type `assistant`) trong `~/.local/share/opencode/opencode.db` cho mọi khóa `providerID/id` (vd `openai/gpt-6.1-sol` 124 436 683, `opencode/beam-free` 32 910 490, `opencode-go/kimi-k3` 12 107).
- Người dùng vừa đặt model mặc định **global** của OpenCode (`~/.config/opencode/opencode.json`) là `opencode-go/kimi-k3`, nên từ nay phần lớn phiên OpenCode sẽ chạy Go. Ba khoảng trống dưới đây vì thế quan trọng hơn.

### Khoảng trống 1 — `ctx %` trống với gần hết phiên OpenCode

- `src/opencode.rs:138` ghi `ContextSnapshot { window_tokens: None }`. `src/report.rs:181` và `src/policy.rs:76` chỉ lấy cửa sổ từ `cfg.model(m).context_window` (`~/.agmag/providers.toml`).
- Kết quả: `agm ls` cho `context_pct: None` với `opencode/iceman-free`, `opencode/hy4-flash-free`, `opencode/flint-40b-free`, `opencode/beam-free`, `openai/gpt-6.2-sol`, `opencode-go/kimi-k3`… Chỉ `openai/gpt-6.1-sol` có số (nhờ `[models."gpt-6.1-sol"] context_window`). Hệ quả: TUI cột `ctx` trống và **`ctx_warn`/`ctx_rotate` không bao giờ kích hoạt** cho thợ OpenCode.
- Nguồn cửa sổ ngữ cảnh khả dĩ (phải tự kiểm, không đoán): danh mục model của OpenCode — `opencode models --verbose`, hoặc tệp cache `~/.cache/opencode/models.json` (dạng `{providerID: {models: {id: {limit: {context, output}}}}}`; **tệp này đang cũ**, chưa có `deepseek-v4.1-flash`, `iceman-free`… nên không được là nguồn duy nhất).

### Khoảng trống 2 — cảnh báo giả `unknown format?`

- `src/report.rs:185`: phiên OpenCode chỉ có message assistant lỗi/0 token (vd `ses_e096ae37fffeU2H3mwQ4haxA95`, `anthropic/claude-opus-5-5`, provider chưa đăng nhập) bị gắn `warning: "unknown format?"`. Đây không phải lỗi định dạng.

### Khoảng trống 3 — hạn mức Go (5 giờ / tuần / tháng) chưa có trong `agm quota`

- Hiện chỉ kiểm được trên [Go Console](https://opencode.ai/console/go). Chưa biết có nguồn máy đọc được, 0 token hay không (`opencode stats --json` chỉ có token/cost/tool, **không** có hạn mức; `opencode api <operation>` chưa được khảo sát).

## Yêu cầu

1. **Cửa sổ ngữ cảnh OpenCode.** Thêm tra cứu `context_window` cho model OpenCode theo thứ tự ưu tiên: (a) `providers.toml` `[models."<providerID/id>"]`, rồi `[models."<id>"]` như hiện nay — cấu hình người dùng luôn thắng; (b) danh mục model của OpenCode (nguồn đã kiểm ở trên, đọc chỉ-đọc, có cache trong tiến trình, không gọi mạng mỗi vòng daemon); (c) không có → để `None` như cũ, **không** đoán số mặc định. Áp dụng cho cả `report.rs` (ctx % của `ls`/TUI) và `policy.rs` (`ctx_warn`/`ctx_rotate`).
2. **Bỏ cảnh báo giả.** Phiên OpenCode có message nhưng tổng token = 0 không được gắn `unknown format?`. Giữ cảnh báo cho các nguồn khác (Codex/Claude/agy) đúng như hiện tại.
3. **Khảo sát hạn mức Go — chỉ đọc, không cài đặt.** Viết `.agents/decisions/opencode-go-quota.md` theo mẫu `.agents/decisions/agy-quota-usage.md`: liệt kê các nguồn đã thử (lệnh chính xác, thời gian chạy, số token tiêu tốn, trích JSON đã xóa thông tin định danh), kết luận có/không có nguồn 5h/tuần/tháng đáng tin, và đề xuất ánh xạ vào bảng `quota` (provider `opencode-go`, cửa sổ 300 / 10080 / tháng) nếu có. **Không** cào Console bằng cookie/phiên trình duyệt, **không** đọc `auth.json`, bảng `credential`/`account`/`control_account` của `opencode.db`.
4. Cập nhật `README.md` và `README_vi.md` (mục OpenCode / ctx) cho thay đổi 1–2. `docs/` chỉ tiếng Anh; không sửa `docs/` trong task này.

## Phạm vi tệp

Được sửa: `src/opencode.rs`, `src/report.rs`, `src/policy.rs`, `src/config.rs` (nếu cần hàm tra cứu), tệp mới `src/opencode_models.rs` (nếu tách), `src/lib.rs` (khai báo module), `tests/**`, `README.md`, `README_vi.md`, `CHANGELOG.md`, `.agents/log/opencode-metrics.md`, `.agents/decisions/opencode-go-quota.md`.
Không sửa: schema SQLite (`src/store/**`) trừ khi bắt buộc — nếu cần, dừng và hỏi; `Cargo.toml` (không thêm crate).

## Tiêu chí nghiệm thu

- Test: model có `context_window` trong `providers.toml` (khóa đầy đủ và khóa trần) thắng danh mục OpenCode; model chỉ có trong danh mục → ctx % đúng; model không có ở đâu → `None`, không panic. Fixture danh mục tự tạo trong test (không đọc tệp thật của người dùng).
- Test: `policy` kích hoạt `ctx_warn` cho phiên OpenCode chỉ có cửa sổ từ danh mục.
- Test: phiên OpenCode toàn 0 token không có `warning`; hồi quy cảnh báo của Codex/Claude không đổi.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- Chạy thật: `cargo run -- ls --json` → các phiên `opencode/*`, `opencode-go/*` có `context_pct` khi danh mục biết model; ghi lệnh + trích kết quả vào `.agents/log/opencode-metrics.md`.
- Có `.agents/decisions/opencode-go-quota.md` với kết luận rõ ràng.

## Báo cáo

Xong: `agm send t4v --type "BÁO CÁO" --task opencode-metrics "Xong: <SHA cuối>, xem .agents/log/opencode-metrics.md"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task opencode-metrics "<câu hỏi ngắn>"`.

## Vòng sửa 1 (sau review `f4ed2fe`, CEO đã xác minh #1)

Nguồn: `.agents/reviews/opencode-metrics-review.md`.

1. **Chặn (#1) — `load_with` phải có hạn chót cho TOÀN BỘ thao tác.** Hiện nhánh timeout gọi `reader.join()` không giới hạn (`src/opencode_models.rs` ~126), nhánh thành công `child.wait()` cũng không giới hạn. Trên Windows `opencode` có thể là `opencode.cmd` → `node.exe`, hoặc khởi động dịch vụ nền kế thừa stdout; kill tiến trình trực tiếp không đóng pipe → treo vĩnh viễn.
   - Một hạn chót chung (5 s) tính từ lúc spawn cho: chờ dữ liệu + chờ thoát.
   - Hết hạn: diệt **cả cây** tiến trình (Windows: `taskkill /T /F /PID <pid>` qua `std::process::Command`; nơi khác: `child.kill()`), **không `join` luồng đọc** (để nó tự kết thúc/bị bỏ), trả `Err`.
   - Nhánh thành công: chờ thoát bằng vòng `try_wait()` trong phần thời gian còn lại; quá hạn thì xử lý như trên.
   - Không thêm crate.
2. **Test cho #1 (bắt buộc).** Cho phép chỉ định binary trong test (vd biến môi trường `AGM_OPENCODE_BIN`, hoặc hàm `load_with` công khai). Fake treo mà tiến trình con giữ stdout: Windows `.cmd` có `ping -n 30 127.0.0.1` (`#[cfg(windows)]`); Unix `sh` script `sleep 30` (`#[cfg(unix)]`). Kiểm: trả `Err` trong ≤ 8 s. Thêm fake trả JSON hợp lệ → `Ok`.
3. **Nên sửa (#2).** Thêm test: `ctx_rotate` kích hoạt với phiên OpenCode chỉ có cửa sổ từ danh mục; hồi quy cảnh báo `unknown format?` cho Claude và agy (không chỉ Codex). Test "chỉ tải một lần" chỉ làm nếu không cần hack trạng thái toàn cục.
4. **Nhỏ (#3).** Thêm xuống dòng cuối `.agents/decisions/opencode-go-quota.md`.

Nghiệm thu: `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings` sạch; `rustfmt --edition 2024 --check` sạch **trên các tệp bạn sửa** (không fmt cả cây). Cập nhật `.agents/log/opencode-metrics.md`.
Báo cáo: `agm send user --type "BÁO CÁO" --task opencode-metrics "Vòng sửa 1 xong: <SHA>"`.
