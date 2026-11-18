# agmag Kế hoạch 1: Xác minh (P0) + Lõi đo (P1) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Trong dự án này:** người thực thi mỗi đầu việc Rust là **Codex** (`codex exec` trong worktree riêng); việc tra cứu là **agy**; Claude (phiên điều phối) chia việc, duyệt diff, chạy lại test, tích hợp. Xem "Quy trình giao việc" ngay dưới.

**Goal:** `agm` đọc log của Codex và Claude Code, lưu token / hạn mức gói / mức đầy ngữ cảnh / tóm tắt lượt vào SQLite, và trả lời được "hôm nay tốn bao nhiêu, gói nào sắp chạm trần, phiên nào sắp đầy" qua `agm ls`, `agm cost`, `agm quota`, `agm log`.

**Architecture:** Một crate Rust, binary `agm`. Bộ đọc log tách theo nguồn (`parse::codex`, `parse::claude`) trả về `ParseEvent` chuẩn hóa; `ingest` đọc nối tiếp theo con trỏ lưu trong CSDL; `store` là SQLite; `report` tổng hợp; `main` là CLI. Chưa có daemon (P2): lệnh đọc tự chạy quét tăng dần trước khi trả lời.

**Tech Stack:** Rust (edition 2021, toolchain stable đang cài), clap 4 (derive), serde/serde_json, rusqlite (feature `bundled`), toml, anyhow, thiserror, walkdir, dirs, comfy-table; dev: tempfile. Python 3.13 chỉ cho `tools/`.

**Spec:** `docs/superpowers/specs/2026-11-18-agmag-design.md`

## Quy trình giao việc (Claude điều phối)

1. Task 0 do Claude làm trên `main` (dựng crate + `cargo fetch`), để Codex làm việc **offline** trong sandbox.
2. Mỗi Task N (N ≥ 1): `git worktree add .wt/t<N> -b feat/t<N>` → chép nội dung Task N vào `.wt/t<N>/.agents/tasks/t<N>.md` → chạy:
   ```powershell
   codex exec -C .wt/t<N> -s workspace-write -o .agents/.runtime/t<N>.last.md "Đọc .agents/tasks/t<N>.md và docs/superpowers/specs/2026-11-18-agmag-design.md. Làm đúng các bước theo thứ tự (TDD), chạy cargo test --offline, commit trên nhánh hiện tại với thông điệp bắt đầu bằng 't<N>:'. Không sửa ngoài phạm vi tệp đã liệt kê. Cuối cùng in: tệp đã đổi, kết quả cargo test."
   ```
3. Claude duyệt: `git -C .wt/t<N> diff main...HEAD`, chạy lại `cargo test --offline` trong worktree, đối chiếu Interfaces. Đạt → `git merge --no-ff feat/t<N>` vào `main`, gỡ worktree. Chưa đạt → ghi lỗi nguyên văn vào `.agents/tasks/t<N>.md` mục "Lần sửa", chạy lại `codex exec` (tối đa 2 lần, sau đó Claude tự sửa hoặc hỏi người dùng).
4. Task 1, 2, 3 độc lập sau Task 0 → chạy song song được. Task 4 cần 1; Task 5 cần 2, 3, 4; Task 6 cần 1; Task 7 cần 4, 5, 6.
5. Task P0 (tra cứu) chạy song song với tất cả, không chặn P1.

## Global Constraints

- Nền chính Windows 11; đường dẫn dùng `PathBuf`, không ghép chuỗi `/`. Nhà agmag: `~/.agmag/` (ghi đè bằng biến môi trường `AGM_HOME`); CSDL `agm.db`; cấu hình `providers.toml`.
- **Không bao giờ ghi** vào `~/.codex` hoặc `~/.claude`. Mở log chỉ để đọc (`std::fs::File::open` trên Windows đã chia sẻ READ|WRITE|DELETE — không khóa tệp của CLI).
- **Chuẩn hóa token** (`Usage`): `input_uncached`, `cache_read`, `cache_write`, `output` (đã gồm reasoning/thinking), `reasoning` (tập con của `output`, chỉ để hiển thị).
  - Codex: `input_uncached = input_tokens − cached_input_tokens`; `cache_read = cached_input_tokens`; `cache_write = cache_write_input_tokens`; `output = output_tokens`; `reasoning = reasoning_output_tokens`.
  - Claude: `input_uncached = input_tokens`; `cache_read = cache_read_input_tokens`; `cache_write = cache_creation_input_tokens`; `output = output_tokens`; `reasoning = output_tokens_details.thinking_tokens` (thiếu → 0).
- **Không số giả:** thiếu giá → chi phí `None` (hiện `?`); thiếu cửa sổ ngữ cảnh → `context_pct` `None`; dòng không phân tích được → đếm `bad_lines`, không bỏ qua im lặng.
- CLI in JSON khi stdout không phải TTY hoặc có `--json`; in bảng khi là TTY (giống `hc`).
- Không lưu: `encrypted_content`, `base_instructions`, `world_state`, id tài khoản/tổ chức, nội dung `api-request-blob`. Tóm tắt lượt ≤ 200 ký tự (cắt theo ký tự, không theo byte).
- Thông điệp commit bắt đầu bằng `t<N>:`.

