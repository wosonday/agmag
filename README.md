# agmag — bảng điều khiển model cho herdr / herdr-core

`agm` đứng cạnh herdr: đo token và hạn mức gói tháng của Claude, Codex, OpenCode, agy; theo dõi các terminal của herdr-core; gom câu hỏi của agent vào một hộp thư; ghi lại tin nhắn giữa agent; sinh agent có hạn mức; áp ngân sách; tự xoay vòng thợ OpenCode khi ngữ cảnh đầy.

Thiết kế: [`docs/superpowers/specs/2026-11-18-agmag-design.md`](docs/superpowers/specs/2026-11-18-agmag-design.md) · Kết quả xác minh: [`.agents/decisions/p0-research.md`](.agents/decisions/p0-research.md)

## Cài

```powershell
cargo build --release          # target\release\agm.exe — chép vào một thư mục trong PATH
agm hook install               # (tùy chọn) cho Claude Code gửi hạn mức 5h/7d + % ngữ cảnh cho agm
```

Dữ liệu nằm ở `~/.agmag/` (`agm.db`, `providers.toml`). Đổi bằng biến `AGM_HOME`.

## Dùng hằng ngày

| Lệnh | Việc |
|---|---|
| `agm quota` | Hạn mức gói theo cửa sổ (Codex: đọc từ log; Claude: cần `agm hook install`) |
| `agm ls` · `agm cost --by model\|day\|source\|cwd` | Phiên hôm nay, token, % ngữ cảnh; tổng token (USD chỉ cho model trả theo API) |
| `agm log <session>` | Tóm tắt lượt hội thoại (`--full`: dòng log gốc) |
| `agm daemon` | Chạy nền (để trong một terminal của herdr-core): sự kiện hc, quét log, ghép terminal↔phiên, ngân sách, xoay vòng |
| `agm tui` | Bảng điều khiển trong một pane (chỉ người dùng ở terminal); lệnh `r <#> <trả lời>`, `d <#>`, `s <t> <LOẠI> <tin>`, `i <t>`, `k <t>`, `p on\|off`, `q` |
| `agm ps` · `agm inbox` · `agm reply <#> <text>` · `agm done <#>` | Terminal đang sống; hộp thư (agent bị chặn, câu hỏi, đề nghị, cảnh báo); trả lời câu hỏi của agent; đóng mục |
| `agm send <đích> --type GIAO --task <ID> <tin>` | Gửi tin theo mẫu (GIAO, BÁO CÁO, LỖI, HỎI, CHẶN, ĐẠT, CHƯA ĐẠT); đích `user` → hộp thư |
| `agm spawn <template> --task <ID> [--cwd …] -- <nhiệm vụ>` | Sinh agent qua `hc run --bg --outlive-parent`, kiểm `max_workers` và ngân sách |
| `agm kill\|interrupt <t>` | Codex/Claude (và app chưa rõ): **chỉ người dùng**, luôn phải gõ `y` (`--yes` không áp dụng); agent và policy chỉ dừng được thợ OpenCode |
| `agm rotate <t>` | Xoay vòng một thợ OpenCode ngay (không chờ 70%) |
| `agm budget set task:<ID> --soft 2000000 --hard 5000000 [--action notify\|interrupt\|kill]` | Ngân sách token cho đầu việc |
| `agm budget set provider:codex:10080 --soft 80 --hard 95` | Ngân sách % hạn mức gói (cửa sổ tính bằng phút) |
| `agm policy off` | Nút dừng khẩn: tắt mọi hành động tự động |
| `agm ask [--model gemini-4-argon-high] <câu hỏi>` | Hỏi agy một lần (chỉ đọc), ghi lại token |
| `agm link <t> <session>` | Ghép tay terminal ↔ phiên khi agm không chắc |

Cho agent dùng: các lệnh in JSON khi đầu ra không phải terminal (hoặc `--json`). Agent gọi `agm send`/`agm spawn` bên trong terminal của hc sẽ được nhận diện qua `HC_TERMINAL_ID`.

## Cấu hình `~/.agmag/providers.toml`

```toml
[providers.codex]
billing = "subscription"        # gói tháng: đo bằng % hạn mức, không quy ra USD
[providers.openrouter]
billing = "api"

[models."gpt-6.1-sol"]
provider = "codex"              # OpenCode dùng openai/gpt-6.1-sol cũng tiêu hạn mức ChatGPT
context_window = 475000
[models."some-api-model"]
provider = "openrouter"
price = { input = 3.0, cache_read = 0.3, cache_write = 3.75, output = 15.0 }   # USD / 1M token

[policy]
max_workers = 3
manager = "codex-mgr"           # nhận cảnh báo ngân sách; bỏ trống = chỉ hộp thư
ctx_warn = 50                   # % ngữ cảnh: cảnh báo
ctx_rotate = 70                 # % ngữ cảnh: xoay vòng thợ OpenCode
handoff_timeout_min = 10
```

## Xoay vòng thợ OpenCode (§5.9)

Thợ (`agm.role=worker`, app `opencode`) đầy ≥ 70% ngữ cảnh → khi rảnh, agm yêu cầu ghi bàn giao vào `.agents/log/<ID>.md` và commit → khi agent xong lượt, agm kiểm **worktree sạch** và **có commit mới chạm tệp đó** → kill phiên cũ → mở phiên mới cùng template/model/thư mục với nhiệm vụ "đọc tasks/log/git log rồi làm tiếp". Bàn giao không đạt hoặc quá `handoff_timeout_min` → **không kill**, báo vào hộp thư. Codex và Claude không bao giờ bị xoay tự động.

## An toàn (quy tắc của người dùng)

- **Codex và Claude không bao giờ bị dừng tự động.** Agent hoặc policy chỉ dừng được terminal mà `hc get` xác nhận là OpenCode (danh sách trắng); app chưa rõ → từ chối và chuyển đề nghị vào hộp thư.
- Người gọi được xác định *fail-closed*: có `HC_TERMINAL_ID` mà không đọc được terminal, hoặc terminal là app agent → coi là agent. Không có TTY → agent.
- `agm policy off` (nút dừng khẩn) chạy được cả khi `providers.toml` hỏng.
- Xoay vòng kiểm mọi điều kiện mở phiên mới **trước** khi kill; bàn giao chưa xong thì chờ tới `handoff_timeout_min`, quá hạn thì báo và giữ phiên cũ.

## Giới hạn đã biết (v1)

- Agent chạy trong một terminal có TTY, tự xóa `HC_TERMINAL_ID` rồi tự gõ `y`, về lý thuyết vẫn được coi là người dùng. agm không kiểm cây tiến trình cha.
- Ngân sách theo provider (`provider:<tên>:<phút>`) chỉ cảnh báo; chưa chặn `spawn` vì agm chưa biết template dùng provider nào.

- Phiên chạy trong HERDR TUI được đo token (qua log), nhưng chỉ terminal của herdr-core mới can thiệp được.
- Token của agy chỉ đo được qua `agm ask` (agy tương tác không lưu token trên máy).
- Hạn mức ChatGPT do OpenCode tiêu không có trong log; agm thấy nó gián tiếp qua lần đọc `rate_limits` kế tiếp của Codex.
- `agm tui` là bảng vẽ lại theo chu kỳ + dòng lệnh (máy dựng v1 không có mạng để thêm thư viện TUI).

## Phát triển

```powershell
cargo test --offline
python -I tools/argon_review.py          # nhờ Gemini Argon (agy) review theo nhóm mô-đun
python -I tools/sanitize_fixture.py <log.jsonl> <fixture.jsonl>   # làm sạch log thật thành fixture (danh sách trắng)
```
