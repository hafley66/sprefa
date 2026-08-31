# Lane `fix-extract-ts-codeql-gap` (glm53f): the ts call edges codeql and tsc agree on that we miss

Read `plans/extract-bench-2026-08-29/TOOLS.REPORT.md` "Pass 2",
`plans/extract-crawl-2026-08-29/ts.GAPS.md`, `ts-ns-iface-destructure.RECEIPT.md`
(#578). Recall = overlap / oracle. ts call after #578: overlap 50,383 of
59,356 tsc rows = 84.88%, precision 71.16%; codeql pass 2 reads 88.6% /
98.9%. Take `(ts.codeql2.call.tsv ∩ ts5.oracle.call.tsv) − ours`, the
go #577 method.

## First action
```
git merge --ff-only 807f091546a49d4f5d831fd289573872ea1af67d
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/TypeScript-5.9`, `find src -name '*.ts' ! -name '*.d.ts'`,
ONE process, `--resolve --project-root`, `timeout 30`, background, log (2.03 s at #578).

## Step 1: measure (commit the tsv and a section in ts.GAPS.md)
`bench.py` sets; write the agreed-and-missed set to
`plans/extract-crawl-2026-08-29/ts.codeql_agreed_missed.tsv`. Classify 300
(seed 7) using the tsc TypeChecker where a text guess would do
(`plans/extract-bench-2026-08-29/oracle_ts.mjs` shows the API): receiver
typed through a union or intersection, receiver from a `this.` field,
method on a class instance created in another file, callback param typed
by the callee's signature, `namespace` member through a nested namespace,
generic instantiation, overload set, other. Count, projection, two
file:line each, the ts.rs / ts_receivers.rs fn that owns it.

## Step 2: fix the top two classes, fail-first
Tests in `tests/7N_ts_*.rs` with fixtures under `tests/fixtures/ts5_findings/`,
HEAD failure pasted in each header. Commit after each green test.

## Receipt
Single-process rerun; `bench.py` vs `ts5.oracle.call.tsv` and vs
`ts.codeql2.call.tsv`. PR body: recall 84.88% -> n, precision, agreed-and-missed
before -> after per class, wall, gate. `just extract-ratchet` green,
`RATCHET_BUMP=1` when ts rows improve.

## Ownership
`v6/sprefa-extract/src/lang/ts.rs`, `ts_receivers.rs`, `ts_resolve.rs`, ts
test files and fixtures, `plans/extract-crawl-2026-08-29/ts*`, RATCHET.tsv
ts rows. NOT `src/types.rs`, `src/project.rs`, `go*.rs`, `rust*.rs`, `scip*.rs`.
No `cargo fmt` on files you do not own. Gate in background with a log;
wall-ratio flakes rerun 3x isolated. No file over 1 MB. Budget 60 min.

Push `fix/extract-ts-codeql-gap`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-ts-codeql-gap sprefa-coordinator "ts codeql gap: PR #N, recall 84.88->x, top class <name>, gate a/b"`.
Laws: no em dashes anywhere, no eprintln, descriptive names, comments only
for what code cannot show, no words provenance/substrate/load-bearing/regime/refusal,
never "ground truth".