## Review Focus

1. Tệp log đang được ghi dở (dòng cuối chưa có `\n`) → lần quét này bỏ qua phần dở, lần sau đọc đủ; không bao giờ ra `bad_lines`. Test: Task 5 `partial_line_is_deferred`.
2. Claude ghi cùng `message.id` trên nhiều dòng với `usage` tăng dần → chỉ tính một lần, giá trị dòng cuối. Test: Task 3 `duplicate_message_id_last_wins` + Task 4 `upsert_usage_last_wins`.
3. Codex `token_count` có `info: null` (đầu phiên) → không panic, chỉ ra `Quota` nếu có `rate_limits`. Test: Task 2 `token_count_with_null_info`.
4. Model không có trong `providers.toml` / không có giá → `agm cost` hiện `?` cho USD, vẫn hiện token. Test: Task 6 `unknown_model_has_no_price`, Task 7 `cost_json_unknown_model_null_usd`.
5. `cwd` có ký tự tiếng Việt (`D:\Dự án\x`) đi qua CSDL và JSON nguyên vẹn. Test: Task 4 `unicode_cwd_roundtrip`.

---

### Task 0: Dựng crate (Claude làm trên `main`)

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `.gitignore`

- [ ] **Step 1:** `cargo init --name agmag` rồi `cargo add clap --features derive`, `cargo add serde --features derive`, `cargo add serde_json toml anyhow thiserror walkdir dirs comfy-table`, `cargo add rusqlite --features bundled`, `cargo add --dev tempfile`. Trong `Cargo.toml` đặt `[[bin]] name = "agm" path = "src/main.rs"`.
- [ ] **Step 2:** `src/lib.rs` khai báo `pub mod model; pub mod parse; pub mod store; pub mod ingest; pub mod config; pub mod report;` — mỗi mô-đun một tệp rỗng tạm (`src/parse/mod.rs` rỗng) để crate biên dịch.
- [ ] **Step 3:** `.gitignore`: `/target`, `.wt/`, `.agents/.runtime/`.
- [ ] **Step 4:** `cargo fetch` rồi `cargo build --offline` → biên dịch thành công.
- [ ] **Step 5:** Commit `t0: scaffold agmag crate`.

---

### Task P0: Xác minh (agy tra cứu + kiểm trên máy) — song song, không chặn P1

**Files:**
- Create: `.agents/decisions/p0-research.md`, `tools/capture_hc_events.py`

- [ ] **Step 1: agy tra cứu** (chạy trong thư mục tạm ngoài repo):
  ```powershell
  agy --mode plan --sandbox --print-timeout 300s --output-format json -p "Trả lời từng câu, mỗi kết luận kèm URL nguồn, không chắc thì ghi 'chưa xác minh': (1) OpenCode v2 (gói npm @opencode/cli) lưu lịch sử phiên ở đâu trên Windows, định dạng gì, có trường token usage và model không? (2) Claude Code: JSON mà statusline command nhận qua stdin có trường nào về hạn mức gói (rate limit, five_hour, seven_day, used_percentage)? Liệt kê tên trường. (3) Antigravity CLI (agy): ~/.gemini/antigravity/conversations và conversation_summaries.db có định dạng gì (protobuf? sqlite?), có token usage không?" > $env:TEMP\agmag-p0.json
  ```
- [ ] **Step 2: Kiểm trên máy** từng kết luận của agy (mở thư mục/tệp nó nêu; `sqlite3`/Python `-I` để xem `.db`). Kết quả agy từng sai phiên bản — chỉ ghi "đã kiểm" khi thấy tận mắt.
- [ ] **Step 3: `tools/capture_hc_events.py`** — chạy `hc events --json` trong N giây (tham số, mặc định 120), ghi mỗi dòng có `params.type` ∈ {`status`, `turn`} vào `.agents/.runtime/hc-events.jsonl`. Trong lúc chạy, gửi một câu hỏi cho một phiên Claude Code và một phiên OpenCode đang mở để bắt được sự kiện. Ghi lại dạng `data` thật.
- [ ] **Step 4: Ghép terminal ↔ phiên và người gửi** (spec §9 mục 4, 6): trong một terminal `hc run --bg pwsh`, chạy `Get-ChildItem env: | Where-Object Name -match 'HC|HERDR'` để xem hc đặt biến môi trường nào; xem `session_meta` của Codex và dòng đầu log Claude có lưu biến/định danh terminal không; đọc mô tả tham số `caller` của `terminal.send` trong `hc schema`.
- [ ] **Step 5:** Ghi `.agents/decisions/p0-research.md`: mỗi câu (1)–(3), dạng sự kiện `status`/`turn`, kết quả Step 4, đánh dấu **[ĐÃ KIỂM]** hoặc **[CHƯA]**, kèm hệ quả cho Kế hoạch 2.
- [ ] **Step 6:** Commit `p0: research notes for opencode/agy/claude quota/hc events`.

