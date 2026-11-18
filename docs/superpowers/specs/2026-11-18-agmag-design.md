# agmag — Bảng điều khiển model cho herdr / herdr-core

**Ngày:** 2026-11-18 · **Trạng thái:** bản thiết kế, chờ duyệt
**Binary:** `agm` (Rust) · **Thư mục:** `D:\Project\agmag`

## 1. Mục đích và lý do

Khi chạy đội agent theo `AGENTS-TUTORIAL.md` (Claude Code, Codex, OpenCode, agy trong herdr / herdr-core), người dùng hiện **không thấy rõ**:

- mỗi agent đang tiêu bao nhiêu token, ở model nào, tốn bao nhiêu tiền;
- các agent đã nói gì với nhau (tin `hc send` biến mất sau khi gõ vào pane);
- agent nào đang chờ người (hỏi quyền, hỏi câu hỏi) mà người dùng chưa để ý.

Và **không có chỗ can thiệp tập trung**: muốn ngắt, gửi tin, chặn sinh thêm agent khi vượt ngân sách thì phải gõ `hc` tay.

`agmag` là công cụ **gắn kèm herdr**: quan sát mọi phiên, ghi token + hội thoại + tin nhắn giữa agent vào một CSDL cục bộ, hiển thị trong một pane TUI và CLI, và can thiệp qua API của `hc`. Mục tiêu cuối: **minh bạch các tầng giao tiếp giữa model và tiết kiệm chi phí cho model đắt tiền**.

## 2. Mục tiêu / không mục tiêu / phản mục tiêu

**Mục tiêu (v1)**

1. Theo dõi token theo terminal, phiên, model, đầu việc (`task`), ngày; quy ra chi phí theo bảng giá người dùng sửa được.
2. Xem hội thoại của từng phiên (lượt người/agent, tóm tắt lệnh gọi công cụ) từ log của chính CLI.
3. Ghi lại mọi tin nhắn giữa agent đi qua `agm send` (ai gửi, ai nhận, lúc nào, trạng thái đích, đã giao chưa).
4. Sinh agent qua `agm spawn` có kiểm hạn mức: số thợ đồng thời, ngân sách token/chi phí theo đầu việc.
5. Ngân sách: cảnh báo mềm (báo quản lý + nổi lên TUI) và chặn cứng (ngắt agent, từ chối spawn mới) — hành động cứng cấu hình được.
6. Hộp thư người dùng: agent ở trạng thái `blocked` (permission/question/auth) hoặc gửi tin loại `HỎI`/`CHẶN` cho `user` hiện ở một danh sách riêng trong TUI; người dùng trả lời bằng `agm reply`.
7. TUI chạy trong một pane của herdr; CLI mọi lệnh in JSON khi đi qua đường ống (theo quy ước của `hc`).

**Không mục tiêu (v1)**

- Không đứng giữa agent và API (không proxy, không chặn lời gọi trước khi tốn token). Đã cân nhắc và để lại cho sau (mục 4).
- Không web dashboard (để sau; CSDL đã đủ để thêm).
- Không điều khiển phiên chạy trong HERDR TUI qua `herdr` CLI (core và TUI khác backend — `AGENTS-TUTORIAL.md` §4.3). v1 **đọc log** được của mọi phiên (log nằm ở thư mục của từng CLI, không phụ thuộc herdr), nhưng **chỉ can thiệp** được terminal của core.
- Không tự động đổi model của agent. v1 chỉ **gợi ý** hạ/nâng bậc.
- Không thay thế `.agents/` và git làm nguồn sự thật cho đầu việc; agmag chỉ đọc trường `budget:`/`tier:` từ tệp tác vụ.

**Phản mục tiêu (dù chạy đúng vẫn coi là thất bại)**

