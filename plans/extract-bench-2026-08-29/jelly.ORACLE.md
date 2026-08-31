# Jelly as a second ts/js call oracle (lane feat-extract-jelly-comparator, 2026-08-31)

## How it ran

| item | value |
|---|---|
| tool | `@cs-au-dk/jelly` (npm exec, no install) |
| version | 0.13.0 |
| node | v24.15.0 (jelly runs on node; Babel TS transform inside) |
| corpus | `/Users/chrishafley/projects/TypeScript-5.9`, `src/**` minus `src/lib` (tests/bench/mod.rs `wants("ts5", ..)`) |
| flags | `-b . -j <out>.json --callgraph --no-print-progress --ignore-dependencies --ignore-unresolved` |
| heap | `NODE_OPTIONS=--max-old-space-size=4096` (4 GB cap per lane brief) |
| nice / timeout | `nice -n 15`, `timeout 900` per run, background |
| files on cmdline | all chunk files passed explicitly (jelly resolves `-b .` relative to the corpus root) |

## Failure: single run over the whole corpus does not finish

One run with all 600 files as entries: node heap OOM at ~4 GB
(`FATAL ERROR: Ineffective mark-compacts near heap limit ... JavaScript heap
out of memory`, exit 134 Abort trap 6) before writing any JSON; also 16
per-file errors. Raising the heap cap would break the 4 GB RSS rule, so the
run is chunked instead.

| attempt | entries | result |
|---|---|---|
| single, 600 entries, default heap | 600 | OOM abort, no JSON (/tmp/jelly_ts5.log) |
| single, 600 entries, 4 GB heap + `--ignore-dependencies --ignore-unresolved` | 600 | OOM abort < 60 s, no JSON |
| chunk compiler (src/compiler) | 77 | ok, ~80 s, 2.4 MB JSON |
| chunk misc (13 small top dirs) | 76 | ok, 2 s |
| chunk services (src/services) | 168 | ok, 3 s |
| chunk testRunner (src/testRunner) | 279 | ok, 4 s |

## Coverage loss (jelly-side, per-file errors)

| file | error |
|---|---|
| src/compiler/checker.ts | Babel: Duplicate declaration "SymbolLinks" |
| src/compiler/debug.ts | Babel: Namespaces exporting non-const are not supported |
| src/compiler/factory/nodeFactory.ts | Babel: Duplicate declaration "SourceMapSource" |
| src/compiler/utilities.ts | Babel: Duplicate declaration (same class) |
| src/harness/harnessIO.ts | Babel transform failed |

595 of 600 corpus files analyzed. The 4 dropped compiler files are among the
highest fan-in in the corpus, so jelly's overlap numbers below are depressed
by corpus loss alone.

## Conversion

`jelly_convert.py` (this dir, stdlib only) unions the 4 chunk JSONs. Jelly's
JSON maps functions to `fileIdx:startLine:startCol:endLine:endCol` (1-based)
with no names; names are recovered from the source line at the declaration
site (`function f`, `f = function/=>`, methods, getters, `Class.constructor`
by upward class scan). A span starting at 1:1 is jelly's module
pseudo-function, named `<module>`. Rows are filtered by the ts5 file rule and
deduplicated.

| chunk | files | fun2fun rows | dropped (name-unrecoverable) |
|---|---|---|---|
| compiler | 73 | 30,679 | 4,031 |
| misc | 75 | 3,949 | 1,050 |
| services | 168 | 6,849 | 226 |
| testRunner | 279 | 7,813 | 2,072 |
| union | 595 seen | **49,290** unique | 7,379 |

Caveats: (1) 5 files unanalyzed (above); (2) name recovery is heuristic, a
failed name drops the edge; (3) chunks are separate analyses, so
cross-chunk edges exist only where one chunk's import resolution reaches the
other; (4) jelly emits module-init edges (`<module>` -> `<module>`, 2,071
rows) that inflate row counts relative to the other tools.

## Scores (`fuzzy_bench.py --mode exact`, ts5 scores raw per mod.rs)

Baseline note: "ours" here is the committed `ts5.parse.call.tsv`
(59,311 rows), which scores 70.00/70.05 vs tsc and 72.89/65.31 vs codeql2 on
this same script; the RATCHET.tsv floors (88.20/76.13) come from the live
ratchet emission at ed6079f84 and are not comparable to committed tsvs.

| pair | recall % | precision % | |rows a| | |rows b| | overlap |
|---|---|---|---|---|---|
| ours (`ts5.parse.call.tsv`) vs jelly | 38.58 | 32.06 | 59,311 | 49,290 | 19,015 |
| jelly vs tsc (`ts5.oracle.call.tsv`) | 35.69 | 42.98 | 49,290 | 59,356 | 21,186 |
| jelly vs codeql2 (`ts.codeql2.call.tsv`) | 36.79 | 39.66 | 49,290 | 53,140 | 19,550 |
| tsc vs codeql2 (context) | 98.91 | 88.56 | 59,356 | 53,140 | 52,563 |

`<module>`-named rows: ours 344, jelly 3,152, tsc 2,389, codeql2 1,628.

## Verdict: does jelly add discriminating signal beyond tsc + codeql2?

Weak. jelly-only rows (jelly minus tsc minus codeql2) = 28,096, but 3,020 are
`<module>` init edges and the named-edge remainder (25,076) is dominated by
edges into the 4 Babel-dropped files and by `src/compiler/sys.ts callback`
fan-in (jelly's natives model), which the other two oracles model differently.
10 example jelly-only rows with named src and dst:

| src_path | src_name | dst_path | dst_name |
|---|---|---|---|
| src/compiler/binder.ts | addLateBoundAssignmentDeclarationToSymbol | src/compiler/sys.ts | callback |
| src/compiler/builder.ts | arrayFrom | src/compiler/builder.ts | relativeToBuildInfo |
| src/compiler/builderPublic.ts | createAbstractBuilder | src/compiler/program.ts | getCompilerOptions |
| src/compiler/builderState.ts | addSourceFile | src/compiler/program.ts | isSourceFileDefaultLibrary |
| src/compiler/commandLineParser.ts | convertArrayLiteralExpressionToJson | src/compiler/commandLineParser.ts | forEach |
| src/compiler/core.ts | addRange | src/compiler/sys.ts | callback |
| src/compiler/emitter.ts | collectLinkedAliases | src/compiler/core.ts | notImplemented |
| src/compiler/executeCommandLine.ts | afterProgramEmitAndDiagnostics | src/compiler/builder.ts | getProgram |
| src/compiler/expressionToTypeNode.ts | findAncestor | src/compiler/factory/nodeTests.ts | isCallExpression |
| src/compiler/factory/emitHelpers.ts | createAddDisposableResourceHelper | src/compiler/sys.ts | callback |

Meanwhile jelly misses 33,021 rows that tsc and codeql2 both agree on
(`ts5.oracle` ∩ `codeql2` minus jelly), most targeting the dropped
`utilities.ts`/`checker.ts`/`debug.ts` callees. Recall ~36-38% / precision
~40-43% against each existing oracle, against 98.91% tsc-codeql2 agreement:
jelly is far noisier and far narrower on this corpus. With a 4 GB heap it
cannot analyze the corpus whole, Babel rejects 5 files (4 of them core
compiler modules), and its name-free JSON forces source-text name recovery.
As a third ts/js call oracle it adds little beyond what tsc + codeql2
already give; keep it as a documented negative result.