---

### Task 1: Kiểu dữ liệu chuẩn + bộ làm sạch fixture

**Files:**
- Create: `src/model.rs`, `tools/sanitize_fixture.py`, `tests/fixtures/codex/real_sanitized.jsonl`, `tests/fixtures/claude/real_sanitized.jsonl`
- Test: `tests/model_test.rs`

**Interfaces:**
- Produces (`agmag::model`, tất cả `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`):
  ```rust
  pub enum Source { Codex, Claude }            // serde: "codex" | "claude"; impl Source { fn as_str(&self)->&'static str }
  pub struct Usage { pub input_uncached: u64, pub cache_read: u64, pub cache_write: u64, pub output: u64, pub reasoning: u64 }
  impl Usage { pub fn add(&mut self, o: &Usage); pub fn total(&self) -> u64 /* input_uncached+cache_read+cache_write+output */ }
  pub struct UsageRecord { pub dedupe_key: String, pub ts_ms: i64, pub model: Option<String>, pub usage: Usage }
  pub struct QuotaWindow { pub window_minutes: u32, pub used_percent: f64, pub resets_at_s: i64 }
  pub struct QuotaSnapshot { pub provider: String, pub ts_ms: i64, pub windows: Vec<QuotaWindow> }
  pub struct ContextSnapshot { pub ts_ms: i64, pub used_tokens: u64, pub window_tokens: Option<u64> }
  pub struct SessionMeta { pub session_id: String, pub cwd: Option<String>, pub cli_version: Option<String>, pub started_ms: Option<i64> }
  pub enum Role { User, Assistant, Tool }      // serde lowercase
  pub struct TurnSummary { pub ts_ms: i64, pub role: Role, pub summary: String, pub byte_offset: u64 }
  pub enum ParseEvent { Meta(SessionMeta), Model(String), Usage(UsageRecord), Quota(QuotaSnapshot), Context(ContextSnapshot), Turn(TurnSummary) }
  pub enum LineOutcome { Events(Vec<ParseEvent>), Ignored, Unknown(String) }
  pub fn summarize(text: &str) -> String      // gộp khoảng trắng, cắt ≤ 200 ký tự, thêm "…" nếu cắt
  pub fn parse_rfc3339_ms(s: &str) -> Option<i64>
  ```

- [ ] **Step 1: Test** `tests/model_test.rs`:
  - `usage_total_and_add`: `Usage{1,2,3,4,0}.total()==10`; `add` cộng từng trường.
  - `summarize_truncates_by_chars`: chuỗi 300 ký tự "ệ" → kết quả `chars().count()==200` và kết thúc bằng `…`; `"a \n  b"` → `"a b"`.
  - `parse_rfc3339`: `"2026-11-18T05:27:51.441Z"` → `Some(1794979671441)`; `"x"` → `None`.
- [ ] **Step 2:** `cargo test --offline --test model_test` → FAIL (chưa có kiểu).
- [ ] **Step 3:** Viết `src/model.rs` đúng Interfaces. `parse_rfc3339_ms` tự viết (không thêm crate thời gian): chỉ cần dạng `YYYY-MM-DDTHH:MM:SS(.fff)?Z`.
- [ ] **Step 4:** Test PASS.
- [ ] **Step 5: `tools/sanitize_fixture.py <in.jsonl> <out.jsonl> --max-lines 400`** — giữ cấu trúc, thay mọi chuỗi trong `content`/`text`/`input`/`output`/`arguments` bằng `"lorem"`, xóa khóa `encrypted_content`, `base_instructions`, `world_state` (bỏ cả dòng), `creator_user_id`, `creator_account_id`, `accountUuid`, `organizationUuid`, bỏ dòng `api-request-blob`. Chạy trên tệp log thật mới nhất của mỗi nguồn → hai fixture. Kiểm: `grep -E "accountUuid|creator_account_id|encrypted_content" tests/fixtures -r` không ra gì.
- [ ] **Step 6:** Commit `t1: normalized model types and sanitized fixtures`.

---

### Task 2: Bộ đọc log Codex

**Files:**
- Create: `src/parse/mod.rs`, `src/parse/codex.rs`, `tests/fixtures/codex/mini.jsonl`
- Test: `tests/parse_codex_test.rs`

**Interfaces:**
- Consumes: `agmag::model::*` (Task 1).
- Produces:
  ```rust
  // src/parse/mod.rs
  #[derive(Debug, thiserror::Error)] pub enum ParseError { #[error("bad json: {0}")] BadJson(String) }
  pub trait LogParser {
      fn parse_line(&mut self, line: &str, byte_offset: u64) -> Result<LineOutcome, ParseError>;
      fn seed_model(&mut self, model: &str);   // nạp model hiện tại khi đọc tiếp giữa tệp (Task 5)
  }
  pub fn parser_for(source: Source) -> Box<dyn LogParser>   // nhánh Claude: todo!("t3") cho tới khi Task 3 merge
  // src/parse/codex.rs
  pub struct CodexParser { /* model hiện tại từ turn_context */ }  impl Default + LogParser
  ```
