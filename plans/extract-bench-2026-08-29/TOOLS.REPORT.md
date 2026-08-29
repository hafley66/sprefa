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
