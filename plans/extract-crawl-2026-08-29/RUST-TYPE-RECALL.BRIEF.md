# Lane `fix-extract-rust-type-recall` (opus): rust TYPE recall 27.33% -> up, diagnosis first

RATCHET row: `rust type rust.oracle.type.typedecl.tsv 27.33 89.31` while the
call rows sit at 93.68% recall since the checker tier (#598). The type leg
barely moved (26.23 -> 27.33). User priority 2026-08-31: grind this.
Precision 89.31% says our rows are good; recall 27.33% says the oracle holds
~3.7x more type edges than we emit. Find out WHICH type edges, then take the
top classes.

## First action
```
git merge --ff-only <BASE_SHA from spawn message>
cd v6/sprefa-extract && cargo build --release --features cli,rust-checker 2>&1 | tail -1
```

## Read before coding
- `plans/extract-crawl-2026-08-29/rust.REPORT.md` sections 24-25 (checker
  tier design, the scip-informed trap, fuzzy re-measure).
- `tests/bench/mod.rs` RustProjection; `tests/ratchet_recall.rs` rust leg.
- `src/lang/rust_checker.rs` (seam), `rust_checker_ra.rs` (ra_ap loader),
  `src/lang/rust.rs` resolve_type_dst (the type leg, keyed (file, name)).
- How the oracle tsv was made: grep plans/extract-bench-2026-08-29 for the
  emitter of rust.oracle.type.typedecl.tsv before assuming its semantics.

## Arc 1: diagnosis (commit the receipt before any fix)
Regenerate ours + oracle under ratchet projection, compute oracle-minus-ours,
classify 300 sampled rows (seed 7) by SHAPE: struct field types, fn
param/return types, generic args, trait bounds, impl-block self types, type
aliases, macro-generated decls, tuple/array elements, cross-crate paths.
Commit a table (class, est rows, 5 file:line examples) into rust.REPORT.md
section 26. If a class is an ORACLE CONVENTION mismatch (they record a
relation we deliberately do not), say so and exclude it with a written
warrant, never silently.

## Arc 2+: one class per commit, fail-first
For each top class: fixture under tests/fixtures/rust_findings/, test asserts
the edge with a HEAD-failure receipt in the header, then wire the checker
tier (ra_ap type resolution answers "what type does this annotation/expr
name and where is it declared") or the syntax leg where syntax suffices.
Types stay on the same 3-answer seam as calls (Corpus/External/absent).

## Ceilings and laws
- Checker run currently 10.5 s wall / 2.5 GB RSS. Ratchet tolerances +15%
  wall / +10% rss from the committed row; do not exceed. RSS grind is
  explicitly OUT of scope (user).
- Precision must not drop below 89.31 - 0.10 pt. RATCHET_BUMP=1 only.
- `just extract-ratchet` all rows hold; `cargo test --release --features
  cli,rust-checker` green (wall-ratio flakes: 3x isolated rerun).
- No eprintln; tracing only. Comment budget: constraints only.
- Ownership: src/lang/rust*.rs, src/project.rs (checker touchpoints only),
  tests/7N_rust_*, tests/fixtures/rust_findings/, rust.REPORT.md,
  RATCHET.tsv (bump only). NOTHING else; go/ts untouched byte-identical.
- PR to MAIN with the class table + before/after recall per class. Commit
  after each green class.
- Done/blocked: `boop beep --no-wait --as fix-extract-rust-type-recall sprefa-coordinator "<one line>"`.
