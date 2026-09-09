# Lane `bench-extract-go-parity` (glm53f): one go call normal form for codeql, vta and ours, then the excess set

Goal (user, 2026-08-30): our go call rows scored against BOTH codeql
(`go.codeql2.call.tsv`, 48,528 rows) and go/callgraph vta
(`go.oracle.call.vta.bare.tsv`, 55,098 rows) with ONE consistent projection,
so recall AND precision are comparable and the precision number has a story.
Today RATCHET.tsv reads go call 97.42 / 51.22 vs codeql2 and 84.42 / 50.40 vs
vta. Two facts already measured, verify them first:

1. Neither oracle contains a single row whose src_path ends in `_test.go`
   (`grep -c '_test.go' <tsv>` = 0 on both). 27,834 of our 40,130 ours-only
   rows (`plans/extract-crawl-2026-08-29/go.gaps.ours_only.tsv`) come from
   `_test.go` sources. The oracles never saw test files; scoring our test-file
   rows as false positives is a scope error, not an extractor error.
2. vta names implementers; codeql names the interface method
   (`go.GAPS.md` "Which leg takes each class", ORACLES.REPORT.md section 12).
   Ours emits both (`go-interface-fanout.FIX.BRIEF.md`). A row that is right
   under one oracle is "excess" under the other.

## First action
```
git merge --ff-only 98f9cac1fd373b4b0048f0a3fc83c5b1f2624669
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/typescript-go` (read-only). ONE process,
`--resolve --project-root`, `timeout 30`, background with a log. Read
`plans/extract-bench-2026-08-29/COMMON.md`, `normalize.py`, `bench.py`,
`v6/sprefa-extract/tests/bench/mod.rs` (`normal_form` at line 170) and
`tests/ratchet_recall.rs` before writing anything.

## Task 1: the projection, written once, applied to all three sides
Add `plans/extract-bench-2026-08-29/go.project.py` (stdlib python only):
- `--scope oracle-files`: drop every row whose src_path is not in the set of
  src_paths the oracle tsv contains at least once (this removes `_test.go`
  and any package the oracle never built, e.g. `testdata/`).
- `--closure enclosing`: a `closure@<n>` src_name maps to the enclosing named
  fn (ours already emits the mirrored row; drop the `closure@` row so it is
  not double-counted).
- `--iface method|impl|both`: for an interface call site, keep the interface
  method row (codeql shape), the per-implementer rows (vta shape), or both.
Rerun the four numbers with the projection applied to OURS ONLY (oracle
rows untouched except `--iface`), and write the table in
`plans/extract-bench-2026-08-29/GO-PARITY.REPORT.md`:

| oracle | projection | recall | precision | ours rows | oracle rows | overlap |

One row per combination (none, scope, scope+closure, scope+closure+iface=method
for codeql, =impl for vta). Numbers from `bench.py` are labelled backwards
(ORACLES.REPORT.md:583); compute recall = overlap/oracle and precision =
overlap/ours yourself and say so in the report.

## Task 2: the ratchet measures the same thing
Port the projection into `tests/bench/mod.rs` behind a `GoProjection` struct
(pub fields, one per flag) and have `tests/ratchet_recall.rs` apply
scope+closure+iface-per-oracle for the two go call rows. Add a fail-first
test in `tests/bench/` that a hand-made 6-row ours set with 2 test-file rows,
1 closure row and 1 iface row projects to the expected 3 rows. Then
`RATCHET_FORCE=1 just extract-ratchet` for the go rows ONLY (leave ts5 and
rust rows byte-identical; a ts lane owns them), and put before -> after in
the PR body with `measured_at_sha`.

## Task 3: the residual excess, classified
After projection, take `ours − vta` and `ours − codeql2`, sample 300 each
(seed 7), classify with `go_gap_classify` (extend its classes if needed:
wrong target, over-fan-out, generated file, cgo/build-tag file, method value
not call, other). Table in GO-PARITY.REPORT.md with count, two file:line
each, and the `go.rs` fn that emits the class. Fix the top class fail-first
if it is under 100 lines of change (`tests/7N_go_<name>.rs`, fixture under
`tests/fixtures/go_findings/`, HEAD failure in the header). Otherwise
write the fix brief as `plans/extract-crawl-2026-08-29/go-excess-1.FIX.BRIEF.md`.

## Ownership
`plans/extract-bench-2026-08-29/go.project.py`, `GO-PARITY.REPORT.md`,
`RATCHET.tsv` go rows only, `v6/sprefa-extract/tests/bench/mod.rs`,
`tests/ratchet_recall.rs`, `tests/7N_go_*.rs`, `src/lang/go.rs`,
`go_modules.rs`, `plans/extract-crawl-2026-08-29/go*`. NOT `src/lang/ts*`,
`rust*`, `scip*`, `src/types.rs`, `src/project.rs`, RATCHET ts5/rust rows.
No `cargo fmt` on files you do not own. Go wall must stay under 10 s
(3-run median in the PR body). Gate (`cargo test --release --features cli`)
in background with a log; wall-ratio flakes rerun 3x isolated. No file over
1 MB (sample tsvs, not whole sets). Budget 90 min; post the PR with Tasks 1-2
at 90 min.

Push `bench/extract-go-parity`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-go-parity sprefa-coordinator "go parity: PR #N, vs codeql2 r/p a/b -> c/d, vs vta a/b -> c/d, top excess class <name> n, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