- Làm hỏng hoặc chậm agent: agmag không bao giờ ghi vào thư mục log của CLI, không giữ khóa tệp log, không gõ vào pane trừ khi được lệnh hoặc quy tắc ngân sách cứng.
- Số liệu token sai mà trông như đúng: khi bộ đọc không nhận ra định dạng, phải báo "không đọc được" chứ không âm thầm bỏ qua hoặc đếm 0.
- Lưu bí mật: không lưu `auth.json`, khóa API, biến môi trường; nội dung hội thoại chỉ ở máy cục bộ.
- Can thiệp bất ngờ: mọi hành động agmag tự làm (ngắt, từ chối, gửi cảnh báo) đều ghi vào bảng `actions` và hiện trên TUI.

## 3. Ràng buộc

- Windows 11 là nền chính (đường dẫn `C:\Users\…`, PowerShell); không giả định bash/jq/systemd. Code phải chạy được trên Linux/macOS nhưng không bắt buộc kiểm v1.
- Phiên bản trên máy: `hc 0.1.21`, `herdr 0.9.5`, `codex-cli 0.170.0`, `agy 1.5.10`, Claude Code hiện hành, OpenCode v2.
- Định dạng log của Codex / Claude Code / OpenCode / agy là **nội bộ, có thể đổi mà không báo**. Bộ đọc phải tách riêng từng nguồn, có phiên bản, có test bằng mẫu log thật đã làm sạch.
- `hc` có API JSON-RPC (`hc connect` qua stdin/stdout; `hc schema` ra OpenRPC). Dùng API này, **không** phân tích văn bản in ra của CLI.
- Tên terminal: 1–32 ký tự `a-z 0-9 - _`.
- Agent sinh từ `hc run` mặc định chết theo cha → `agm spawn` phải dùng `outlive_parent` cho thợ.
- Ngôn ngữ: Rust cho binary chính. Phần phụ, không đáng kể (script làm sạch mẫu log, giải mã thử định dạng agy, công cụ một lần) được viết Python.

## 4. Các phương án đã xét

**Tầng can thiệp**

| Phương án | Đánh đổi | Kết quả |
|---|---|---|
| A. Quan sát (log + `hc events`) rồi điều khiển qua `hc` | Nhẹ, không phá CLI, không đụng OAuth; chỉ phản ứng **sau** khi token đã tiêu | **Chọn** |
| B. Proxy giữa agent và API | Chặn/hạ model/nén ngữ cảnh **trước** khi tiêu; nặng, dễ vỡ với OAuth của Codex/Claude, phải theo kịp từng API | Để sau; thiết kế CSDL không cản trở thêm |

**Giao diện**

| Phương án | Kết quả |
|---|---|
| Pane TUI trong herdr + CLI | **Chọn** — nằm ngay trong herdr, dùng được qua Moshi/SSH, agent gọi được CLI |
| Web dashboard localhost | Để sau |
| Chỉ CLI | Không đủ cho việc "nhìn rõ" |

**Ngôn ngữ**

| Phương án | Kết quả |
|---|---|
| Rust (ratatui + rusqlite + notify) | **Chọn** — một tệp exe, tiến trình nền nhẹ, cùng hệ sinh thái herdr/hc |
| Python (Textual) | Chỉ cho phần phụ (mục 3) |
| TypeScript (Ink) | Loại — native module trên Windows hay lỗi |

## 5. Thiết kế

### 5.1. Thành phần

