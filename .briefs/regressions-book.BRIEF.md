# regressions-book: fix two bisected _22_book regressions (fix approved by user 2026-09-26)

Full bodies: issuectl show regression-demand-narrowing; issuectl show regression-module-graph.

1. 54012b05c ("eval: demand only where a rule needs bound head args, one memo per round"):
   book/src/probes/22_curry_prelude_name.dl7 expects rc=0, exits 1 missing_derived_edge_label.
   book/src/modules/2_the_return_column.md fails: jq: null (null) has no keys.
   Constraint: tests/_24_demand_count.rs stays under its bound (fixture sum < 500).
2. 9bf3eec69 ("feat(modules): steps 6 and 7, the module graph is prelude rules"):
   book/src/probes/12_relation_named_accounts.dl7 exits 0, expected duplicate_relation_name.
   book/src/modules/0_a_module_is_a_file.md prints module(file(...)) instead of accounts / consumer.
   book/src/modules/1_names.md prelude alias query prints no rows for owner accounts.

Fix the engine/compiler, not the pages. One commit per regression.
Acceptance: the five pages pass in tests/_22_book; tests/_24_demand_count green; oracle tests _0.._8 and _28 _29 green; remove these rows from .github/CI-KNOWN-RED.md.
Validation command in Boop-Check. Final commit Boop-Status done.

## Laws
- Work only in $PWD (your worktree). Never cd to a primary checkout. Never push. Never commit on main.
- RAM: CARGO_BUILD_JOBS=4, cargo -j 4, RUST_TEST_THREADS=4, CARGO_INCREMENTAL=0. Stop if your process tree exceeds 4GB RSS.
- CARGO_TARGET_DIR=$HOME/.cache/boop/lanes/<your-lane-id>/target. Never $PWD/target.
- Never edit expected/golden/fixture files or flip probe rc to make a test pass. If an expectation looks wrong, commit with Boop-Status blocked + Boop-Ask.
- Commit trailers: Boop-Status wip|done|blocked, Boop-Check "<cmd> -> <result>".
- Final message receipt: status, sha, files, validation, next.

