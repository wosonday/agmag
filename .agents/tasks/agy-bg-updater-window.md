---
id: agy-bg-updater-window
tier: fast
skills: test-driven-development
branch: fix/agy-bg-updater-window
worktree: D:/Project/agmag.wt/agy-bg-updater-window
---

# Chặn cửa sổ Windows Terminal bật lên mỗi 15 phút do updater nền của agy

## Hiện trạng (Claude Code chẩn đoán 2026-11-20, phiên ngoài herdr)

Người dùng thấy cửa sổ console/Windows Terminal "nháy" lên định kỳ. Đã bắt quả tang bằng watcher (quét cửa sổ `CASCADIA_HOSTING_WINDOW_CLASS`/`ConsoleWindowClass` 50 ms/lần, khi có cửa sổ mới thì chụp mọi tiến trình sinh trong 15 s trước kèm chuỗi cha):

```
16:10:52.148  agy.exe(396)   <- agm.exe(52668) daemon <- herdr-core   "agy --print /usage --output-format json --print-timeout 20s"
16:10:52.617  agy.exe(3676)  <- agy.exe(396)                          "agy.exe --bg-updater --app_data_dir=antigravity-cli --gemini_dir=.gemini"
16:10:53.012  WindowsTerminal.exe -Embedding   ← cửa sổ hiện ra (Windows Terminal là terminal mặc định)
```

Đối chiếu `~/.gemini/antigravity-cli/cli.log` (`auto_updater.go`):

| Thời điểm | agy auto-updater | Cửa sổ |
|---|---|---|
| 14:19:07 | `Spawned background update process` | người dùng báo ~14:19 |
| 15:49:11.76 | `Spawned background update process` | watcher bắt 15:49:13.30 |
| 15:55:53 | `Last check was less than 15 minutes ago, skipping update (fast path)` | **không** có cửa sổ (watcher đang chạy) |
| 16:10:52.62 | `Spawned background update process` | watcher bắt 16:10:53.47 |

Theo dõi liên tục 16:00–17:00: **4/4** cửa sổ (16:10:53, 16:25:54, 16:40:55, 16:55:58) đều có cùng chuỗi `agm daemon → agy --print /usage → agy --bg-updater → agy (cháu)` 1–2 s trước đó; `cli.log` có `Spawned background update process` đúng 4 mốc này. Không có cửa sổ nào khác trong 60 phút.

Kết luận: mỗi lần `AgyQuotaPoller` (`src/agy_quota.rs`, 15 phút/lần, `[policy].agy_quota_poll_min`) gọi `agy`, agy tự kiểm tra cập nhật; nếu lần kiểm cuối > 15 phút thì sinh `agy.exe --bg-updater`, tiến trình này mở console **hiển thị**. Trùng khớp "khi agent chạy" chỉ là ngẫu nhiên theo chu kỳ.

Đã loại trừ (đo trực tiếp, không cửa sổ): lệnh Bash/PowerShell của Claude Code, `opencode serve --service`/`opencode-pty`, `powershell` do `herdr-app` gọi định kỳ, `agy` poll ở lượt fast-path.

## Hướng sửa (chưa kiểm chứng — phải chứng minh trong task này)

Binary agy (`%LOCALAPPDATA%\agy\bin\agy.exe`) chứa chuỗi `AGY_CLI_DISABLE_AUTO_UPDATE` cạnh mã updater → rất có thể là biến môi trường tắt auto-update. **Chưa xác nhận**: thử A/B bị fast-path che (agy không dùng mtime `updater/update_status.json` làm mốc kiểm cuối; chưa tìm ra nơi lưu mốc).

## Yêu cầu

1. **Kiểm chứng biến trước khi sửa code** (ghi bằng chứng vào `.agents/log/agy-bg-updater-window.md`):
   - Tìm nơi agy lưu mốc "last check" (hoặc chờ > 15 phút kể từ dòng `auto_updater` cuối trong `cli.log`), rồi chạy `AGY_CLI_DISABLE_AUTO_UPDATE=1 agy --print /usage --output-format json --print-timeout 20s` và xem log mới trong `~/.gemini/antigravity-cli/log/`: **không** được có `Spawned background update process`. Thử giá trị `1` trước, nếu không ăn thử `true`.
   - Đối chứng: không đặt biến (sau khi mốc lại quá 15 phút) → có `Spawned…`.
   - Nếu biến không có tác dụng: dừng, báo CEO kèm log; phương án dự phòng là tăng `agy_quota_poll_min` hoặc đặt `0` (tắt) — không tự ý chọn.
2. Nếu biến có tác dụng: đặt `AGY_CLI_DISABLE_AUTO_UPDATE` cho **mọi** lần agm gọi agy:
   - `src/agy_quota.rs` (`fetch_usage`, `Command::new(... "agy")` ~dòng 124)
   - `src/ask.rs` (~dòng 119)
   - Gom vào một helper dùng chung (vd `agy_command()` trả `Command` đã đặt env) để lần gọi agy sau này không quên. Chỉ ảnh hưởng tiến trình agy do agm sinh; agy người dùng mở tay vẫn tự cập nhật bình thường.
3. Test (TDD): binary giả qua `AGM_AGY_BIN` in ra giá trị biến môi trường → assert cả đường quota và `agm ask` đều truyền biến.
4. Ghi chú vào `README.md`, `README_vi.md`, `CHANGELOG.md`: agm tắt auto-update của agy cho các lệnh nó tự gọi (lý do: updater mở cửa sổ console trên Windows); cập nhật agy bằng `agy update` hoặc mở agy tay.

## Phạm vi tệp

Được sửa: `src/agy_quota.rs`, `src/ask.rs`, (nếu cần helper) `src/lib.rs` hoặc tệp mới `src/agy_cmd.rs`, `tests/**`, `README.md`, `README_vi.md`, `CHANGELOG.md`, `.agents/log/agy-bg-updater-window.md`.
Không sửa: schema `src/store/**`, `Cargo.toml`, cấu hình agy của người dùng (`~/.gemini/**`). Không đặt biến môi trường toàn hệ thống (`setx`).

## Tiêu chí nghiệm thu

- Bằng chứng A/B ở yêu cầu 1 trong log task.
- Test env cho quota + ask xanh; `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings` đạt.
- Sau merge + `cargo install` + khởi động lại `agm daemon`: qua **ít nhất 2 chu kỳ 15 phút**, `cli.log` không có `Spawned background update process` vào các mốc poll của daemon, và không có cửa sổ Windows Terminal mới (người dùng xác nhận hoặc chạy lại watcher).
