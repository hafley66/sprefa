# Lane `bench-extract-scip-informed` (glm53f): how far scip gets us past codeql, measured one process

User word (2026-08-29): beat codeql (go call 82.4%, ts call 88.6% of the compiler oracles; ours after #579 and #578: go 84.42%, ts 84.88%, rust 67.56% of the
compiler oracles) using scip and dataflow. Today's scip-informed leg
(`--resolve --scip-index`, `resolved_edge kind=scip_override`) was measured
only under the chunked driver: ts 69.3% recall, rust 29.9%, go never
(ORACLES.REPORT.md sections 3 and 7; `scip_override` was 3.1% of scip's own
`scip_fn_edge` rows for ts, 0.3% rust). `scip_impl` and
`scip_relationship` are emitted (`src/scip_rows.rs`) and read by no resolve
arm.

## First action
```
git merge --ff-only 6511fa1d974e476d537acb3c6d63a2c052c0f9ab
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
scip indexes: find the existing ones first (`ls **/index.scip` under the
three corpora and `plans/extract-bench-2026-08-29/`; `scip_freshness.rs`
says how the binary locates them). Missing one: `scip-typescript index`,
`scip-go`, `rust-analyzer scip .`, each in background under
`timeout 900` with a log (the named exception to the 10-second law).

## Task A: measure, one process per corpus
For each corpus: `--resolve --family call,type` plain, and
`--resolve --family call,type --scip-index <idx>`, both `timeout 60`
(scip decode is the known cost; report the wall and, if over 10 s, the
5 s `sample` top frames). Also raw scip: `--family scip` rows
(`scip_fn_edge`, `scip_impl`, `scip_relationship`, `scip_callee_type`)
normalised to the call/type normal form. Table per language:
rows = plain / scip-informed / raw scip / codeql2 / joern2, columns =
rows, overlap with the compiler oracle, recall, precision, wall.
Then the ratio the user asked for: raw scip rows, rows our scip-informed
leg consumed (`scip_override` + any edge whose target came from scip),
rows plain resolve reaches without scip, per family per language.

## Task B: the gap between raw scip and our consumption
Take `(raw scip ∩ oracle) − scip-informed`: edges scip carries that we
drop. Classify 300 (seed 7): occurrence not joined to a site span,
symbol not mapped to a def, `scip_impl` needed (interface/trait dispatch),
`scip_relationship` needed, cross-file symbol with no `ContentId`, other.
Table with the `src/scip*.rs` fn that would take each class.

## Task C: fix the top class from B inside the scip seam
Ownership below stops at the scip seam; the resolve arms belong to three
live lanes. Fail-first test in `tests/8_scip_families_cli.rs` or a new
`tests/7N_scip_*.rs` with a fixture and a checked-in tiny `index.scip`
(under 200 KB). Receipt: recall table row before/after.

## Ownership
`v6/sprefa-extract/src/scip_rows.rs`, `src/scip*.rs`, the scip test files,
`plans/extract-bench-2026-08-29/SCIP.REPORT.md` (new) and
`*.scipinformed.*.tsv`, `*.rawscip.*.tsv`. NOT `src/project.rs`,
`src/types.rs` (the speed lane), NOT `src/lang/*`, NOT `ratchet.py`.
No `cargo fmt` on files you do not own. No file over 1 MB (indexes stay
out of git; write their paths in the report).

Push `bench/extract-scip-informed`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-scip-informed sprefa-coordinator "scip: PR #N, go plain x% informed y% raw z% codeql 82.4; ts ...; rust ...; top gap class <name>"`.
Laws: no em dashes, no eprintln, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
