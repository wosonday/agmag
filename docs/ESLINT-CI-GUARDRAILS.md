# Hàng rào ESLint & CI bắt buộc cho mọi model (Đổi kiểm tra tĩnh lấy Model nhẹ)

Tài liệu bổ sung cho [AGENTS-TUTORIAL.md](../AGENTS-TUTORIAL.md) (mục 2, 6), [HERDR-CORE-BOT.md](../HERDR-CORE-BOT.md) (mục 1.4 `auditor.md`) và [OPENCODE-WORKERS-REVIEWER.md](OPENCODE-WORKERS-REVIEWER.md).

---

## 1. Vì sao ESLint + CI cứng giúp dùng được nhiều model nhẹ hơn?

Model nhẹ / miễn phí (`worker-fast` dùng `:free`, Ollama cục bộ 7B–14B, `gpt-6.1-terra-fast`, `gemini-3.8-flash-low/medium`) rẻ và nhanh nhưng hay mắc **5 tật cố hữu**:
1. Dùng `any`, `@ts-ignore` hoặc `// eslint-disable` để "bịt miệng" trình biên dịch cho qua lỗi.
2. Viết một hàm/tệp dài hàng trăm dòng, lồng `if/for` quá sâu làm các phiên sau đọc tốn gấp đôi token.
3. Quên `await` Promise hoặc viết `catch (e) {}` nuốt lỗi im lặng.
4. Để sót `console.log`, biến thừa, hoặc import xuyên ranh giới module/lane.

Khi ép **ESLint (`--max-warnings 0`) + kiểm tra cấm tắt rule trên `git diff` + CI chặn cứng**:
- **Trình lint bắt ngay 80% lỗi của model nhẹ trong 1 giây với chi phí 0 token LLM**, trả về đúng số dòng để `worker-fast` tự sửa trong worktree.
- Nhờ đó, bạn hạ được phần lớn việc code thông thường (CRUD, component, API handler, unit test ≤ 2–3 tệp) xuống **`worker-fast` (model nhẹ/miễn phí)** mà không sợ rác lọt vào Reviewer hay nhánh `main`.

---

## 2. Năm nhóm ESLint rule bắt buộc mọi model phải tuân thủ

| Nhóm | Rule ESLint bắt buộc (`"error"`) | Chặn tật gì của model nhẹ? |
|---|---|---|
| **A. Chống phình tệp & hàm (bảo vệ ngữ cảnh)** | `max-lines` (≤ 350 dòng code), `max-lines-per-function` (≤ 70 dòng), `complexity` (≤ 10), `max-depth` (≤ 3), `max-params` (≤ 4) | Chặn đẻ "God file" / hàm dài lê thê làm phình ngữ cảnh của các agent đọc sau |
| **B. Chống lách kiểu TypeScript** | `@typescript-eslint/no-explicit-any`, `@typescript-eslint/ban-ts-comment` (cấm `ts-ignore`/`ts-nocheck`), `@typescript-eslint/no-unused-vars`, `@typescript-eslint/consistent-type-imports` | Chặn thói quen gán `: any` hoặc thêm `// @ts-ignore` khi model nhẹ không giải được kiểu |
| **C. Chống lỗi bất đồng bộ ngầm** | `@typescript-eslint/no-floating-promises`, `@typescript-eslint/no-misused-promises`, `@typescript-eslint/await-thenable` | Chặn gọi hàm `async` mà quên `await` hoặc đặt sai trong callback |
| **D. Chống nuốt lỗi & rác debug** | `no-empty` (`allowEmptyCatch: false`), `no-console` (chỉ cho `warn`/`error`), `no-debugger`, `no-param-reassign`, `eqeqeq`, `prefer-const`, `no-var` | Chặn `catch {}` rỗng làm mất dấu lỗi runtime và `console.log` rác |
| **E. Bảo vệ kiến trúc & ranh giới lane** | `no-restricted-imports`, `no-duplicate-imports` | Chặn tầng lõi (`core`/`domain`) import ngược tầng giao diện (`ui`/`web`) hoặc lấn sang nội bộ lane khác |

---

## 3. Cấu hình mẫu `eslint.config.mjs` (ESLint v9 Flat Config)

Đặt ở gốc dự án JS/TS. **Cấm mọi agent sửa tệp này** trừ khi có đầu việc riêng thuộc nhánh `harness/*` đã được người dùng duyệt:

