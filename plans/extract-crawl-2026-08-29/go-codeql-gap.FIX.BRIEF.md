# Lane `fix-extract-go-codeql-gap` (opus): the go call edges codeql 2 has and we do not

Read `plans/extract-bench-2026-08-29/TOOLS.REPORT.md` "Pass 2",
`ORACLES.REPORT.md` section 14, `plans/extract-crawl-2026-08-29/go.GAPS.md`.
Against `go.oracle.call.vta.bare.tsv` (55,099 rows) codeql pass 2 reads
82.4% recall / 93.6% precision; ours, single process, 75.9%. Take the set
`(codeql2 ∩ vta) − ours`: edges two independent tools agree on and we miss.

## First action
```
git merge --ff-only e794d817bb0a34fab287e9f4c23f65c6dc166329
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/typescript-go`, `internal/**/*.go cmd/**/*.go`,
ONE process (`timeout 30`, background, log; 9.4 s is the measured wall).

## Step 1: measure (commit the tsv and a table in go.GAPS.md, new section)
`bench.py` gives the sets; write the agreed-and-missed set to
`plans/extract-crawl-2026-08-29/go.codeql_agreed_missed.tsv`. Sample 300
(seed 7), classify by what binds the callee: interface dispatch (which of
#565's paths declined it: fan-out cap, no Implements edge, receiver untyped),
method value / func-typed field, generic instantiation, embedded-struct
promoted method, closure naming (our `closure@N` vs the oracle's enclosing
fn; #563 mirrored these, so say why the row still misses), unannotated
receiver beyond one hop, stdlib (skip), other. Count, projection, two
file:line each, the go.rs fn that owns it.

## Step 2: fix the top NON-representational class, fail-first
Test in the go test file that owns the leg (`tests/6*_go_*.rs`), HEAD
failure pasted in the header, fixture under `tests/fixtures/go_*`.
Promoted methods: walk embedded fields (`go_field_types`, `go.rs:1066`)
depth 4 before the method lookup. Method values: a selector that is not
called but assigned or passed binds an edge from the enclosing fn.

## Receipt
Single-process rerun, `bench.py` against vta bare and against
`go.codeql2.call.tsv`: recall 75.9% -> n, precision, agreed-and-missed
count before -> after, wall. Gate counts.

## Ownership
`v6/sprefa-extract/src/lang/go.rs`, `go_modules.rs`, the go test files
and fixtures, `plans/extract-crawl-2026-08-29/go*`. NOT `src/types.rs`,
NOT `rust*.rs`, `ts*.rs`, NOT `plans/extract-bench-2026-08-29/` (read
only; write your tsvs under extract-crawl). No `cargo fmt` on files you
do not own. Gate `cargo test --features cli --no-fail-fast` in background
with a log; wall-ratio flakes rerun 3x isolated, say so. No file over 1 MB.

Push `fix/extract-go-codeql-gap`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-go-codeql-gap sprefa-coordinator "go codeql gap: PR #N, top class <name>, recall 75.9->x, gate a/b"`.
Laws: no em dashes, no eprintln, descriptive names, comments only for what
code cannot show, no words provenance/substrate/load-bearing/regime/refusal,
never "ground truth" (say oracle).