- Ánh xạ (theo `type` / `payload.type`):
  - `session_meta` → `Meta{session_id: payload.id, cwd, cli_version, started_ms: parse(payload.timestamp)}`
  - `turn_context` → `Model(payload.model)` và nhớ làm model hiện tại.
  - `token_usage_record` → `Usage{dedupe_key: payload.response_id, ts_ms: timestamp ngoài, model: model hiện tại, usage: chuẩn hóa Codex (Global Constraints) từ payload.usage}`
  - `event_msg`/`token_count` → nếu `payload.info` khác null: `Context{used_tokens: info.last_token_usage.input_tokens, window_tokens: info.model_context_window}`; nếu `payload.rate_limits` có: `Quota{provider: "codex", windows: [primary, secondary] bỏ cái null}`.
  - `response_item`/`message` role `user`|`assistant` → `Turn` (text đầu tiên của `content`, qua `summarize`); role `developer` → `Ignored`.
  - `response_item`/`function_call`|`custom_tool_call` → `Turn{role: Tool, summary: "[tool] " + name}`.
  - Các loại khác đã thấy (`reasoning`, `function_call_output`, `custom_tool_call_output`, `additional_tools`, `world_state`, `event_msg` khác, `token_usage_record` thiếu `response_id`…) → `Ignored`. `type` chưa từng thấy → `Unknown(type)`. JSON hỏng → `Err(BadJson)`.

- [ ] **Step 1:** Viết `tests/fixtures/codex/mini.jsonl` (tay, 7 dòng): `session_meta` (cwd `D:\\Project\\demo`), `turn_context` model `gpt-6.1-sol`, `response_item` message user "hello", `token_usage_record` (response_id `resp_1`, input 29000, cached 13312, cache_write 0, output 285, reasoning 50), `event_msg` token_count với `info: null` + rate_limits (primary 22.0/300/1794990587, secondary 79.0/10080/1795408985), `event_msg` token_count có info (last input 116029, window 475000), một dòng `{"type":"brand_new_kind"}`.
- [ ] **Step 2: Test** `tests/parse_codex_test.rs`:
  - `codex_usage_normalized`: Usage của `resp_1` == `Usage{input_uncached:15688, cache_read:13312, cache_write:0, output:285, reasoning:50}`, `model==Some("gpt-6.1-sol")`.
  - `token_count_with_null_info`: dòng info null → chỉ một `Quota` với 2 cửa sổ, `windows[0]==QuotaWindow{300, 22.0, 1794990587}`; không có `Context`.
  - `context_snapshot`: `Context{used_tokens:116029, window_tokens:Some(475000)}`.
  - `meta_and_turn`: `Meta.cwd==Some("D:\\Project\\demo")`; Turn user summary `"hello"`.
  - `seeded_model_used`: parser mới, `seed_model("gpt-6.1-sol")`, chỉ đưa dòng `token_usage_record` → `model==Some("gpt-6.1-sol")`.
  - `unknown_type_reported`: dòng cuối → `Unknown("brand_new_kind")`; `"{oops"` → `Err`.
  - `real_fixture_smoke`: đọc `real_sanitized.jsonl` → 0 lỗi, ≥ 1 Usage, ≥ 1 Quota.
- [ ] **Step 3:** FAIL. **Step 4:** Viết `CodexParser` (dùng `serde_json::Value`, không định nghĩa struct cho toàn bộ log — định dạng đổi thường xuyên). **Step 5:** PASS.
- [ ] **Step 6:** Commit `t2: codex rollout parser`.

---

### Task 3: Bộ đọc log Claude Code

**Files:**
- Create: `src/parse/claude.rs`, `tests/fixtures/claude/mini.jsonl`
- Modify: `src/parse/mod.rs` (thêm nhánh `Source::Claude` trong `parser_for` — nếu Task 2 chưa merge, tạo `mod.rs` đúng Interfaces của Task 2)
- Test: `tests/parse_claude_test.rs`

