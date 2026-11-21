---
id: inbox-done-bulk
tier: mid
skills: test-driven-development
branch: feat/inbox-done-bulk
worktree: D:/Project/agmag.wt/inbox-done-bulk
---

# `agm done` chỉ đóng được 1 mục — thêm đóng hàng loạt (CLI + TUI)

## Hiện trạng (Claude CEO kiểm 2026-11-21)

- Inbox chỉ tự đóng mục `blocked` (daemon), và `context`/`rotation` khi `agm cleanup`. Mục `message` (`BÁO CÁO`, `HỎI`, `ĐẠT`…) **không bao giờ tự đóng** → hôm nay có 42 mục mở, phải chạy `agm done` 36 lần.
- CLI: `Cmd::Done { id: i64 }` (`src/main.rs`) → `actions::dismiss(store, id, now)` (`src/actions.rs`, lỗi nếu id không tồn tại). JSON: `{"done": <id>}`.
- TUI: `TuiCmd::Done(i64)`, parser `"done" => rest.parse()` (`src/dashboard.rs`), xử lý ở `src/tui.rs`; bảng lệnh `("done", "d", "<#>")`.
- Alias loại tin tiếng Anh đã có: `src/actions.rs` (`("ASSIGN","GIAO")`, `("REPORT","BÁO CÁO")`, …) — **dùng lại**, không chép bảng mới.

## Quyết định của CEO (thiết kế)

1. **Chọn theo ID**: `agm done <IDS>...` — mỗi đối số là `N`, `N-M` (khoảng, gồm 2 đầu), hoặc danh sách ngăn bởi dấu phẩy `N,M-K`. Khoảng ngược (`9-3`) hoặc khoảng > 10 000 mục → lỗi rõ ràng.
2. **Chọn theo bộ lọc** (không cần ID): `--kind <KIND>` (nhận cả mã tiếng Việt và alias tiếng Anh qua bảng sẵn có; so khớp không phân biệt hoa thường với alias), `--terminal <addr>`, `--source <message|context|rotation|blocked>`. Nhiều bộ lọc = AND. Kết hợp ID + bộ lọc = chỉ các ID trong danh sách **và** khớp bộ lọc.
3. **An toàn:**
   - Phải có ít nhất một ID hoặc một bộ lọc; `agm done` trống → lỗi usage.
   - Mục `source = blocked` (agent đang chờ duyệt) **không bao giờ** bị đóng bởi khoảng ID hay bộ lọc, trừ khi người dùng ghi rõ `--source blocked` hoặc ghi đúng ID đơn lẻ của nó. Lý do: đóng nhầm làm khuất một agent đang chờ.
   - `done` vẫn **không gõ gì vào terminal nào** (chỉ cập nhật DB) — giữ nguyên.
4. **Kết quả:**
   - Một ID đơn (hành vi cũ): giữ nguyên — lỗi nếu không tồn tại, JSON `{"done": <id>}`, text `Dismissed #<id>.` (tương thích script).
   - Hàng loạt: bỏ qua ID không tồn tại / đã đóng / bị loại bởi quy tắc `blocked`, **không lỗi**. Text: `Dismissed N item(s): #a, #b…` + dòng `Skipped M: …` nếu có (nêu lý do gọn: `missing`, `already done`, `blocked (use --source blocked)`). JSON: `{"done":[…],"skipped":[{"id":…,"reason":"…"}]}`.
   - Không khớp gì → `Nothing to dismiss.`, exit 0.
5. **Đóng trong 1 transaction** SQLite (thêm hàm store mới; `resolve_inbox` cũ giữ nguyên).
6. **TUI**: `done 150-179`, `done 150,151 160-162`, `d 5` (cũ) đều chạy; thêm dạng lọc `done kind:REPORT` / `done kind:"BÁO CÁO"` / `done term:t4w` (có thể kết hợp với ID). Thông báo: `dismissed N (skipped M)`. Cập nhật mô tả cú pháp trong bảng lệnh TUI (`"<#|a-b|kind:K|term:T>"` hoặc tương đương ngắn).
7. Không thêm tự đóng theo thời gian, không đổi daemon.

## Phạm vi tệp

Được sửa: `src/main.rs` (Cmd::Done + help tiếng Anh), `src/actions.rs`, `src/store/v2.rs` (thêm hàm, không đổi schema), `src/dashboard.rs` (parser TUI + bảng lệnh), `src/tui.rs` (xử lý lệnh), tests liên quan (`tests/cli_test.rs`, test TUI/parser hiện có — tìm file test đang test `parse` lệnh TUI), `README.md`, `README_vi.md` (các dòng `done <#>`/`agm done <#>` ở mục TUI và bảng lệnh), `CHANGELOG.md` (`[Unreleased]` → `### Thêm mới`, giữ LF), `.agents/log/inbox-done-bulk.md`.
Không sửa: schema SQL, daemon, các module khác. Chỉ `cargo fmt` tệp bạn sửa.

## Tiêu chí nghiệm thu (test trước — đỏ — rồi mới code)

- Parser: `"5"`, `"3-7"`, `"1,4-6"`, nhiều đối số; lỗi với `"9-3"`, `"a"`, `"1-"`, khoảng quá lớn.
- Store/actions (dùng `AGM_HOME` tạm): tạo mục `message/BÁO CÁO`, `message/HỎI`, `blocked/…`, `context/warn` ở vài terminal:
  - khoảng ID đóng đúng các mục mở, bỏ qua `blocked` (lý do ghi đúng), bỏ qua đã đóng/không tồn tại;
  - `--kind REPORT` và `--kind "BÁO CÁO"` cho cùng kết quả; `--terminal` + `--kind` là AND;
  - `--source blocked` đóng được mục blocked; ID đơn của mục blocked đóng được;
  - `agm done` không đối số → exit ≠ 0; ID đơn không tồn tại → exit ≠ 0 (như cũ); JSON ID đơn giữ `{"done": id}`.
- Test help tiếng Anh `cli_help_is_english_for_every_command` vẫn xanh.
- `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- **Commit sau mỗi bước** (tiền tố `inbox-done-bulk:`), ghi `.agents/log/inbox-done-bulk.md`. Dùng `AGM_HOME` tạm khi chạy thử; **không** chạy `agm done` trên inbox thật.

## Báo cáo

Xong: `agm send user --type "BÁO CÁO" --task inbox-done-bulk "Xong: <SHA>"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task inbox-done-bulk "<câu hỏi ngắn>"`.
