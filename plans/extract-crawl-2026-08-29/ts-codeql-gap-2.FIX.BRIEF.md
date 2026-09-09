# Lane `fix-extract-ts-codeql-gap-2` (glm53f): the ts call edges codeql and tsc agree on that we miss, continued

Recall = overlap / oracle, oracle = `plans/extract-bench-2026-08-29/ts5.oracle.call.tsv`
(tsc). Ours reads 84.88% recall / 71.16% precision (RATCHET.tsv row
`ts5 call ts5.oracle.call.tsv`); codeql pass 2 reads 88.6% / 98.9% against the
same tsc oracle. Target: close the 3.7 pt gap. The prior lane measured and
classified, then was stopped mid-fix. Its work is on
`origin/fix/extract-ts-codeql-gap` (2 commits): the agreed-and-missed set
(5,463 rows, `plans/extract-crawl-2026-08-29/ts.codeql_agreed_missed.tsv`), a
300-sample classification (`ts.codeql_agreed_missed.sample300.classes.tsv`,
class in the second-to-last column), and a WIP diff in `src/lang/ts.rs` plus
`tests/74_ts_property_arrow.rs` (property-named lambdas:
`getAllCodeActions: context => ...` names the enclosing callable by the
property). Class counts from that sample:

| class | n |
|---|---|
| other: bare call, local function | 61 |
| bare call, callee imported | 46 |
| method on a class instance created in another file | 39 |
| interface receiver (signature in another file) | 36 |
| concrete class receiver, method in another file | 26 |
| receiver from a `this.` field | 21 |
| receiver typed through a union or intersection | 19 |
| namespace member through a (nested) namespace | 16 |
| other: constructor call on a local class | 15 |

## First action
```
git merge --ff-only aa95c0ef361e305f362005b09d0fbabaa75afca7
git merge --no-edit origin/fix/extract-ts-codeql-gap
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
cargo test --release --features cli --test 74_ts_property_arrow 2>&1 | tail -3
```
Corpus `/Users/chrishafley/projects/TypeScript-5.9`, files
`find src -name '*.ts' ! -name '*.d.ts'`, ONE process, `--resolve --project-root`,
`timeout 30`, background with a log. Read
`plans/extract-bench-2026-08-29/COMMON.md` for the exact invocation and
`bench.py` usage; `plans/extract-crawl-2026-08-29/ts.GAPS.md` for prior fixes.

## Step 1: finish the WIP (property-named lambdas)
Make `tests/74_ts_property_arrow.rs` pass (HEAD failure text pasted in its
header). Commit.

## Step 2: the two "bare call" classes (107 of 300)
"bare call, local function" and "bare call, callee imported" are 36% of the
gap and the oracle names them the SAME way codeql does. Pick 20 rows of each
from the sample tsv, open the src file:line, and find what our resolver does
with the site (run extract on that one file with `--resolve` and grep the
`unresolved` row and its reason). Write the reason histogram into
`ts.GAPS.md` section "gap 2: bare calls". Fix the top reason fail-first:
fixture under `tests/fixtures/ts5_findings/<name>/`, test `tests/7N_ts_<name>.rs`
with HEAD failure in the header. Commit after each green test. Then the next
class by count (method on a class instance created in another file) the same way.

## Receipt (mandatory before PR)
Single-process rerun, `bench.py` vs `ts5.oracle.call.tsv` and vs
`ts.codeql2.call.tsv`, 3 runs. Then `cd v6 && just extract-ratchet` in
background with a log; `RATCHET_BUMP=1` on the ts5 rows only when they
improve. PR body table: recall/precision before -> after per oracle,
agreed-and-missed count before -> after per class touched, wall, gate summary.
LABEL HAZARD: `bench.py` prints recall and precision swapped in one of its
tables; the ratchet test output is canonical, copy numbers from there.

## Ownership
`v6/sprefa-extract/src/lang/ts.rs`, `ts_receivers.rs`, `ts_resolve.rs`, ts
test files and fixtures, `plans/extract-crawl-2026-08-29/ts*`, RATCHET.tsv
ts5 rows only. NOT `src/types.rs`, `src/project.rs`, `go*.rs`, `rust*.rs`,
`scip*.rs`, `tests/6_kind_vocab.rs`, `tests/45_emit_throughput.rs` (a scip
lane is live on those). No `cargo fmt` on files you do not own. Gate
(`cargo test --release --features cli`) in background with a log; wall-ratio
flakes rerun 3x isolated. No file over 1 MB. Budget 90 min; at 90 min post the
PR with what is green.

Push `fix/extract-ts-codeql-gap-2`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-ts-codeql-gap-2 sprefa-coordinator "ts codeql gap 2: PR #N, recall 84.88->x vs tsc, y vs codeql2, classes fixed <names>, gate a/b"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