**Interfaces:**
- Consumes: `LogParser`, `ParseError`, `parser_for` (Task 2), model (Task 1).
- Produces: `pub struct ClaudeParser` impl `Default + LogParser`.
- Ánh xạ:
  - Dòng đầu tiên có `sessionId` và `cwd` → `Meta{session_id: sessionId, cwd, cli_version: version, started_ms: timestamp}` (chỉ một lần).
  - `type=="assistant"`: `Model(message.model)` khi đổi; `Usage{dedupe_key: message.id, ts_ms, model, usage: chuẩn hóa Claude}`; `Context{used_tokens: input+cache_read+cache_creation, window_tokens: None}`; `Turn{Assistant, summary: text đầu tiên, hoặc "[tool] "+name của tool_use đầu tiên}`. Model `"<synthetic>"` → bỏ Usage/Context, chỉ Turn.
  - `type=="user"`: `message.content` là chuỗi hoặc có khối `text` → `Turn{User}`; chỉ có `tool_result` → `Ignored`.
  - Ignored: `custom-title`, `agent-name`, `session-owner`, `queue-operation`, `attachment`, `last-prompt`, `atis-latch`, `api-request-shape`, `api-request-blob`, `api-request`, `tools-used`, `summary`, `system`, `file-history-snapshot`. Khác → `Unknown`.

- [ ] **Step 1:** `tests/fixtures/claude/mini.jsonl` (tay): dòng `user` (sessionId `s-1`, cwd `D:\\Dự án\\x`, text "xin chào"), hai dòng `assistant` cùng `message.id` `msg_1` (dòng 1 output 10, dòng 2 output 1208; cả hai input 2, cache_read 356264, cache_creation 281, thinking 348, model `claude-opus-5-5`), một dòng `api-request-blob`, một dòng `{"type":"weird"}`.
- [ ] **Step 2: Test** `tests/parse_claude_test.rs`:
  - `claude_usage_normalized`: Usage dòng 2 == `Usage{input_uncached:2, cache_read:356264, cache_write:281, output:1208, reasoning:348}`.
  - `duplicate_message_id_last_wins`: cả hai dòng ra `dedupe_key=="msg_1"` (khử trùng là việc của store, Task 4).
  - `context_from_last_turn`: `Context.used_tokens == 2+356264+281`, `window_tokens==None`.
  - `meta_unicode_cwd`: `Meta.cwd==Some("D:\\Dự án\\x")`.
  - `ignored_and_unknown`: blob → `Ignored`; `weird` → `Unknown("weird")`.
  - `real_fixture_smoke`: 0 lỗi, ≥ 1 Usage.
- [ ] **Step 3:** FAIL. **Step 4:** Viết `ClaudeParser`. **Step 5:** PASS.
- [ ] **Step 6:** Commit `t3: claude code transcript parser`.

---

### Task 4: Kho SQLite

**Files:**
- Create: `src/store/mod.rs`, `src/store/schema.sql`
- Test: `tests/store_test.rs`

**Interfaces:**
- Consumes: model (Task 1).
- Produces:
  ```rust
  pub struct Store { /* rusqlite::Connection */ }
  pub struct Cursor { pub path: String, pub offset: u64, pub size: u64, pub mtime_ms: i64, pub head_hash: u64 }
  pub struct SessionStats { pub unknown_lines: u64, pub bad_lines: u64 }
  impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Store>;      // tạo thư mục cha, WAL, chạy schema.sql (include_str!), user_version=1
    pub fn open_in_memory() -> anyhow::Result<Store>;
    pub fn session_id_for_path(&self, source: Source, path: &str) -> anyhow::Result<i64>;   // tạo nếu chưa có; khóa duy nhất = path
    pub fn set_meta(&self, sid: i64, meta: &SessionMeta) -> anyhow::Result<()>;
    pub fn set_model(&self, sid: i64, model: &str) -> anyhow::Result<()>;
    pub fn upsert_usage(&self, sid: i64, rec: &UsageRecord) -> anyhow::Result<()>;          // UNIQUE(session, dedupe_key), dòng sau ghi đè
    pub fn insert_quota(&self, snap: &QuotaSnapshot) -> anyhow::Result<()>;                // UNIQUE(provider, ts_ms, window_minutes) bỏ qua trùng
    pub fn set_context(&self, sid: i64, ctx: &ContextSnapshot) -> anyhow::Result<()>;       // chỉ giữ bản mới nhất theo ts_ms
    pub fn insert_turn(&self, sid: i64, t: &TurnSummary) -> anyhow::Result<()>;             // UNIQUE(session, byte_offset) bỏ qua trùng
    pub fn add_line_stats(&self, sid: i64, unknown: u64, bad: u64) -> anyhow::Result<()>;
    pub fn cursor(&self, path: &str) -> anyhow::Result<Option<Cursor>>;
    pub fn save_cursor(&self, c: &Cursor) -> anyhow::Result<()>;
    pub fn conn(&self) -> &rusqlite::Connection;          // cho report (Task 7) truy vấn đọc
  }
  ```
- Bảng: `sessions(id, source, path UNIQUE, session_id, cwd, cli_version, started_ms, last_ms, model, ctx_ts_ms, ctx_used, ctx_window, unknown_lines, bad_lines)`, `usage(session, dedupe_key, ts_ms, model, input_uncached, cache_read, cache_write, output, reasoning)`, `quota(provider, ts_ms, window_minutes, used_percent, resets_at_s)`, `turns(session, byte_offset, ts_ms, role, summary)`, `cursors(path PRIMARY KEY, offset, size, mtime_ms, head_hash)`. `last_ms` = max ts của usage/turn trong phiên (cập nhật trong `upsert_usage`/`insert_turn`). Các bảng của P2–P4 (`terminals`, `messages`, `inbox`, `budgets`, `actions`, `rotations`) **chưa tạo** — Kế hoạch 2 thêm bằng `user_version=2`.

