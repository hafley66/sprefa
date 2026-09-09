# Lane `chore-extract-cleanup-lang` (glm53f): cleanup pass 1, src/lang after a week of grind

User 2026-08-31: two cleanup passes, use `extract move` where useful (it is
this repo's own symbol-relocation tool; `extract move --help` from
v6/sprefa-extract after `cargo build --release --features cli`). This pass
owns src/lang; NO behavior change, provable.

## First action
```
git merge --ff-only <BASE_SHA from spawn message>
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
cargo test --release --features cli 2>&1 | tail -3   # green BEFORE touching anything
```

## Ownership
- v6/sprefa-extract/src/lang/** and src/lib.rs / src/lang/mod.rs wiring only.
- FORBIDDEN: src/project.rs, src/types.rs, tests/** (except updating a
  `use` path a move forces), plans/**, RATCHET.tsv. Another lane owns
  plans/. If a cleanup wants to cross into forbidden files, note it in the
  PR body and skip it.

## The work, in priority order (commit per item, stop at ~6 commits)
Sizes today: ts.rs 3,821 / rust.rs 3,215 / go.rs 2,187 / kotlin.rs 1,833
lines. rust.rs absorbed a week of arcs (#584-#605) and is the churn hotspot.
1. rust.rs: extract cohesive units into siblings (candidates: the resolve
   legs, the type-leg candidates walk, call_drops/classification). Use
   `extract move` for each relocation so imports rewrite mechanically;
   verify each with `cargo build` + the rust test files.
2. Dead code: `cargo +nightly udeps` if available else `cargo build` warnings
   + grep for `#[allow(dead_code)]`; delete what nothing references.
3. Comment budget (repo law): comments state only constraints code cannot
   show. Sweep src/lang for change-log narrative, arc references, restating
   comments; delete them. Keep TEST-header receipts and @-waivers.
4. eprintln check: `grep -rn "eprintln!" src/` must stay empty (tracing only,
   @eprintln-ok waivers stay).
5. Naming: single-letter locals in touched functions get descriptive names;
   only in functions you already touched.

## Proof of no behavior change (the receipt, non-negotiable)
- `cargo test --release --features cli` green before AND after (flakes 3x
  isolated).
- `just extract-ratchet` all rows hold, run ONCE at the end, nice -n 15,
  never while another heavy process runs.
- Golden parity: the goldens the suite checks must pass byte-identical
  (tests/golden_parity.rs); if a golden changes, the cleanup changed
  behavior — revert that commit.

## Laws
nice -n 15 everything; one build at a time (a second lane is live).
PR to MAIN, body lists per-commit what moved and the line-count delta.
Done/blocked: `boop beep --no-wait --as chore-extract-cleanup-lang sprefa-coordinator "<one line>"`.