```
             ┌───────────── hc core (JSON-RPC: hc connect) ─────────────┐
             │ events.subscribe  terminal.get/send/run/interrupt/kill    │
             └──────▲───────────────────────────────▲───────────────────┘
                    │ sự kiện                        │ lệnh
┌───────────────────┴────────────────────────────────┴──────────────────┐
│ agm daemon                                                             │
│  ├─ hc client       (một kết nối `hc connect`, tự nối lại)            │
│  ├─ log watchers    codex | claude | opencode | agy  (mỗi nguồn 1 mô-đun)│
│  ├─ correlator      ghép terminal ↔ phiên log                          │
│  ├─ pricing         bảng giá → chi phí                                 │
│  ├─ policy          ngân sách, hạn mức thợ, gợi ý bậc model            │
│  └─ store           SQLite (WAL)                                       │
└───────────▲───────────────────────────────────────────────────────────┘
            │ đọc CSDL + gửi lệnh qua socket cục bộ (named pipe trên Windows)
   ┌────────┴─────────┐            ┌──────────────────────────────┐
   │ agm tui (pane)   │            │ agm <lệnh> (CLI, cho người     │
   └──────────────────┘            │ và cho agent)                  │
                                   └──────────────────────────────┘
```

- **Một tiến trình nền** (`agm daemon`) là nơi duy nhất ghi CSDL và giữ kết nối `hc`. TUI và CLI đọc CSDL trực tiếp (chỉ đọc, SQLite WAL) và gửi lệnh ghi/can thiệp qua IPC tới daemon.
- Nếu daemon không chạy: lệnh đọc (`ls`, `cost`, `log`) vẫn chạy trên dữ liệu cũ kèm cảnh báo; lệnh can thiệp (`send`, `spawn`) **vẫn chạy** bằng cách gọi `hc` trực tiếp và ghi CSDL ở chế độ đơn, để agent không bị kẹt khi daemon chết.
- Daemon tự chạy bằng `agm daemon --install` (Task Scheduler khi đăng nhập trên Windows), hoặc chạy trong một terminal core có `keep`.

### 5.2. Nguồn dữ liệu

| Nguồn | Vị trí | Dữ liệu lấy | Ghi chú đã kiểm |
|---|---|---|---|
| hc events | `events.subscribe` | created / finished / lost / status (state, kind, msg, progress) / turn / labels | Có `heartbeat` 15 s và `missed` + ảnh chụp mới khi tụt sau → phải xử lý để không mất trạng thái |
| Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `event_msg` loại `token_count` có `total_token_usage` {input, cached_input, cache_write_input, output, reasoning_output, total}; `response_item` cho hội thoại | Đã thấy 2026-11-18. `total_token_usage` là **cộng dồn** → lưu delta |
| Claude Code | `~/.claude/projects/<cwd-mã-hóa>/<session>.jsonl` | `message.usage` {input, cache_creation_input, cache_read_input, output, thinking} theo từng lượt; model trong `message.model` | Đã thấy. Mỗi lượt là **delta**. Có thể trùng `message.id` giữa các dòng stream → khử trùng theo id |
| OpenCode v2 | chưa xác định | — | Có tự báo trạng thái 7501 (`hc ls` thấy `status: idle`). **Vị trí lưu và định dạng phải tra (P0)** |
| agy (một lần) | đầu ra `--output-format json` | `usage`, `conversation_id` | Khi chạy qua `agm ask` (mục 5.6) agmag tự ghi |
| agy (tương tác) | `~/.gemini/antigravity/conversations/`, `conversation_summaries.db` | chưa rõ | Có thể protobuf. **Tra ở P0**, có thể giải mã thử bằng Python. Nếu không đọc được: chỉ tính token cho lượt chạy qua `agm ask` và hiện "không đo được" cho phiên tương tác |

Bộ đọc log theo dõi tệp bằng `notify`, đọc nối tiếp từ offset đã lưu (bảng `log_cursors`), chịu được dòng ghi dở (chờ dòng hoàn chỉnh). Lần chạy đầu **quét ngược** N ngày (mặc định 7, cấu hình được) để có số liệu ngay.

### 5.3. Ghép terminal ↔ phiên log (correlator)

Một terminal core và một tệp log không có khóa chung. Quy tắc theo thứ tự tin cậy:

1. **Nhãn do agm đặt**: khi `agm spawn`, agmag đặt nhãn `agm.task=<ID>` và biến môi trường riêng (ví dụ `AGM_TERMINAL=<address>`) cho tiến trình; nếu CLI ghi biến môi trường hoặc tham số vào log (Codex ghi `session_meta`), ghép chắc chắn.
2. **cwd + thời điểm**: log mới xuất hiện trong cùng `cwd`, cùng loại app (`status.app` hoặc `agent` của `hc ls`), trong cửa sổ ±N giây sau `created_ms` hoặc sau một lượt `turn` mới của terminal đó.
3. **Không ghép được**: phiên log vẫn được tính token, gắn vào "phiên không thuộc terminal core" (đây chính là phiên chạy trong HERDR TUI hoặc ngoài herdr). TUI hiện riêng nhóm này.

Khi có nhiều ứng viên ngang nhau, **không đoán**: để chưa ghép và hiện trên TUI cho người dùng ghép tay (`agm link <terminal> <session>`).

### 5.4. CSDL (SQLite, `~/.agmag/agm.db`)

Bảng chính (người làm tự chọn kiểu cột cụ thể, chỉ mục, migration):

- `terminals` — address, name, app, cwd, template, labels (JSON), task_id, parent, created/finished, exit, trạng thái cuối.
- `sessions` — nguồn (codex/claude/opencode/agy), session_id, đường dẫn log, cwd, model chính, terminal (nullable), bắt đầu/kết thúc, parser_version, `parse_errors`.
- `usage` — mỗi bản ghi một delta: session, thời điểm, model, input, cached_input, cache_write, output, reasoning; chi phí tính lúc truy vấn (không lưu cứng, để sửa bảng giá thì số liệu cũ đổi theo).
- `turns` — lượt hội thoại: session, thứ tự, vai (user/assistant/tool), tóm tắt ngắn (cắt theo giới hạn), con trỏ tới vị trí trong log gốc (để xem đầy đủ thì đọc lại log, không chép toàn bộ vào CSDL).
- `messages` — tin giữa agent qua `agm send`: từ, đến, loại (`GIAO|BÁO CÁO|LỖI|HỎI|CHẶN|ĐẠT|CHƯA ĐẠT`), task, nội dung, thời điểm, trạng thái giao (queued/sent/refused/failed), turn của đích khi giao.
- `inbox` — mục chờ người dùng: nguồn (status blocked / tin gửi `user`), terminal, nội dung, đã trả lời chưa.
- `budgets` — phạm vi (task / terminal / model / ngày), ngưỡng mềm, ngưỡng cứng, đơn vị (token hoặc USD), hành động cứng.
- `actions` — nhật ký mọi can thiệp (ai/cái gì kích hoạt, lệnh `hc` gửi đi, kết quả).
- `log_cursors` — tệp, offset, inode/size/mtime để phát hiện tệp bị thay.

### 5.5. Bảng giá và chi phí

`~/.agmag/prices.toml`: mỗi model có giá / 1M token cho input, cached input (cache read), cache write, output (reasoning tính như output nếu không có giá riêng). Có thể đặt **bí danh** (`gpt-5.5-codex` → mục giá). Model chưa có giá → chi phí hiện "?" kèm số token, **không** tính bằng 0. Gói thuê bao (Claude Max, ChatGPT Pro) có thể đánh dấu `billing = "subscription"`: vẫn hiện "chi phí tương đương API" nhưng tách riêng khỏi tiền thật.

Giá mặc định đi kèm phải được người dùng xác nhận; agmag không tự lấy giá từ mạng ở v1.

### 5.6. Lệnh CLI

Tất cả in JSON khi đi qua đường ống, văn bản bảng khi ra màn hình (giống `hc`).