- [ ] **Step 1: Test** `tests/store_test.rs` (dùng `open_in_memory`):
  - `upsert_usage_last_wins`: hai lần `upsert_usage` cùng `msg_1` (output 10 rồi 1208) → `SELECT COUNT(*), SUM(output)` == `(1, 1208)`.
  - `unicode_cwd_roundtrip`: `set_meta` cwd `D:\Dự án\x` → đọc lại đúng nguyên văn.
  - `context_keeps_latest`: ts 200 rồi ts 100 → còn ctx của ts 200.
  - `turn_idempotent`: `insert_turn` cùng offset hai lần → 1 dòng.
  - `cursor_roundtrip`: `save_cursor` rồi `cursor` trả đúng giá trị; path chưa có → `None`.
  - `open_creates_file`: `open(tmp/a/b/agm.db)` tạo được và `PRAGMA user_version` == 1.
- [ ] **Step 2:** FAIL. **Step 3:** Viết `Store` + `schema.sql`. **Step 4:** PASS.
- [ ] **Step 5:** Commit `t4: sqlite store`.

---

### Task 5: Quét tăng dần (ingest)

**Files:**
- Create: `src/ingest.rs`
- Test: `tests/ingest_test.rs`

**Interfaces:**
- Consumes: `parser_for` (Task 2/3), `Store` (Task 4).
- Produces:
  ```rust
  pub struct Roots { pub codex: PathBuf, pub claude: PathBuf }
  impl Roots { pub fn from_home() -> Roots /* ~/.codex/sessions, ~/.claude/projects */ }
  #[derive(Default, Debug, PartialEq)] pub struct IngestStats { pub files: u64, pub lines: u64, pub events: u64, pub unknown: u64, pub bad: u64 }
  pub fn discover(roots: &Roots, since_ms: i64) -> Vec<(Source, PathBuf)>;   // codex: **/rollout-*.jsonl; claude: **/*.jsonl; lọc mtime >= since_ms; thư mục không tồn tại → bỏ qua
  pub fn ingest_file(store: &Store, source: Source, path: &Path) -> anyhow::Result<IngestStats>;
  pub fn ingest_all(store: &Store, roots: &Roots, since_ms: i64) -> anyhow::Result<IngestStats>;
  ```
- Hành vi `ingest_file`: đọc từ `cursor.offset`; chỉ xử lý tới `\n` cuối cùng, phần sau giữ cho lần sau; `head_hash` = hash 4 KiB đầu (`std::hash::DefaultHasher` là đủ); nếu `size < offset` hoặc `head_hash` khác → đọc lại từ 0 (an toàn vì mọi ghi đều idempotent). Parser dựng mới mỗi lần gọi; khi `offset > 0` và `sessions.model` có giá trị → gọi `seed_model` trước khi đọc tiếp. Dòng không phải UTF-8 → `String::from_utf8_lossy`. Mỗi `ParseEvent` gọi đúng hàm `Store` tương ứng; `Unknown`/`Err` cộng vào `add_line_stats`. Toàn bộ một tệp trong một transaction.

- [ ] **Step 1: Test** `tests/ingest_test.rs` (tempdir + `Store::open_in_memory`):
  - `ingest_mini_codex`: chép `fixtures/codex/mini.jsonl` → `IngestStats{files:1, lines:7, unknown:1, bad:0, ..}`; bảng usage có 1 dòng.
  - `partial_line_is_deferred`: ghi 1 dòng đủ + nửa dòng không `\n` → `bad==0`, `lines==1`; nối nốt nửa sau + `\n` → lần 2 `lines==1`, tổng usage đúng.
  - `rescan_is_noop`: quét hai lần liên tiếp → lần 2 `lines==0`.
  - `replaced_file_rereads`: ghi đè tệp bằng nội dung khác ngắn hơn → lần sau đọc từ 0, không lỗi.
  - `model_survives_resume`: Codex: quét tới sau `turn_context`, nối thêm `token_usage_record` → usage mới có `model==Some("gpt-6.1-sol")`.
  - `discover_filters`: tạo `codex/2026/11/18/rollout-a.jsonl`, `codex/x.txt`, `claude/p/s.jsonl` → trả đúng 2 tệp với Source đúng.
- [ ] **Step 2:** FAIL. **Step 3:** Viết `ingest.rs`. **Step 4:** PASS.
- [ ] **Step 5:** Commit `t5: incremental ingest with cursors`.

---

### Task 6: Cấu hình nhà cung cấp, cửa sổ ngữ cảnh, giá

