# Brief: sqlite_ivm materializes only nodes that need state

Repo: sqlite_ivm (+ dl8 for measurement). Run after the kernel-recursion lane merges.

User (2026-10-02): "i imagine when we declare a product type then make rules on it, its a lot of
copy nodes."

## Numbers (c15 corpus `crates/ivm-sqlite/tests/corpus/8_c15_program.json`, sqlite_ivm 5c727db)

- Install: 4499 CREATE statements, 621 ms. Tables: 1507 `…_n<node>_i` (node state), 1413
  `…_n<node>_d` (node delta), 918 indexes on them, 128 `_hold`, 37 each `_acc`/`_del`/`_nx`,
  161 `__ir_seed_n*` + 161 indexes.
- Plan: 4011 nodes: Get 1495, Join 986, Mfp 861, Mint 209, Union 172, Antijoin 148, Threshold 133,
  Reduce 7. Readers: 3675 nodes have exactly one reader, 320 none (outputs), 16 two or more.
  Single-reader by op: Get 1490, Join 986, Mfp 861, Mint 183, Antijoin 148, Reduce 7.
- Sample Mfp: `{"input": 41, "filter": [], "map": [], "project": [7, 11]}` (a pure projection).

## Goal

1. Write `plans/<date>-materialize-less.findings.md` first: which ops get `_i`/`_d` tables today
   and why (read `ivm-sqlite/src/3_nodes.rs`, the plan builder, `e39fe50` shared joins); how many
   of the 1507 are Get copies of a source table, pure projections, single-reader joins; what dl8
   lowering shape produces them (a product declaration plus rules over it: count Get/Mfp per goal).
2. Materialize only nodes whose state is required: sources, recursion (LetRec) members, reduce/
   aggregate inputs, antijoin right sides, threshold, outputs, nodes read by 2+ readers. Inline
   everything else into the reader's SQL (Get of a source reads the source table; single-reader Mfp
   and Join become a subquery/CTE in the reader's statement). Deltas follow the same rule.
3. Dedupe identical Gets (same relation) into one node in the plan builder if not already.
4. Same for ivm-dd if it arranges single-reader nodes (measure arrangement count before/after).

## Targets

- c15 install: CREATE count and install time both cut by at least half; update the pin in
  `6_install_growth.rs` with the measured number.
- dl8 `dl8 build std/tsi_registry_emit.dl7`: ir:sqlite below 2.8 s (today), byte-identical; ir:dd
  not slower than 1.35 s.
- No test weakened.

## Method, laws, validation, receipt

As `plans/v8/2026-10-02-strings.brief.md`: sqlite_ivm clone inside the sprefa worktree, branch
`perf/materialize-less`; rebuild the extension dylib from the clone (`RCARGO_OFF=1
scripts/0_build.sh release`); all edits first, `cargo check` while editing, then test; sqlite_ivm
crate tests (`cargo test -j 4 -p ivm-ir -p ivm-engine -p ivm-dd -p ivm-sqlite`, fresh `TMPDIR` for
`tests/extension.rs`) and both dl8 full suites; one cargo process at a time; no recursion; no
silent fallbacks; no `json_*(`; no merge, no push. Read `.claude/skills/sqlite-costs` in sprefa
before proposing SQLite changes.
