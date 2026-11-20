# `agm tui`: mã loại tin tiếng Anh trong dòng `cmd:`

## Hiện trạng

- `src/dashboard.rs`: dòng `cmd:` chỉ ghi `s <t> <TYPE> <msg>`. `parse_command()` và hướng dẫn khi nhập sai dùng danh sách `KINDS` tiếng Việt.
- `src/tui.rs`: lệnh `s` gọi `actions::send()`. Hàm này cũng phục vụ CLI `agm send`, kiểm tra loại tin rồi dùng mã đó khi lưu và định dạng tin gửi đi.
- `README.md` và `README_vi.md` vẫn dùng `GIAO` trong ví dụ TUI. Các hướng dẫn cho agent cũng có lệnh CLI dùng mã tiếng Việt.

## Yêu cầu

1. Trong `agm tui`, lệnh `s <đích> <TYPE> <nội dung>` nhận các mã tiếng Anh sau. Hiển thị mã bằng chữ hoa trong hướng dẫn; chấp nhận cả chữ thường hoặc viết xen hoa/thường khi nhập mã tiếng Anh.

   | Mã tiếng Anh | Mã tương ứng hiện tại |
   | --- | --- |
   | `ASSIGN` | `GIAO` |
   | `REPORT` | `BÁO CÁO` |
   | `ERROR` | `LỖI` |
   | `QUESTION` | `HỎI` |
   | `BLOCKED` | `CHẶN` |
   | `PASS` | `ĐẠT` |
   | `FAIL` | `CHƯA ĐẠT` |

2. Hiển thị đủ bảy mã tiếng Anh trong khu vực `cmd:` hoặc phần hướng dẫn ngay dưới đó. Giữ cách xuống dòng theo độ rộng pane; không để mất mã ở độ rộng tối thiểu 60 cột. Khi thiếu đích, thiếu nội dung hoặc mã không hợp lệ, báo lỗi bằng tiếng Anh và không gửi tin. Không chấp nhận tiền tố như `REPORTING` thay cho `REPORT`.
3. Tiếp tục chấp nhận cả bảy mã tiếng Việt cũ trong TUI, bao gồm hai mã nhiều từ. Mã mới và mã cũ tương ứng phải cho cùng kết quả gửi. Chỉ đổi cách nhập và hướng dẫn của TUI: không đổi mã ghi trong SQLite, định dạng tin chuyển đến terminal/hộp thư, cách hiển thị lịch sử hay hợp đồng `agm send --type`. Chỉ chuẩn hóa loại tin, không dịch hoặc đổi chữ hoa/thường trong nội dung.
4. Cập nhật mục lệnh TUI trong `README.md` và `README_vi.md`: dùng ví dụ tiếng Anh như `s t5 REPORT All tests passed` và giải thích mã tiếng Việt cũ vẫn dùng được. Giữ nguyên ví dụ dành cho CLI `agm send`.

## Tiêu chí nghiệm thu

- Kiểm thử parser cho đủ bảy cặp mã; `s user QUESTION Cần xác nhận?` tương đương `s user HỎI Cần xác nhận?`, và `s t5 report Đã chạy test` tương đương `s t5 BÁO CÁO Đã chạy test`.
- Kiểm thử từ chối mã lạ, tiền tố và trường hợp thiếu đích hoặc nội dung; nội dung Unicode và chữ hoa/thường vẫn giữ nguyên.
- Kiểm thử phần `cmd:` ở độ rộng 60 và 110 cột: đủ bảy mã tiếng Anh, không tràn khung. Các lệnh `r`, `d`, `c`, `i`, `k`, `p`, `q` không đổi.
- Kiểm thử hồi quy `agm send --type HỎI` và `agm send --type "BÁO CÁO"`; các tin gửi qua mã tiếng Anh trong TUI vẫn dùng mã tương ứng hiện tại khi lưu/chuyển tiếp.