**Files:**
- Create: `src/config.rs`, `assets/providers.default.toml`
- Test: `tests/config_test.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Deserialize, Clone, Copy, PartialEq, Debug)] #[serde(rename_all="lowercase")] pub enum Billing { Subscription, Api }
  pub struct ProviderCfg { pub billing: Billing }
  pub struct Price { pub input: f64, pub cache_read: f64, pub cache_write: f64, pub output: f64 }   // USD / 1M token
  pub struct ModelCfg { pub provider: String, pub context_window: Option<u64>, pub price: Option<Price> }
  pub struct Config { pub providers: BTreeMap<String, ProviderCfg>, pub models: BTreeMap<String, ModelCfg>, pub aliases: BTreeMap<String, String> }
  impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Config>;   // không có tệp → builtin(); có tệp → parse, lỗi parse là Err (không im lặng dùng builtin)
    pub fn builtin() -> Config;                             // từ include_str!("../assets/providers.default.toml")
    pub fn model(&self, name: &str) -> Option<&ModelCfg>;   // tra alias trước
    pub fn billing_for_model(&self, name: &str) -> Option<Billing>;
    pub fn provider_for_source(source: Source) -> &'static str;   // Codex→"codex", Claude→"claude"
  }
  pub fn cost_usd(u: &Usage, p: &Price) -> f64;
  pub fn context_pct(used: u64, window: Option<u64>) -> Option<f64>;   // None nếu window None hoặc 0; làm tròn 1 chữ số
  ```
- `providers.default.toml`: `[providers.codex] billing="subscription"`, `[providers.claude] billing="subscription"`, `[providers.agy] billing="subscription"`, `[providers.openrouter] billing="api"`, `[providers.ollama] billing="api"`; `[models."gpt-6.1-sol"] provider="codex"` (không ghi context_window — Codex tự báo trong log); `[models."claude-opus-5-5"] provider="claude"` với `context_window` **để trống và chú thích "người dùng xác nhận"**. Không ghi giá nào (người dùng điền cho model `api`).
- **Người dùng viết `cost_usd`** (chế độ học): Codex viết test và để thân hàm là `todo!("TODO(human)")`; test `cost_*` được đánh dấu là chờ người dùng — Claude điều phối gửi yêu cầu "Learn by Doing" khi tích hợp Task 6.

- [ ] **Step 1: Test** `tests/config_test.rs`:
  - `default_has_subscriptions`: `billing_for_model("gpt-6.1-sol")==Some(Subscription)`, `billing_for_model("claude-opus-5-5")==Some(Subscription)`.
  - `alias_resolves`: tệp tạm có `[aliases] "opus"="claude-opus-5-5"` → `model("opus")` trả cfg của claude.
  - `unknown_model_has_no_price`: `model("nope")==None`.
  - `bad_toml_is_error`: `load` tệp `billing = 3` → `Err`.
  - `context_pct_cases`: `(116029, Some(475000))==Some(24.4)`; `(5, None)==None`; `(5, Some(0))==None`.
  - `cost_basic`: `Usage{1_000_000,0,0,0,0}` với `Price{input:3.0,..0}` → `3.0`; `Usage{0,1_000_000,0,1_000_000,500_000}` với `Price{input:3.0,cache_read:0.3,cache_write:3.75,output:15.0}` → `15.3` (reasoning đã nằm trong output, không cộng lần hai).
- [ ] **Step 2:** FAIL. **Step 3:** Viết `config.rs` (trừ thân `cost_usd`). **Step 4:** Mọi test trừ `cost_basic` PASS.
- [ ] **Step 5:** Commit `t6: provider config, context pct (cost_usd awaiting human)`.

---

### Task 7: Báo cáo + CLI `agm`

**Files:**
- Create: `src/report.rs`, `src/main.rs` (thay stub)
- Test: `tests/report_test.rs`, `tests/cli_test.rs`

**Interfaces:**
- Consumes: `Store` (4), `ingest_all`/`Roots` (5), `Config`/`cost_usd`/`context_pct` (6).
- Produces:
  ```rust
  #[derive(Serialize)] pub struct SessionRow { pub source: Source, pub session_id: Option<String>, pub path: String, pub cwd: Option<String>, pub model: Option<String>, pub last_ms: Option<i64>, pub usage: Usage, pub context_pct: Option<f64>, pub unknown_lines: u64, pub bad_lines: u64, pub warning: Option<String> }
  #[derive(Clone, Copy, clap::ValueEnum)] pub enum GroupBy { Model, Day, Source, Cwd }
  #[derive(Serialize)] pub struct CostRow { pub key: String, pub usage: Usage, pub billing: Option<Billing>, pub cost_usd: Option<f64> }
  #[derive(Serialize)] pub struct QuotaRow { pub provider: String, pub window_minutes: u32, pub used_percent: f64, pub resets_at_s: i64, pub observed_ms: i64, pub reset_passed: bool }
  pub fn sessions(store: &Store, cfg: &Config, since_ms: i64) -> anyhow::Result<Vec<SessionRow>>;   // sắp theo last_ms giảm dần
  pub fn cost(store: &Store, cfg: &Config, by: GroupBy, since_ms: i64) -> anyhow::Result<Vec<CostRow>>;
  pub fn quota(store: &Store, now_s: i64) -> anyhow::Result<Vec<QuotaRow>>;   // bản mới nhất mỗi (provider, window)
  pub fn turns(store: &Store, session_prefix: &str, limit: usize) -> anyhow::Result<Vec<TurnSummary>>;   // prefix khớp >1 phiên → Err nêu các id
  ```