```javascript
// eslint.config.mjs — Hàng rào bắt buộc cho mọi model (CI chặn nếu > 0 warning/error)
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "build/**", "coverage/**", ".wt/**", ".agents/**"],
  },
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    linterOptions: {
      reportUnusedDisableDirectives: "error",
    },
    rules: {
      // Nhóm A: Giới hạn kích thước & độ phức tạp để giữ sạch ngữ cảnh
      "max-lines": ["error", { max: 350, skipBlankLines: true, skipComments: true }],
      "max-lines-per-function": ["error", { max: 70, skipBlankLines: true, skipComments: true, IIFE: true }],
      "complexity": ["error", 10],
      "max-depth": ["error", 3],
      "max-params": ["error", 4],

      // Nhóm B: Cấm lách kiểu TypeScript
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/ban-ts-comment": [
        "error",
        {
          "ts-ignore": true,
          "ts-nocheck": true,
          "ts-check": false,
          "ts-expect-error": "allow-with-description",
          minimumDescriptionLength: 12,
        },
      ],
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/consistent-type-imports": ["error", { prefer: "type-imports" }],

      // Nhóm C: Bất đồng bộ an toàn
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/await-thenable": "error",

      // Nhóm D: Chống nuốt lỗi & rác debug
      "no-empty": ["error", { allowEmptyCatch: false }],
      "no-console": ["error", { allow: ["warn", "error"] }],
      "no-debugger": "error",
      "no-param-reassign": ["error", { props: false }],
      "eqeqeq": ["error", "always"],
      "prefer-const": "error",
      "no-var": "error",

      // Nhóm E: Kiến trúc & import (tùy chỉnh patterns theo dự án)
      "no-duplicate-imports": "error",
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["**/../.wt/**"],
              message: "Không được import từ thư mục worktree (.wt).",
            },
          ],
        },
      ],
    },
  }
);
```

---

## 4. Chặn model tự chèn `eslint-disable` / `@ts-ignore` để vượt rào

Model nhẹ khi gặp lỗi `max-lines-per-function` hoặc `no-explicit-any` rất hay tự chèn `/* eslint-disable ... */` lên đầu tệp để lệnh `eslint` báo xanh. Vì vậy, ngoài `eslint --max-warnings 0`, cổng **Audit (`auditor.md`)** và **CI** chạy thêm lệnh kiểm tra trên `git diff`:

```bash
#!/usr/bin/env bash
# scripts/check-no-lint-bypass.sh — Chặn mọi dòng mới thêm eslint-disable / @ts-ignore / @ts-nocheck
set -euo pipefail
BASE="${1:-origin/main}"

if git diff "$BASE"...HEAD -- '*.ts' '*.tsx' '*.js' '*.jsx' | grep -E '^\+.*(eslint-disable|@ts-ignore|@ts-nocheck)'; then
  echo "LỖI CỔNG CI: Phát hiện model tự thêm eslint-disable / @ts-ignore / @ts-nocheck trong diff!" >&2
  echo "Hãy sửa đúng kiểu dữ liệu hoặc tách nhỏ hàm thay vì tắt rule." >&2
  exit 1
fi
```

---

## 5. Cấu hình GitHub Actions CI chặn cứng (`.github/workflows/ci-guardrails.yml`)

Nếu model nào không tuân thủ hoặc cố lách rule, CI báo đỏ và **`copilot-git` từ chối merge** (`status: failed`):

```yaml
name: CI Guardrails (Model Compliance)

on:
  pull_request:
  push:
    branches: [main, "feat/**", "**/*"]

jobs:
  lint-and-verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm

      - run: npm ci

      - name: Chặn model sửa cấu hình ESLint hoặc chèn eslint-disable/@ts-ignore
        run: |
          BASE_REF="${{ github.event.pull_request.base.sha || 'HEAD~1' }}"
          if git diff --name-only "$BASE_REF"...HEAD | grep -E '^eslint\.config\.|\.eslintrc'; then
            echo "::error::Model không được tự ý sửa cấu hình ESLint trên nhánh tính năng!"
            exit 1
          fi
          if git diff "$BASE_REF"...HEAD -- '*.ts' '*.tsx' '*.js' '*.jsx' | grep -E '^\+.*(eslint-disable|@ts-ignore|@ts-nocheck)'; then
            echo "::error::Phát hiện eslint-disable / @ts-ignore mới trong diff!"
            exit 1
          fi

      - name: ESLint nghiêm ngặt (0 warning, 0 error)
        run: npx eslint . --max-warnings 0 --report-unused-disable-directives

      - name: Kiểm tra kiểu TypeScript
        run: npx tsc --noEmit

      - name: Chạy Unit Test
        run: npm test
```

---

## 6. Ba chốt chặn trong luồng làm việc (tương ứng cho cả Python / Rust)

1. **Chốt 1 — Tại worktree của thợ (`worker-fast/mid/strong`):**
   - Trước mỗi commit, thợ bắt buộc chạy `npx eslint . --max-warnings 0` (hoặc `ruff check .` với Python, `cargo clippy --all-targets -- -D warnings` với Rust). Còn lỗi thì tự sửa ngay, không được gửi báo cáo `xong`.
2. **Chốt 2 — Tại `auditor` (model miễn phí):**
   - Chạy lại lệnh lint + `check-no-lint-bypass.sh` + test. Nếu phát hiện lỗi hoặc có `eslint-disable` mới → đánh **`CHƯA ĐẠT` ngay lập tức**, trả thẳng về thợ mà **không tốn token gọi Reviewer**.
3. **Chốt 3 — Tại CI & `copilot-git`:**
   - Sau khi merge thử trên checkout chính, nếu cổng lint/CI lỗi → `copilot-git` hủy tích hợp / `git revert` và đánh dấu `status: failed` trong `BRANCHES.md`.
