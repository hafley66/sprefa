# Lane `fix-extract-rust-type-residual` (opus): the 38.3% of type rows still missing

After #605 (RATCHET rust type 61.70 / 93.75 vs rust.oracle.type.typedecl.tsv),
rust.REPORT.md section 26 carries the class census. User priority: keep
grinding types. OPEN-PROBLEMS.md row 1 is yours; update it in your PR.

## First action
```
git merge --ff-only <BASE_SHA from spawn message>
cd v6/sprefa-extract && cargo build --release --features cli,rust-checker 2>&1 | tail -1
```

## Read before coding
rust.REPORT.md sections 26-26.7 (census, the four cut classes, residual),
src/lang/rust_type_edges.rs + rust_type_refs.rs (NEW homes after #608 —
the type walk moved out of rust.rs), rust_checker.rs seam,
tests/79_rust_type_dump.rs (the census dump pair).

## Arc 1: re-census the residual
Rerun the 79_rust_type_dump census on current HEAD; the four cut classes are
gone, so the residual composition CHANGED. Classify what remains (expect:
where-clause/trait bounds, impl trait, tuple/array elements, macro-minted
types, closure signatures, cross-crate). Commit the table (class, est rows,
5 file:line examples) as section 28 before any fix. The 9 previously
unexplained rows: explain or mark dead with a reason each.

## Arc 2+: one class per commit, fail-first, same laws as #605
- fixture + HEAD-failure receipt per class
- precision floor: 93.75 - 0.10 pt; RATCHET_BUMP only
- wall/RSS within ratchet tolerances (+15%/+10%); RSS grind out of scope
- oracle-convention mismatches: exclude with written warrant, never silently

## Ownership
src/lang/rust_type_edges.rs, rust_type_refs.rs, rust.rs + rust_modules.rs
(touchpoints), tests/79_*, tests/fixtures/rust_findings/, rust.REPORT.md,
RATCHET.tsv (bump), OPEN-PROBLEMS.md row 1. NOTHING else; another lane owns
plans/extract-bench-2026-08-29/corpus-stats/.

## Gate
`cargo test --release --features cli,rust-checker` green (3x-isolated for
flakes), `just extract-ratchet` all rows hold — COORDINATE the final ratchet:
one on the machine at a time. PR to MAIN with per-class before/after.
Done/blocked: `boop beep --no-wait --as fix-extract-rust-type-residual sprefa-coordinator "<one line>"`.