- Quy tắc: `warning` = `"không đọc được N dòng"` khi `bad_lines>0`; = `"định dạng lạ?"` khi có turn nhưng 0 usage. `context_pct` của phiên: `ctx_window` trong CSDL, nếu None thì `cfg.model(model).context_window`. `cost_usd` = `Some` chỉ khi billing `Api` và có giá; `Subscription` → `None` (hiện `—`); không rõ → `None` (hiện `?`). `GroupBy::Day` nhóm theo ngày **UTC** (`YYYY-MM-DD`), tiêu đề cột ghi `(UTC)`; không thêm crate thời gian.
- CLI (clap): `agm scan [--days 7]` · `agm ls [--days 1]` · `agm cost [--by model|day|source|cwd] [--days 1]` · `agm quota` · `agm log <session> [--limit 50] [--full]` · cờ chung `--json`. `ls/cost/quota/log` chạy `ingest_all` (since = now − 7 ngày cho lần đầu, sau đó con trỏ làm phần còn lại) trước khi trả lời. JSON khi `!stdout().is_terminal() || --json`. `--full`: với mỗi turn, mở log gốc tại `byte_offset`, in nguyên dòng đó dạng JSON đẹp. Bảng: `comfy-table`; `quota` vẽ thanh 10 ô (`█`/`░`) + `reset HH:MM` hoặc `đã reset`. Mã thoát 0 khi thành công; lỗi → in lỗi ra stderr, mã 1. Biến `AGM_HOME`, `AGM_CODEX_DIR`, `AGM_CLAUDE_DIR` ghi đè đường dẫn (cho test).

- [ ] **Step 1: Test** `tests/report_test.rs` (nạp `mini.jsonl` hai nguồn qua `ingest_file` vào store tạm):
  - `sessions_rows`: 2 hàng; hàng Codex `context_pct==Some(24.4)`, `unknown_lines==1`; hàng Claude `context_pct==None` (default config chưa có window).
  - `cost_by_model_subscription_has_no_usd`: by Model → hàng `gpt-6.1-sol` có `billing==Some(Subscription)`, `cost_usd==None`, `usage.cache_read==13312`.
  - `quota_latest`: 2 hàng codex (300, 10080); với `now_s=1795000000` → hàng 300 `reset_passed==true`, hàng 10080 `false`.
  - `turns_ambiguous_prefix`: prefix `""` khi có 2 phiên → `Err`.
- [ ] **Step 2: Test** `tests/cli_test.rs` (chạy binary `env!("CARGO_BIN_EXE_agm")` với `AGM_HOME`, `AGM_CODEX_DIR`, `AGM_CLAUDE_DIR` trỏ vào tempdir chứa fixture):
  - `cli_ls_json`: `agm ls --json` → mảng JSON 2 phần tử, mã thoát 0.
  - `cost_json_unknown_model_null_usd`: trong test, chép `codex/mini.jsonl` và đổi model ở dòng `turn_context` thành `mystery-1` → `agm cost --by model --json` có phần tử `key=="mystery-1"`, `cost_usd==null`, `usage.output>0`.
  - `cli_quota_json`: có `used_percent==22.0`.
  - `cli_bad_config_exits_1`: `providers.toml` hỏng → mã thoát 1, stderr chứa `providers.toml`.
- [ ] **Step 3:** FAIL. **Step 4:** Viết `report.rs`, `main.rs`. **Step 5:** PASS (trừ phần phụ thuộc `cost_usd` nếu người dùng chưa viết).
- [ ] **Step 6: Kiểm trên máy thật:** `cargo run --offline -- quota` → thấy 2 cửa sổ codex khớp số trong log mới nhất; `cargo run --offline -- ls` → thấy phiên Claude hiện tại và các phiên Codex hôm nay, không có `warning`.
- [ ] **Step 7:** Commit `t7: report + agm cli (scan/ls/cost/quota/log)`.

---

## Sau kế hoạch này

Kế hoạch 2 (P2 daemon + kết nối `hc connect` + correlator + inbox) viết sau khi có `.agents/decisions/p0-research.md`. Kế hoạch 3 (TUI), Kế hoạch 4 (can thiệp, ngân sách, xoay vòng thợ OpenCode — §5.9), Kế hoạch 5 (OpenCode/agy parser, cài daemon).
