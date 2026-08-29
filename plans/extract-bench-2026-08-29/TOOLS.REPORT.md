# Third-party static-analysis tools on the bench corpora (2026-08-29)

Lane `bench-extract-tools`. Corpora and normal form per `COMMON.md`. All tsvs in
`plans/extract-bench-2026-08-29/`, converters and queries in
`plans/extract-bench-2026-08-29/tools/`.

## TOC

- [TypeScript (TypeScript-5.9/src, 702 files)](#typescript)
- [Go (typescript-go, 5,097 .go files)](#go)
- [Rust (rust-analyzer, 873 files)](#rust)
- [What it took to run (install steps, wall, disk, failures)](#costs)
- [Artifacts](#artifacts)
- [Pass 2 (lane `bench-extract-tools-2`)](#pass-2)

## TypeScript

| tool | family | edges | ∩ our parse resolve | ∩ raw scip | wall | ran |
|---|---|---|---|---|---|---|
| madge | module | 2,011 | 990 of 2,099 ours | 2,009 of 2,009 scip (madge is a superset; +2) | 0.7s | yes |
| dependency-cruiser 18.2.0 | module | 2,011 | 991 | 2,009 (+2) | 3.9s | yes |
| codeql 2.26.4 | module | 1,477 | 990 | 1,477 (all ⊆ scip) | db 53s + query 12s | yes |
| codeql 2.26.4 | call | 34 | 0 of 55,611 | 1 of 59,356 | query 17s | yes |
| stack-graphs 0.4.0 | module | 1,208 | 456 | 1,208 (all ⊆ scip) | index 76s + query 9s | yes |
| joern 4.0.614 | call | 7,955 | 115 of 55,611 | 2,808 of 59,356 | parse 60s + query 20s | yes |

Notes (data, not prose):
- madge/depcruise agree on 2,010 of 2,011 edges; the one disagreement is
  `src/testRunner/parallel/host.ts -> scripts/failed-tests.d.cts` (madge only).
- madge/depcruise route through `src/compiler/_namespaces/ts.ts` re-export
  files; our parse resolve resolves direct file-to-file edges, hence 47% overlap
  with the same 2,011-edge oracle set both sides agree on.
- codeql ts call: `getACallee(1)` yields 0 cross-file edges on this
  namespace-style corpus (resolution relies on global-variable flow);
  `getACallee(2)` yields 34. Same-file edges are excluded by the query.
- stack-graphs: 703 of 1,917 import positions report `file not indexed`
  (indexer only covered tsconfig-referenced files); 1,214 resolved, 1,208
  distinct in-src edges.
- joern call rows are name-based (`cpg.call.callee`), filtered to named
  functions in .ts files, cross-file, operators and `:program` removed.

| go tooling on ts corpus | not applicable |
|---|---|

## Go

| tool | family | edges | ∩ our parse resolve | ∩ raw scip | wall | ran |
|---|---|---|---|---|---|---|
| codeql 2.26.4 | call | 4,810 | 0 of 49,082 | 0 of 172,957 (CHA) | db 60s + query 3s | yes |
| joern 4.0.614 | call | 1 | 0 | 0 | parse 300s | yes |

Notes:
- codeql go rows are name-based and pathological on this corpus: callers named
  `Error` resolve to every same-named `Error` method cross-file; the 0 overlaps
  are consistent with that noise.
- joern: `joern-parse` misdetects the repo as JSSRC (Herebyfile.mjs at root);
  forced `gosrc2cpg` directly, which produced only 447 methods and 1 cross-file
  call edge for 5,097 .go files.
- codeql go module: not emitted; Go has package-level imports, file→file does
  not apply without a per-package mapping convention.

## Rust

| tool | family | edges | ran | exact error |
|---|---|---|---|---|
| codeql 2.26.4 (rust beta) | all | 0 | no | `thread 'main' panicked at library/core/src/str/mod.rs:861:21` in `rust/tools/index-files.sh`; reproduced twice (whole repo and crates/ only) |

No other tool in the lane list ships a rust frontend (madge/depcruise ts,
stack-graphs ts, joern go/ts per run, glean/kythe skipped, below).

## Costs

| tool | install command | version | install wall | disk | ran |
|---|---|---|---|---|---|
| madge | preinstalled (`which madge`) | latest via npm | 0 | npm global | yes |
| dependency-cruiser | `npx --yes dependency-cruiser` | 18.2.0 | 2.7s (npx cache) | npm cache | yes |
| codeql | `brew install codeql` | 2.26.4 | ~60s | cask ~1GB in Caskroom | ts yes, go yes, rust no |
| stack-graphs | `cargo install tree-sitter-stack-graphs-typescript --features cli` | 0.4.0 | ~7 min, 169 crates | ~/.cargo | yes |
| joern | `curl joern-install.sh && ./joern-install.sh --install-dir=/tmp/joern --without-backend` + `brew install openjdk` | 4.0.614 | 34s script + openjdk brew | ~1GB in /tmp/joern | ts yes, go partial |
| glean | not installed | n/a | skip per docs: "You will need: Linux. The build is only tested on Linux" (glean.software/docs/building); docker demo image is Linux | n/a | no |
| kythe | release tarball downloaded and inspected | v0.0.76 | n/a | skip: shipped binaries are Linux ELF x86-64 (`file` = "ELF 64-bit LSB executable, x86-64 ... cannot execute binary file"); source build requires Bazel | no |

Environment failures encountered and worked around:
| failure | workaround |
|---|---|
| codeql ts extractor crashes with node 24 (`Maximum call stack size exceeded`, parser wrapper exit 1) | ran with node v22.23.2 first on PATH |
| `NODE_OPTIONS=--stack-size` is rejected by node ("not allowed in NODE_OPTIONS") | dropped the flag; only files under src/ extracted via `--source-root=<corpus>/src` |
| codeql go autobuild "go: executable file not found in $PATH" | go lives in /usr/local/go/bin, not /opt/homebrew/bin; added to PATH |
| joern `@main def main(...)` scripts fail to run ("Error during compilation: ScalaReplPP.main") | used `joern <cpg> --runBefore '<query>'` with piped `exit` instead |

## Artifacts

| file | content |
|---|---|
| `ts.madge.module.tsv` | 2,011 rows |
| `ts.depcruise.module.tsv` | 2,011 rows |
| `ts.codeql.module.tsv` | 1,477 rows |
| `ts.codeql.call.tsv` | 34 rows |
| `ts.stackgraphs.module.tsv` | 1,208 rows |
| `ts.joern.call.tsv` | 7,955 rows |
| `go.codeql.call.tsv` | 4,810 rows |
| `go.joern.call.tsv` | 1 row |
| `tools/madge2tsv.py` | madge json -> normal form |
| `tools/depcruise2tsv.py` | depcruise json -> normal form |
| `tools/sgdefs2tsv.py` | stack-graphs `query definition` output -> normal form |
| `tools/ts_import_positions.py` | import-specifier positions feeding stack-graphs |
| `tools/ql/ts_calls.ql`, `tools/ql/go_calls.ql` | codeql queries |

## Pass 2

Lane `bench-extract-tools-2`. Same corpora, same normal form. Every pass-1 zero
was a query defect, not a tool limit: both pass-1 call queries filtered
`caller.getFile() != callee.getFile()` while both oracles carry same-file edges
(19,324 of 55,099 vta bare rows, 38,252 of 59,356 tsc rows), and both resolved
callees by name instead of through the frontend's type system.

Oracles: go `go.oracle.call.vta.bare.tsv` (55,099 rows, `go/callgraph` vta,
receiver stripped), ts `ts5.oracle.call.tsv` (59,356 rows, TypeScript
TypeChecker). recall = overlap / oracle rows, precision = overlap / tool rows.

### Scoreboard

| tool | family | lang | rows | overlap with oracle | recall | precision | ours recall on same oracle | wall |
|---|---|---|---:|---:|---:|---:|---:|---|
| codeql 2.26.4 | call | go | 48,529 | 45,406 of 55,099 | 82.4% | 93.6% | 75.9% | query 15s, db reused |
| joern 4.0.614 | call | go | 31,617 | 19,792 of 55,099 | 35.9% | 62.6% | 75.9% | cpg 7m0s + query 40s |
| codeql 2.26.4 | call | ts | 53,140 | 52,563 of 59,356 | 88.6% | 98.9% | 70.0% | query 12s, db reused |
| joern 4.0.614 | call | ts | 24,451 | 19,182 of 59,356 | 32.3% | 78.5% | 70.0% | query 2m, cpg reused |

Ours on the same oracles, measured on the tsvs in the tree today:
`go.parse.call.tsv` 84,618 rows, 41,806 overlap, recall 75.9%, precision 49.4%;
`ts5.parse.call.tsv` 59,311 rows, 41,547 overlap, recall 70.0%, precision 70.0%.
(Section 12 of `ORACLES.REPORT.md` quotes 49,082 go rows and 45.3% recall from a
pre-#558 binary; the file in the tree now has 84,618 rows.)

### Pass 1 against pass 2, same oracle both times

| tool, lang | pass 1 rows | pass 1 overlap | pass 2 rows | pass 2 overlap |
|---|---:|---:|---:|---:|
| codeql go | 4,810 | 385 | 48,529 | 45,406 |
| codeql ts | 34 | 1 | 53,140 | 52,563 |
| joern go | 1 | 0 | 31,617 | 19,792 |
| joern ts | 7,955 | 2,808 | 24,451 | 19,182 |

### What changed per tool

| tool | before | after |
|---|---|---|
| codeql go | `callee = caller.getACall().getACallee().(FuncDef)`, cross-file only | `callee = call.getTarget().getFuncDecl()` on `DataFlow::CallNode`, same-file edges kept (`tools/ql/go/go_calls2.ql`) |
| codeql ts | `callee = invoke.getACallee(2)`, cross-file only | `callee = invoke.getResolvedCallee()` (`TypeResolution::callTarget`), caller named by nearest named enclosing function, `<module>` at top level (`tools/ql/js/ts_calls2.ql`) |
| joern go | `gosrc2cpg` on the corpus root; goastgen rejected the root `go.mod`, so only the nested `_tools` module was parsed (22 files, 447 methods) | corpus copied to `/tmp/tsgo-joern`, `tool (...)` and `ignore (...)` stripped from `go.mod`, `gosrc2cpg` rerun: 5,261 files, 34,266 methods (`tools/joern/go_calls2.sc`) |
| joern ts | `cpg.call.callee` with raw joern names (`:program`, `<lambda>N`) compared against ours | callee restricted to `isExternal == false`, names rebuilt from `method.fullName` into the oracle convention (`tools/joern/ts_calls2.sc`, `tools/joern/joern2tsv.py`) |

### Failures, exact lines

| what | exact error line | resolution |
|---|---|---|
| `gosrc2cpg` on the read-only corpus | `[WARN ] 	- failed to parse '/Users/chrishafley/projects/typescript-go/Failed': 'to generate AST for /Users/chrishafley/projects/typescript-go/go.mod '` | goastgen's go.mod parser does not accept the `tool (...)` / `ignore (...)` directives; patched copy under `/tmp/tsgo-joern` |
| `gosrc2cpg` launcher | `No java installations was detected.` | brew openjdk is keg-only: `JAVA_HOME=/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home`, `PATH=/opt/homebrew/opt/openjdk/bin:$PATH` |
| first `ts_calls2.ql` | `ERROR: getEnclosingFunction() cannot be resolved for type Functions::Function` | `Function` is a `StmtContainer`; recursed on `getEnclosingContainer().(Function)` |
| `gosrc2cpg` per-file, 4 files | `java.util.NoSuchElementException: key not found: Name` (`internal/project/watch.go`, `internal/tsoptions/parsinghelpers.go`, `internal/scanner/scanner.go`, `internal/checker/links.go`) | frontend continues; those 4 files contribute no AST |
| glean demo image | `docker: command not found` | no docker on this machine; see the glean/kythe table |

### Notes, data only

- The codeql ts database was **not** rebuilt. `/tmp/qltsdb` from pass 1
  (`sourceLocationPrefix: /Users/chrishafley/projects/TypeScript-5.9/src`)
  already carries full TypeScript extraction, and `getResolvedCallee()` returns
  53,140 edges from it. The brief's hypothesis (`--source-root` at the repo root)
  was not the cause of the 34 rows.
- codeql go rows absent from vta: 3,123. vta rows absent from codeql: 9,693. vta
  prunes by reachability from `main`/test entry points; the codeql query resolves
  every call site statically.
- 4,848 of the 31,617 joern go rows carry a `<lambda>N` caller or callee where
  the oracle spells the same function `<enclosing>$<n>`. Dropping them leaves
  26,769 rows with the same 19,792 overlap: precision 73.9%, recall unchanged.
- `go.oracle.call.cha.tsv` keeps `Type.Method` names, so it is not comparable to
  a bare-name tool row; only the vta bare tsv is used above.
- joern go per-package probe (`joern-parse --language GOLANG internal/checker`)
  was not needed: `cpg.method.size` reached 34,266 on the whole module.
- joern ts cpg (pass 1, `/tmp/cpg_ts.bin`): `cpg.method.size` 44,153,
  `cpg.file.size` 598.

### Task E: glean and kythe on this Mac, from the docs

`docker version` -> `docker: command not found`. No container route exists here,
so neither tool runs on this machine without installing Docker Desktop first.

| tool | language | indexer | call fact | import fact | run route on this Mac | first-run wall, from docs |
|---|---|---|---|---|---|---|
| glean | go | `scip-go` via `glean index go DIR --db-root DB --db NAME/INSTANCE` | none; SCIP occurrences land in `scip.Reference` / `scip.DefinitionUses` (`glean/schema/source/scip.angle`), no call predicate | none distinct; an import is a reference occurrence | none. Build is Linux-only: "Linux. The build is only tested on Linux so far" (`glean.software/docs/building`) | not stated; docs cite 6 cores / 16G as the machine that halves the build |
| glean | typescript | Sourcegraph `scip-typescript` via `glean index typescript DIR ...` | same `scip.Reference` | same | none | not stated; docs suggest `NODE_OPTIONS=--max-old-space-size=8192` for large repos |
| glean | rust | `rust-analyzer` in SCIP mode via `glean index rust-scip DIR ...` | same `scip.Reference` | same | none | not stated |
| glean | (demo) | prebuilt image `ghcr.io/facebookincubator/glean/demo:latest`, `docker run -it -p 8888:8888 ...`, ~7GB | n/a | n/a | none; docker missing, and the docs carry "The Docker image is currently not working" | 7GB pull |
| kythe | go | `go_indexer` over a kzip from the go extractor | `/kythe/edge/ref/call` (plus `/direct`, `/implicit`) | `/kythe/edge/ref/imports` | none; release binaries are Linux ELF x86-64 (pass 1 receipt), docker image `google/kythe --index` needs docker | not stated |
| kythe | typescript | `kythe/typescript` indexer, bazel-built, plugin-based | `/kythe/edge/ref/call` | `/kythe/edge/ref/imports` | none | not stated; requires a bazel build |
| kythe | rust | no shipped indexer; a `rust-project.json` extractor exists and PR #4550 adds library modules | n/a | n/a | none | n/a |
| kythe | c++ / java | `cxx_indexer`, `javac_extractor.jar` | `/kythe/edge/ref/call` | `/kythe/edge/ref/includes` (C++) | none | not stated |

The glean call-fact column is the finding to carry: glean's go, typescript and
rust facts arrive through SCIP, so they reach exactly as far as the scip index
already measured in `ORACLES.REPORT.md`, section 3. Kythe is the only one of the
two whose schema names a call edge.

### Pass 2 artifacts

| file | content |
|---|---|
| `go.codeql2.call.tsv` | 48,529 rows |
| `ts.codeql2.call.tsv` | 53,140 rows |
| `go.joern2.call.tsv` | 31,617 rows |
| `ts.joern2.call.tsv` | 24,451 rows |
| `tools/ql/go/go_calls2.ql`, `tools/ql/go/qlpack.yml` | codeql go query, semantic callee |
| `tools/ql/js/ts_calls2.ql`, `tools/ql/js/qlpack.yml` | codeql ts query, type-resolved callee |
| `tools/joern/go_calls2.sc`, `tools/joern/ts_calls2.sc` | joern queries, 6-column dump |
| `tools/joern/joern2tsv.py` | joern dump -> normal form, oracle naming |