| Lệnh | Việc |
|---|---|
| `agm daemon [--install\|--uninstall]` | Chạy/cài tiến trình nền |
| `agm ls` | Terminal + phiên đang theo dõi: app, model, trạng thái, token, chi phí, task, % ngân sách |
| `agm cost [--by model\|task\|terminal\|day] [--since …]` | Tổng hợp chi phí |
| `agm log <terminal\|session> [--full] [--since-turn n]` | Hội thoại; mặc định tóm tắt, `--full` đọc lại log gốc |
| `agm send <đích> --type GIAO … [--task ID] <văn bản>` | Gửi tin theo mẫu §3.2 của tutorial, qua `terminal.send` với `if: idle`; ghi `messages`. Đích `user` → vào `inbox` |
| `agm inbox` / `agm reply <mục> <văn bản>` | Xem và trả lời mục chờ người dùng (trả lời = `terminal.send` tới agent đó, có `force` khi agent đang blocked theo câu hỏi) |
| `agm spawn <template> --task ID [--name …] [--budget …] [-- nhiệm vụ]` | Kiểm hạn mức và ngân sách rồi `terminal.run` với `outlive_parent`, nhãn `agm.task`; từ chối có lý do rõ |
| `agm ask [--model …] <câu hỏi>` | Gọi `agy` dạng một lần (`--mode plan --sandbox --output-format json`), ghi token và câu trả lời |
| `agm budget set\|ls\|rm …` | Quản lý ngân sách. Ngoài ra đọc trường `budget:` trong `.agents/tasks/<ID>.md` |
| `agm interrupt\|kill <t>` | Gọi `hc` và ghi `actions` |
| `agm link <t> <session>` | Ghép tay |
| `agm tui` | Giao diện pane |

### 5.7. Chính sách (policy)

- **Hạn mức thợ**: số terminal đang sống có nhãn `agm.role=worker` ≤ `max_workers` (mặc định đọc `MAX_WORKERS` từ `.agents/bot.env` nếu có).
- **Ngân sách**: khi tổng của phạm vi vượt ngưỡng mềm → một mục trong TUI + gửi tin `LỖI`/cảnh báo ngắn cho quản lý (`codex-mgr` hoặc tên cấu hình). Vượt ngưỡng cứng → hành động cấu hình: `notify` (mặc định) | `interrupt` | `kill`; luôn từ chối `spawn` mới trong phạm vi đó.
- Vì quan sát là **sau khi tiêu**, ngưỡng cứng có thể bị vượt quá một lượt; tài liệu và TUI nói rõ điều này.
- **Gợi ý bậc model** (chỉ gợi ý): ví dụ thợ bậc Mạnh mà lượt gần nhất ngắn và không lỗi → gợi ý hạ; thợ CHƯA ĐẠT 2 lần → gợi ý nâng. Luật cụ thể là đề xuất, người làm có thể đơn giản hóa.
- Mọi hành động tự động có công tắc tắt toàn cục (`agm policy off`) — "nút dừng khẩn".

### 5.8. TUI

```
┌ agmag ─ hôm nay $4.12 (API $3.20 · thuê bao ~$11.40) ──────── daemon ● ┐
│ TERMINALS                         model            tok     $     task  │
│ ● t11 arch        claude working  opus-5.5      1.2M   2.10  login-7 │
│ ○ t1f w-login-7-1 opencode idle   qwen3-coder   340k   0.08  login-7 │
│ ! t16 review      claude BLOCKED  opus-5.5       90k   0.40  login-7 │
│ · (ngoài core) codex  3 phiên                   2.1M   1.54   —      │
├ NGÂN SÁCH ─────────────────────────────────────────────────────────────┤
│ login-7   ██████▓░░░ 62% / $5    ngày  ███░░░░░░░ 31% / $20            │
├ HỘP THƯ (2) ─ TIN GẦN ĐÂY ─────────────────────────────────────────────┤
│ ! t16 hỏi quyền: chạy `cargo test`?                                    │
│ 10:41 codex-mgr → w-login-7-1  GIAO  login-7  .agents/tasks/login-7.md │
└ [enter] xem  [s] gửi  [r] trả lời  [i] ngắt  [k] kill  [b] ngân sách  [q] ┘
```

