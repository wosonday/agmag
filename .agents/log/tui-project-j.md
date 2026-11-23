# tui-project-j log

- `92a3197` fix(tui): keep handed-off status symbol and accept lowercase blocked (A)
- `c8c8f06` fix(tui): cap agent label/model and inbox source/kind widths (B)
- `606fc3f` fix(tui): disambiguate duplicate project names and show empty list (C)
- `85879b1` fix(tui): prefer live then newest terminal on tree_order name collision (D)
- `3a02bce` test(tui): extend untrusted text coverage for project render (E)
- `7b070ef` test(tui): fix clippy useless_vec in project tree tests

Gates: `cargo test` PASS, `cargo fmt --check` PASS, `cargo clippy --all-targets -- -D warnings` PASS.
Constraints: no load_project_view/compute_roots changes, no tests/project_test.rs edits.
