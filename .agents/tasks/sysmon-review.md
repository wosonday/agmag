---
id: sysmon-review
tier: mid
branch: feat/sysmon
worktree: D:/Project/agmag.wt/sysmon
---

# Review độc lập — `feat/sysmon` (`agm sys`, theo dõi RAM/CPU/ổ đĩa theo agent)

## Bối cảnh

- Tác giả: lane ceo-pc (Claude). Nhật ký: `.agents/log/sysmon.md`.
- CEO đã đặt lại nhánh lên `main` `0385149` (commit `003d024`): chỉ áp phần sửa thật, CHANGELOG về LF và chuyển lên `[Unreleased]`, dời `strip_sgr`/`print_sys` lên trên `mod tests` trong `src/main.rs`. Bản gốc: `feat/sysmon-orig` (`9de7b5c`) — chỉ để đối chiếu, không sửa.
- CEO đã chạy: `cargo test --offline` 308 đạt/0 lỗi, clippy `-D warnings` sạch, `cargo fmt --check` sạch.
- Phạm vi review: `git diff main HEAD` (≈ 12 tệp, +1500 dòng).

## Mục kiểm (mỗi mục: ghi phát hiện kèm `tệp:dòng` + cách tái hiện, rồi commit)

1. **Phụ thuộc mới** `sysinfo 0.40.4` (`Cargo.toml`, `Cargo.lock`): chỉ bật `system`, `disk`; kéo thêm bao nhiêu crate (đếm trong `Cargo.lock`); có crate nào trùng phiên bản lớn với crate sẵn có; `cargo build --offline` không cần mạng. Đánh giá: có cách đọc được số liệu không cần crate không (chỉ ghi nhận, không bắt buộc đổi).
2. **Đúng số liệu** (`src/sysmon.rs`): % CPU (mốc baseline lần refresh thứ hai trong `SysSampler::new`), RAM/swap, ổ đĩa (`merge_disks` gộp cùng một volume nhiều ký tự ổ — không gộp nhầm hai ổ khác nhau cùng dung lượng?), tốc độ đọc/ghi (chia cho đúng khoảng thời gian?), chia RAM/CPU theo **cây tiến trình** (`attribute`: tiến trình con có bị đếm hai lần nếu cây lồng nhau, ví dụ opencode chạy dưới claude-code?). Nhận diện agent theo tên tiến trình (`agent_of`): khớp nhầm tiến trình không phải agent không?
3. **Chi phí/treo**: `agm sys` và TUI có làm chậm vòng vẽ không (refresh toàn bộ tiến trình mỗi lần vẽ? có `sleep` chặn luồng TUI?). Không có lệnh ngoài nào chạy không có hạn chót.
4. **Hiển thị**: `agm sys` (màu, `NO_COLOR`, `--json`), TUI header `cpu · ram · ổ đầy nhất`, bảng `system` qua `view`; kiểm ở `COLUMNS=60` và `110` không tràn; số nhân lớn (vd 64) không vỡ lưới/spark. Ký tự ngoài dữ liệu (tên ổ, tên tiến trình) có đi qua bộ lọc CSI/OSC sẵn có không.
5. **Không đụng CSDL**: không migration, không ghi `~/.agmag` (đúng như nhật ký nói).
6. **Ghép với `main`**: `src/main.rs`/`src/dashboard.rs`/`src/tui.rs` — các lệnh/section mới không phá `quota`, `view` mặc định, `opencode-go` rows. Đối chiếu nhanh với `feat/sysmon-orig` xem có phần nào bị rơi khi áp lại.
7. **Docs**: README.md, README_vi.md, CHANGELOG (`[Unreleased]`) khớp mã.
8. Chạy lại `cargo test --offline` và `cargo clippy --offline --all-targets -- -D warnings`; ghi số đạt/lỗi.

## Quy tắc

- **Không sửa mã sản phẩm.** Chỉ tạo/sửa `.agents/reviews/sysmon-review.md`.
- Tạo tệp review NGAY từ đầu (danh sách mục kiểm), mỗi mục kiểm xong thì ghi phát hiện và **commit ngay** (tiền tố `sysmon-review:`), để người thay thế làm tiếp được nếu model bị chặn.
- Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`. Kết luận cuối: **ĐẠT** (không có `chặn`, `nên sửa` có thể làm sau) hoặc **CHƯA ĐẠT**.
- Không chạy `agm tui` (cần TTY). Chạy thử bằng `cargo run --offline -- sys` và `-- sys --json`; dùng `AGM_HOME` tạm nếu lệnh cần CSDL.
- Không đọc `auth.json`, `opencode.db`, tệp `.env`.

## Báo cáo

Xong: `agm send user --type "BÁO CÁO" --task sysmon-review "ĐẠT|CHƯA ĐẠT: <SHA>, <số chặn>/<số nên sửa>/<số nhỏ>"`.