Màn chi tiết một terminal: hội thoại (tóm tắt lượt), biểu đồ token theo lượt, tin vào/ra. TUI chỉ đọc CSDL và gửi lệnh qua IPC; làm mới khi daemon báo có thay đổi (hoặc 1 s).

## 6. Cách triển khai (đội agent)

Theo `AGENTS-TUTORIAL.md`:

- **Claude (phiên này)**: thiết kế, kế hoạch, chia đầu việc vào `.agents/tasks/<ID>.md`, duyệt từng nhánh.
- **Codex** (`codex exec` trong git worktree riêng ở `.wt/<ID>`): viết code theo từng đầu việc, test, commit trên nhánh.
- **agy** (`agy --mode plan --sandbox --output-format json -p …`, một lần): tra cứu các điểm chưa rõ ở P0.
- Script Python phụ (làm sạch mẫu log, giải mã thử agy) nằm ở `tools/`.

Giai đoạn đề xuất (người lập kế hoạch có thể chia nhỏ hơn):

- **P0 — Xác minh**: OpenCode v2 lưu phiên ở đâu và định dạng; agy tương tác lưu gì, đọc được không; dạng `data` của sự kiện `status`/`turn`; cách gọi `hc connect` từ Rust (khung JSON-RPC, cách nối lại). Thu mẫu log thật, làm sạch, đưa vào `tests/fixtures/`. Kết quả ghi vào `.agents/decisions/`.
- **P1 — Lõi đo**: store + bộ đọc Codex và Claude + pricing + `agm ls`, `agm cost`, `agm log`.
- **P2 — Daemon + hc**: kết nối events, correlator, inbox từ trạng thái blocked.
- **P3 — TUI**.
- **P4 — Can thiệp**: `send`, `reply`, `spawn`, `interrupt/kill`, budgets, policy, `ask`.
- **P5 — OpenCode, agy tương tác** (nếu P0 cho thấy đọc được), `daemon --install`.

Mỗi giai đoạn dùng được ngay sau khi xong (P1 đã trả lời được "hôm nay tốn bao nhiêu").

## 7. Kiểm thử

- Bộ đọc log: test đơn vị trên mẫu log thật đã làm sạch, gồm dòng ghi dở, tệp bị thay, định dạng lạ (phải ra lỗi có tên chứ không ra 0).
- Delta token Codex (cộng dồn → delta) và khử trùng Claude: test riêng, đối chiếu tổng với số CLI tự báo.
- hc: một hc giả (mock JSON-RPC) cho test tích hợp; một bài kiểm tay trên máy thật cho mỗi giai đoạn P2, P4.
- Policy: test bảng quyết định (vượt mềm, vượt cứng, tắt policy, daemon chết).

## 8. Để lại cho người làm

- Tên crate, cấu trúc mô-đun, thư viện cụ thể (đề xuất: `tokio`, `serde_json`, `rusqlite` bundled, `notify`, `ratatui` + `crossterm`, `clap`, `interprocess` cho named pipe).
- Kiểu cột, chỉ mục, cơ chế migration.
- Giới hạn cắt tóm tắt lượt hội thoại, khoảng thời gian cửa sổ ghép, số ngày quét ngược.
- Bố cục chi tiết TUI và phím tắt (giữ đúng các khu vực ở 5.8).
- Luật gợi ý bậc model cụ thể.

## 9. Chưa xác minh (P0 phải trả lời)

1. Vị trí và định dạng phiên OpenCode v2.
2. agy tương tác: định dạng `conversations/` và `conversation_summaries.db`; có tách được token không.
3. Dạng `data` của sự kiện `status`, `turn` trong `events.subscribe`.
4. Codex/Claude có ghi biến môi trường hoặc gì đó để ghép chắc chắn với terminal không (mục 5.3 bước 1).
5. Giá hiện hành của từng model đang dùng (người dùng xác nhận).
