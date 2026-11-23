# tui-board-a log — Task 1 (flows: edges + pair-latest query)

Branch: feat/tui-board-a, base 8d209e1 (Task 0 scaffold).
Scope: implement `flows::edges` (`src/board/flows.rs`) and `Store::pair_latest_messages`
(`src/store/project.rs`); new tests in `tests/board_flows_test.rs`. No frozen signature
or type changed; no other worker's `todo!` touched.

## Steps
- [x] 1. Wrote 4 failing tests in `tests/board_flows_test.rs`:
  `edges_direction_agnostic_and_newest_wins`, `edges_skip_pairs_without_messages`,
  `edges_ignore_messages_to_non_parent`, `pair_latest_one_row_per_unordered_pair`.
- [x] 2. `cargo test --test board_flows_test` → FAILED, 0 passed / 4 failed, all
  `not yet implemented: Task 1` (`todo!()` in both bodies).
- [x] 3. Implemented:
  - `flows::edges`: walks `parents(agents)`; for each child with a parent, matches
    `pair_latest` rows whose ends are one `agent_keys` key of the parent and one of
    the child in either direction, and picks the max by `(ts_ms, id)`. Emits an
    `Edge` per child that has a matching row, with `kind_to_english(row.kind)`;
    no row → no edge. Added `use crate::actions::kind_to_english` only.
  - `Store::pair_latest_messages`: empty parties → empty; otherwise a single
    `ROW_NUMBER() OVER (PARTITION BY MIN(from_t,to_t), MAX(from_t,to_t) ORDER BY id DESC)`
    window query with `numbered_placeholders` reused for both `IN` clauses
    (SQLite reuses `?N`, so parties are bound once), filtered to `rn = 1`.
- [x] 4. Green gates:
  - `cargo test --test board_flows_test` → 4 passed, 0 failed.
  - `cargo test` → 463 passed, 0 failed (Task 0 baseline + these 4).
  - `cargo fmt --check` → clean.
  - `cargo clippy --all-targets -- -D warnings` → clean.
- [x] 5. Commit `feat(board): flow edges and pair-latest query`.

## Open questions
- None. The plan's "one row per unordered pair, highest id" was implemented with a
  `MIN`/`MAX` partition on the ordered string pair; no pairs are ordered by timestamp
  in the query itself because the highest `id` rule is explicit.
