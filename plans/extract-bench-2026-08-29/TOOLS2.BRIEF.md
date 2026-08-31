# Lane `bench-extract-tools-2` (opus): second pass at joern and codeql, prove the queries hit

Read `plans/extract-bench-2026-08-29/COMMON.md`, `plans/extract-bench-2026-08-29/TOOLS.REPORT.md`. Pass 1 left three results that
say "the query missed", never "the tool cannot": codeql ts call 34 rows,
codeql go call 4,810 rows with 0 overlap against `go.oracle.call.vta.bare.tsv`,
joern go 447 methods and 1 edge on 5,097 files. Your job: make each tool see
the corpus, then compare against the compiler oracle, not against us.

## First action
```
git merge --ff-only 291a87a1153135250424034d2ff98f4fd0f0e2b9
```
Tools are installed: `codeql` (brew, 2.26.4), `/tmp/joern` (4.0.614; if
gone, reinstall per TOOLS.REPORT.md Costs table). Node 22 first on PATH for
codeql ts. Go at /usr/local/go/bin.

## Stall law
Every tool invocation runs in background with a log and a
`timeout 600`; you poll the log every 30 s, never foreground-wait. Anything
that hits 600 s is killed, its last 20 log lines go in the report, and you
move on. Pass 1 stalled 30 min inside codeql; do not repeat that.

## Task A: codeql go call, use the call graph the library has
`tools/ql/go_calls.ql` is name-based. Rewrite with
`DataFlow::CallNode.getTarget()` (semantic, resolved through go types) and
emit `src_path src_name dst_path dst_name` where names follow the
`.bare.tsv` convention (method name only, no receiver type). Receipt:
`bench.py go.codeql2.call.tsv go.oracle.call.vta.bare.tsv` must show
overlap above zero; report recall and precision of codeql against vta, and of
`go.parse.call.tsv` (ours) against vta beside it. If overlap stays 0, print
5 rows from each side for the same src file and state the naming mismatch.

## Task B: codeql ts call, resolve through the type system
`getACallee(1)` gave 0 cross-file edges. Try `CallExpr.getResolvedCallee()`
and `DataFlow::InvokeNode.getACallee()` on a db built with the corpus
tsconfig (`--command` not needed; ts extractor reads tsconfig.json so give
it `--source-root` at the repo root, not src/). Receipt: overlap of
`ts.codeql2.call.tsv` against the tsc oracle tsv named in
`ORACLES.REPORT.md` (ts call family). Same recall/precision pair as Task A.

## Task C: joern go, get past 447 methods
`gosrc2cpg` needs the module root and a Go toolchain; run it from
`/Users/chrishafley/projects/typescript-go` with `--fetch-dependencies`
off and check `cpg.method.size` first. If it stays in the hundreds, try
`joern-parse --language GOLANG` on a single package dir
(`internal/checker`) to see whether the per-file count is sane; report
both counts. Then `cpg.call.filter(_.callee.isExternal(false))` with
callee `.filename` and `.name`, same normal form. Receipt: overlap against
vta bare.

## Task D: joern ts, same as C for the 7,955 rows
115 overlap of 7,955 against ours says naming: check whether joern names
methods `<lambda>0`, `ts.foo`, or `foo` and strip to bare. Receipt:
overlap against the tsc oracle, recall/precision.

## Ownership
`plans/extract-bench-2026-08-29/go.codeql2.call.tsv`, `ts.codeql2.call.tsv`, `go.joern2.call.tsv`,
`ts.joern2.call.tsv`, `tools/ql/*`, `tools/joern/*` (new), and a new
section "Pass 2" appended to `TOOLS.REPORT.md`. Nothing else, no `src/`.

## Report shape (tables only)
| tool | family | lang | rows | overlap with oracle | recall | precision | ours recall on same oracle | wall |
Plus one table of what changed per tool (query text before and after, one
line each) and one of failures with the exact error line.

## Receipt
Push `bench/extract-tools-2`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-tools-2 sprefa-coordinator "tools pass 2: PR #N, codeql go recall x%, codeql ts x%, joern go x%, joern ts x%"`.
Laws: no em dashes, no words provenance/substrate/load-bearing/regime, never
"ground truth" (say oracle), commit the tsvs, no `--no-verify`.

## Task E: glean and kythe, read the docs and say what it takes
Pass 1 skipped both as Linux-only. Do not build them. Read
glean.software/docs (indexers: which languages ship an indexer, what
schema predicates carry call and import facts, the Angle query for each) and
kythe.io/docs (the same for go/ts/rust indexers, the `/kythe/edge/ref/call`
edge kind). Docker on this machine: `docker version` says whether the
Linux demo image route exists. Output a table: tool, language, indexer name,
call fact, import fact, run route on this Mac (docker image, or none), and
an estimated first-run wall from the docs. If docker works, run the glean
demo image on the go corpus mounted read-only under `timeout 900` in
background and report `cpg`-equivalent counts; if it does not, say so with
the exact error line.
