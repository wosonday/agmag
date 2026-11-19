# Mandatory ESLint & CI Guardrails for All Models (Trading Static Checks for Lightweight Models)

Supplement to [AGENTS-TUTORIAL.md](../../AGENTS-TUTORIAL.md) (§2, §6), [HERDR-CORE-BOT.md](../../HERDR-CORE-BOT.md) (§1.4 `auditor.md`), and [OPENCODE-WORKERS-REVIEWER.md](OPENCODE-WORKERS-REVIEWER.md).

---

## 1. Why Strict ESLint + CI Unlocks Lightweight Models

Lightweight / free models (`worker-fast` on `:free`, local Ollama 7B–14B, `gpt-6.1-terra-fast`, `gemini-3.8-flash-low/medium`) are fast and cheap, but share **five common failure modes**:
1. Inserting `any`, `@ts-ignore`, or `// eslint-disable` to silence compiler/linter errors.
2. Writing 400-line files or 150-line deeply nested functions that bloat context for downstream agents.
3. Forgetting `await` on Promises or writing empty `catch (e) {}` blocks that swallow errors silently.
4. Leaving `console.log`, unused variables, or cross-lane/layer imports behind.

Enforcing **ESLint (`--max-warnings 0`) + `git diff` bypass checks + hard CI gates** solves this:
- **The linter catches ~80% of lightweight-model mistakes in 1 second at 0 LLM tokens**, returning exact line numbers for `worker-fast` to self-fix in its worktree.
- This lets you safely route routine coding (CRUD, UI components, API handlers, unit tests ≤ 2–3 files) to **`worker-fast` (lightweight/free models)** without polluting Reviewer or `main`.

---

## 2. Five Mandatory ESLint Rule Groups for Every Model

| Group | Mandatory ESLint Rules (`"error"`) | Lightweight-Model Failure Prevented |
|---|---|---|
| **A. File & Function Size Caps (Context Hygiene)** | `max-lines` (≤ 350 LOC), `max-lines-per-function` (≤ 70 LOC), `complexity` (≤ 10), `max-depth` (≤ 3), `max-params` (≤ 4) | Prevents "God files" / giant functions that bloat context for later agents |
| **B. No TypeScript Escape Hatches** | `@typescript-eslint/no-explicit-any`, `@typescript-eslint/ban-ts-comment` (forbids `ts-ignore`/`ts-nocheck`), `@typescript-eslint/no-unused-vars`, `@typescript-eslint/consistent-type-imports` | Stops `: any` and `// @ts-ignore` workarounds when a model struggles with types |
| **C. Async Safety** | `@typescript-eslint/no-floating-promises`, `@typescript-eslint/no-misused-promises`, `@typescript-eslint/await-thenable` | Catches missing `await` and unsafe async callbacks |
| **D. No Swallowed Errors or Debug Junk** | `no-empty` (`allowEmptyCatch: false`), `no-console` (`warn`/`error` only), `no-debugger`, `no-param-reassign`, `eqeqeq`, `prefer-const`, `no-var` | Prevents silent `catch {}` blocks and leftover `console.log` |
| **E. Architecture & Lane Boundaries** | `no-restricted-imports`, `no-duplicate-imports` | Prevents core/domain layers from importing UI layers or crossing into other lanes |

---

## 3. Reference `eslint.config.mjs` (ESLint v9 Flat Config)

Place at the root of any JS/TS project. **No agent may edit this file** except on an explicit, user-approved `harness/*` branch:

```javascript
// eslint.config.mjs — Mandatory guardrails for all models (CI fails on > 0 warnings/errors)
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
      // Group A: Size & complexity caps to keep agent context lean
      "max-lines": ["error", { max: 350, skipBlankLines: true, skipComments: true }],
      "max-lines-per-function": ["error", { max: 70, skipBlankLines: true, skipComments: true, IIFE: true }],
      "complexity": ["error", 10],
      "max-depth": ["error", 3],
      "max-params": ["error", 4],

      // Group B: No TypeScript escape hatches
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

      // Group C: Async safety
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/await-thenable": "error",

      // Group D: No swallowed errors or debug junk
      "no-empty": ["error", { allowEmptyCatch: false }],
      "no-console": ["error", { allow: ["warn", "error"] }],
      "no-debugger": "error",
      "no-param-reassign": ["error", { props: false }],
      "eqeqeq": ["error", "always"],
      "prefer-const": "error",
      "no-var": "error",

      // Group E: Architecture & import hygiene (customize patterns per repo)
      "no-duplicate-imports": "error",
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["**/../.wt/**"],
              message: "Do not import from worktree directories (.wt).",
            },
          ],
        },
      ],
    },
  }
);
```

---

## 4. Blocking `eslint-disable` / `@ts-ignore` Bypasses on `git diff`

When lightweight models hit `max-lines-per-function` or `no-explicit-any`, they often try to prepend `/* eslint-disable ... */` to make `eslint` pass. Both **Audit (`auditor.md`)** and **CI** run this diff check:

```bash
#!/usr/bin/env bash
# scripts/check-no-lint-bypass.sh — Block newly added eslint-disable / @ts-ignore / @ts-nocheck
set -euo pipefail
BASE="${1:-origin/main}"

if git diff "$BASE"...HEAD -- '*.ts' '*.tsx' '*.js' '*.jsx' | grep -E '^\+.*(eslint-disable|@ts-ignore|@ts-nocheck)'; then
  echo "CI GATE ERROR: Model added eslint-disable / @ts-ignore / @ts-nocheck in diff!" >&2
  echo "Fix the types or split the function instead of disabling rules." >&2
  exit 1
fi
```

---

## 5. Blocking GitHub Actions CI Workflow (`.github/workflows/ci-guardrails.yml`)

Any non-compliant model or bypass attempt fails CI, and **`copilot-git` rejects the merge** (`status: failed`):

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

      - name: Block ESLint config edits or eslint-disable/@ts-ignore in diff
        run: |
          BASE_REF="${{ github.event.pull_request.base.sha || 'HEAD~1' }}"
          if git diff --name-only "$BASE_REF"...HEAD | grep -E '^eslint\.config\.|\.eslintrc'; then
            echo "::error::Models may not modify ESLint configuration on feature branches!"
            exit 1
          fi
          if git diff "$BASE_REF"...HEAD -- '*.ts' '*.tsx' '*.js' '*.jsx' | grep -E '^\+.*(eslint-disable|@ts-ignore|@ts-nocheck)'; then
            echo "::error::Detected newly added eslint-disable / @ts-ignore in diff!"
            exit 1
          fi

      - name: Strict ESLint (0 warnings, 0 errors)
        run: npx eslint . --max-warnings 0 --report-unused-disable-directives

      - name: TypeScript typecheck
        run: npx tsc --noEmit

      - name: Run Unit Tests
        run: npm test
```

---

## 6. Three Enforcement Gates (Applies Analogously to Python / Rust)

1. **Gate 1 — Worker worktree (`worker-fast/mid/strong`):**
   - Before every commit, the worker must pass `npx eslint . --max-warnings 0` (or `ruff check .` for Python, `cargo clippy --all-targets -- -D warnings` for Rust). No reporting `done` while lint/tests fail.
2. **Gate 2 — `auditor` (free model):**
   - Re-runs lint + `check-no-lint-bypass.sh` + tests. Any violation or new `eslint-disable` triggers immediate **`CHƯA ĐẠT` (`FAIL`)** back to the worker **without spending Reviewer tokens**.
3. **Gate 3 — CI & `copilot-git`:**
   - If post-merge checks or CI fail on `main`, `copilot-git` aborts/`git revert`s and marks `status: failed` in `BRANCHES.md`.
