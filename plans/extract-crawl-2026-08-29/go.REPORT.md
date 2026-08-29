# Go entrypoint crawl report: typescript-go under sprefa-extract

Corpus: `/Users/chrishafley/projects/typescript-go` (Microsoft's TypeScript compiler, Go port), 5097 `.go` files, 374 MB on disk. Binary: `v6/sprefa-extract/target/release/extract` at worktree `crawl/extract-typescript-go` (commit cec3d5c1d). Scratch: `.../scratchpad/crawl-go`.

## TOC

1. [Step 1: per-file battery](#step-1)
2. [Step 2: whole-project --resolve](#step-2)
3. [Step 3: entrypoint crawl](#step-3)
4. [Step 4: scip comparison](#step-4)
5. [Kinks](#kinks)
6. [Untested and why](#untested-and-why)
7. [Fixes](#fixes)

## Step 1 <a name="step-1"></a>

Battery: `timeout 10 extract FILE` per file, 5097 files, `xargs -P 8`. Excluded dirs: `node_modules`, `target` (neither exists in this corpus; nothing vendored).

| metric | value |
|---|---|
| files | 5097 |
| rc != 0 | 0 |
| rc = 124 (timeout) | 0 |
| size_skip rows | 0 |
| max wall ms | 2228 (`internal/fourslash/tests/gen/codeFixClassImplementInterfaceNoTruncationProperties_test.go`, 1.5 MB, 33 lines) |
| p50 wall ms | ~30 (typical small file) |

Slowest 10:

| ms | file |
|---|---|
| 2228 | internal/fourslash/tests/gen/codeFixClassImplementInterfaceNoTruncationProperties_test.go |
| 1764 | internal/checker/checker.go |
| 439 | internal/ast/ast_generated.go |
| 435 | internal/lsp/lsproto/lsp_generated.go |
| 423 | internal/diagnostics/diagnostics_generated.go |
| 387 | internal/parser/parser.go |
| 370 | internal/fourslash/fourslash.go |
| 327 | internal/stringutil/js_case_generated.go |
| 324 | internal/ls/completions.go |
| 323 | internal/printer/printer.go |

Largest 10 by bytes: `codeFixClassImplementInterfaceNoTruncation_test.go` 4.0 MB (157 ms, 39 lines, one giant array literal), then the slowest-10 table minus its first row. Note the 4 MB file with 39 lines is faster than the 1.5 MB file with 33 lines: the slow file is one line with deeply nested composite literals, so per-byte cost tracks nesting depth.

Raw table: `go.runs.tsv` (path, rc, ms, bytes, lines).

## Step 2 <a name="step-2"></a>

One whole-project call exceeds 10 s, so resolution ran per package dir: 82 dirs with 2+ `.go` files (package granularity, `--resolve dir/*.go` each under `timeout 10`).

| metric | value |
|---|---|
| dirs run | 82, all rc 0 |
| resolved_edge | 46055 |
| resolved_type_edge | 0 |
| distinct callers | 589 |
| distinct callee paths | 484 |
| site rows (per-file battery, 5097 files) | 159740 |
| unresolved sites (sites − resolved edges) | 113685 (71.2%) |

Slowest package runs: internal/api/encoder 1444 ms, internal/contentmapper 1438 ms, _tools/customlint 1433 ms.

Unresolved-cause classification over the top-6 files by unresolved count (checker.go, printer.go, transform.go, completions.go, lsp_generated.go, nodebuilderimpl.go; 11284 unresolved sites sampled):

| class | count (sample) | example |
|---|---|---|
| pkg-qualified method/function in another package (`ast.IsStringLiteral`, `xxh3.HashString128`, `errors.New`) | 6757 | internal/printer/printer.go:194 |
| method on unknown receiver (`c.compilerOptions...IsFalseOrUnknown`) | 2652 | internal/checker/checker.go:934 |
| builtin (`make`, `append`, `len`, `panic`) | 755 | internal/ls/completions.go:51 |
| plain name that is a closure or func-valued parameter (`cleanupDiagnosticContext`, `getLineDifference`, `restoreFlags`) | 1776 | internal/transformers/declarations/transform.go:621 |
| type conversion (`float64`, `int`) | 21 | internal/ls/completions.go:57 |
| empty site (1-byte span, callee "") | 12 | internal/checker/checker.go:25693 |

The whole-project number is the headline: the parse arm's resolution universe is the supplied path list. Packages import each other heavily in this repo, so 71% of call sites have no edge.

## Step 3 <a name="step-3"></a>

Roots: `func main` under `cmd/` plus exported funcs of `internal/compiler/program.go` (104 roots). Second root set: functions in `_test.go` files (5873 extra roots). Crawl: BFS over `resolved_edge` from the step-2 dumps; defs = call-plane `node` rows with kind function/method (18849 across 5055 files; 42 files declare only interfaces/types, no function nodes). Script: `go.crawl.py`.

| plane | roots | reachable | total defs | reachability |
|---|---|---|---|---|
| parse resolved_edge | 104 | 201 | 18849 | 1.1% |
| parse resolved_edge, + test roots | 5977 | 10118 | 18849 | 53.7% |

Depth histogram (program entrypoints): depth 0: 104, 1: +45, 2: +30, 3: +16, 4: +4, 5: +2. The crawl dies inside `cmd/tsgo` and `internal/compiler` because no edge crosses a package boundary.

Top 5 unreachable by span size (all are cross-package callers, i.e. missed edges, not dead code; verified via scip reachability of the same defs):

| bytes | def |
|---|---|
| 327902 | internal/diagnostics/diagnostics_generated.go:keyToMessage (reached in scip via a map init referenced everywhere) |
| 49882 | internal/ls/completions.go:getCompletionData (LSP entry) |
| 37326 | internal/format/rules.go:getAllRules |
| 26955 | internal/checker/nodecopy.go:getExistingNodeTreeVisitor |
| 19844 | internal/ls/hover.go:getQuickInfoAndDeclarationAtLocation |

Top out-degree (parse plane): ast_generated.go:Clone 197, ast_generated.go:VisitEachChild 171, api/session.go:HandleRequest 125, checker/relater.go:structuredTypeRelatedToWorker 86, printer.go:Write 69.

## Step 4 <a name="step-4"></a>

`scip-go` install: `go install github.com/sourcegraph/scip-go/cmd/scip-go@latest` fails (`module declares its path as: github.com/scip-code/scip-go`); installed `github.com/scip-code/scip-go@latest` v0.2.7 into scratch GOBIN.

Index build: on a scratch copy of the repo, `scip-go ./...` after `go mod download` produces a 106 MB index, 5103 documents, ~4 min. Two tooling kinks first:

- `extract --family scip --scip-build --scip-timeout 540` on the copy ran past its own budget and was killed at 590 s by the outer timeout with rc 124, zero rows, and no `scip_skip` row. Indexer wrapper seems to run `scip-go .` (indexes only the root package: a 158-byte index when run directly) and the budget deadline produced no stream.
- Direct `scip-go .` indexes 1 of 210 packages (158-byte index). `./...` is required.

scip stream (`--family scip` reusing the on-disk index; 2.3 s):

| record | count |
|---|---|
| scip_def | 44140 |
| scip_name | 39244 |
| scip_ref | 132844 |
| scip_edge | 33964 |
| scip_fn_edge | 244055 |
| scip_local | 89226 |
| scip_impl | 4763 |

Reachability side by side:

| plane | roots | reachable | defs | reachability |
|---|---|---|---|---|
| parse resolved_edge | 104 | 201 | 18849 | 1.1% |
| scip_fn_edge | 198 | 22480 | 39244 | 57.3% |
| scip_fn_edge, + test roots | 6515 | 31332 | 39244 | 79.8% |

scip depth histogram peaks at depth 4 (3560 new) and decays to zero around depth 30.

Name-pair edge comparison (scip symbol normalized to bare callee name): parse 31494 distinct pairs, scip 174499, overlap 26600, scip-only 147899, parse-only 4894. Sampled 30 each:

- scip-only: overwhelmingly cross-package calls (`visitTopLevelFunctionDeclaration -> HasSyntacticModifier` into ast, `ProvideCodeLenses -> isValidImplementationsCodeLensNode`), plus scip counting non-function references as fn edges (`isTypeSubsetOfUnion -> TypeFlagsUnion`, `scheduleCleanupLocked -> opts`).
- parse-only: mostly closure callers (`closure@6236 -> GetLanguageService`, scip drops closures) and stdlib calls (`-> len`, `-> Fatal`).

## Kinks <a name="kinks"></a>

| class | count | example | owner fn | fixture |
|---|---|---|---|---|
| cross-package call unresolved (no import handling in Go resolve arm) | 113685 sites unresolved overall; 6757 pkg-qualified in 6-file sample | internal/printer/printer.go:194 `ast.IsStringLiteral` | Go resolve arm's package/import name matching (v6/sprefa-extract/src/lang/go* resolve pass) | tests/fixtures/go_findings/corpus_cross_package_call.go (0 edges, expected 1) |
| interface dispatch unresolved (methods on `interface {` types have no def; 500 interface decls in internal/*) | 4763 impl rows in scip; every interface-typed site misses | internal/lsp/lsproto (handler interfaces) | go call-arm def builder (interface method declarations emit no node) | tests/fixtures/go_findings/corpus_interface_dispatch.go (0 edges, expected edge to interface method) |
| closure / func-value call site emits 1-byte site span, no edge | 9 empty/1-byte sites in checker.go alone; 3667 lambda nodes in corpus | internal/checker/checker.go:25693 `f(u)` | go call-arm site span + closure edge lowering | tests/fixtures/go_findings/corpus_closure_call.go |
| builtin calls never resolve (`make`, `append`, `len`) | 755 in 6-file sample | internal/ls/completions.go:51 | go resolve arm (no builtin def table) | (plain-name in same fixtures dir pattern) |
| `--scip-build` ignores `--scip-timeout` (no scip_skip row, stream empty on kill) | 1 run, rc 124 at outer timeout | scratch rerun of `--family scip --scip-build` on corpus copy | scip build wrapper + deadline enforcement (budget flag plumbing) | n/a (process behavior) |
| `--scip-build` runs `scip-go .` (root package only), needs `./...` | 158-byte index vs 106 MB | direct `scip-go .` repro | scip-go invocation args in build wrapper | n/a |
| `--scip-facts` rejects a directory PATH even though PATH only selects the indexer | error on every dir invocation | `extract --scip-facts --project-root X --scip-index Y X` | CLI arg validation (files-only check applied to scip-facts mode) | n/a |
| broken-pipe panic instead of clean exit when stdout closes (`extract ... \| head`) | every piped run | package resolve run piped to head | main stdout writer (no SIGPIPE handling) | n/a |

Also recorded: `resolved_type_edge` is 0 for the whole Go corpus while the flag documents `type` as a resolve choice; either Go has no type-edge lowering (matches the "no const facet" coverage note, but the doc does not say type edges are also absent) or it is a gap.

## Untested and why <a name="untested-and-why"></a>

- `--family df` / `--family cst` / per-family line-count battery (COMMON.md step 2 shape): the crawl brief defines its own battery; per-family diffs over a 200-file sample were not run, only the `call` family needed for the crawl.
- RSS measurement (`/usr/bin/time -l` over 20 largest files): brief's crawl battery does not include it; the 10-second law held everywhere (max 2228 ms), so pressure is low.
- scip crawl with test roots beyond name-pair comparisons: test-binary symbols (`cmd/tsgo.test`) carry go-build cache paths in scip_def, so per-file unreachable tables in that plane were not computed.
- `cfg` family: Go emits `kind_role` rows per the help text, but the crawl brief does not use it; left untouched.

## Fixes <a name="fixes"></a>

Lane `fix-extract-go-imports`, base c60e5c4cc. Same corpus, same binary path;
every number is a rerun on this machine, and the reruns reproduce this
report's own 46,055 edges / 159,740 sites / 201 reachable exactly, so the
before column is the report's own.

Two METHOD corrections first, both re-measured three times:

| the report says | measured |
|---|---|
| one whole-project call exceeds 10 s | rc 0 in 3-5 s over all 5097 files, every run, before and after |
| entrypoint reachability is 1.1% | 1.1% is the 82-per-package battery, whose universe never holds the imported package. The same crawl over whole-project edges reaches 534 defs before this lane, 4,253 after. |

### What landed

| kink | before | after | test |
|---|---|---|---|
| a pkg-qualified site carries no import path | `callee_path` null on all 159,740 sites | the import path on 39,819 sites (24.9%) | `import_qualified_sites_carry_the_import_path` |
| `pkg.F()` binds the CALLER's own `F` | 122 self-edges | 0 | `an_import_qualified_call_never_binds_the_callers_own_def` |
| `pkg.F()` binds ANY package that happens to declare one `F` | 3,997 edges into a package the import does not name | 0 | `every_cross_package_call_resolves_into_the_package_the_import_names` |
| a stdlib or third-party callee binds a corpus def | 3,948 of those 3,997 | 0 | `a_call_through_an_external_import_resolves_to_nothing` |
| an ambiguous cross-package callee (`alpha.Helper3` against beta's `Helper3`) | no edge | resolves, by package directory under the `go.mod` module path | `every_cross_package_call_resolves_into_the_package_the_import_names`, 40 of 40 |
| the resolve seam carries no path | `DefSite` is blob/span/family, `ProjectCx.files` a unit struct | `IndexBag.paths`, blob -> supplied path, one additive `OnceLock` slot | whole gate, 405 passed |
| the own-blob join | scip-only | one per file, and one `go.mod` walk per file | `resolve_wall_grows_linearly_over_import_qualified_files` |

### Corpus receipt

`resolved_edge` and entrypoint reachability, 104 program roots, 18,849 defs:

| scope | | before | + own-def fix | + package join |
|---|---|---|---|---|
| whole project, 5097 files | edges | 101,556 | 101,455 | 99,190 |
| | reachable | 534 (2.8%) | 528 (2.8%) | **4,253 (22.6%)** |
| | with test roots | 12,400 | 12,392 | 12,542 |
| | wall | 4 s | 4 s | 4 s |
| 82 per-package runs | edges | 46,055 | 45,967 | 45,556 |
| | reachable | 201 (1.1%) | 200 | 199 |
| | with test roots | 10,118 | 10,106 | 10,084 |

The crawl now runs to depth 29 (before: depth 8, 534 defs), the same shape the
scip plane shows in step 4. Fewer edges, 21x the reachability: the edges that
went away were guesses into packages the import never named, and the ones that
arrived cross package boundaries the crawl needs.

### Every whole-project edge, classified by the site that minted it

| edge class | before | + own-def fix | + package join |
|---|---|---|---|
| from an unqualified site (a bare name, same package) | 70,630 | 70,630 | 70,630 |
| from a qualified site, into the package the import names | 26,929 | 26,932 | 28,560 |
| from a qualified site, into another package | 3,997 | 3,893 | **0** |
| of those, through an EXTERNAL import (stdlib or third-party, no corpus target exists) | 3,948 | 3,882 | 0 |
| total | 101,556 | 101,455 | 99,190 |

Joined on the full call-site span; a join on the start byte alone
double-counts a chained `f(x).g()`, whose two sites share one start.

### The rule

| step | source |
|---|---|
| the site's import path | phase 1, from the file's own import block: a plain spec binds its path's last segment, an alias binds the alias, `_` and `.` bind no qualifier |
| the file's module and root | `go.mod` in the nearest ancestor of the file's own path, one walk per file |
| the directory the import names | module root + (import path - module path). Not a prefix of the module path: EXTERNAL, no corpus target, no edge |
| the candidates | `corpus_defs(callee)` whose file sits in that directory, unique blob wins |
| the file's own path | `IndexBag.paths`, keyed by the blob `own_blob` already computes |

The seam slot is additive: `IndexBag` derives `Default`, so every existing
`IndexBag::default()` construction keeps compiling and every arm that ignores
the slot is byte-identical. With the slot unset (a hand-built `ProjectCx`, as
in `golden_parity.rs`) the go arm falls back to the own-blob exclusion, and
that is the configuration the scip ratchet grades.

### Out of scope, and why

| row | state |
|---|---|
| method on a known receiver (`c.compilerOptions.IsFalseOrUnknown`) | out of scope by brief; receiver typing is not a syntactic-tier fact |
| interface dispatch | out of scope by brief |
| builtins (`make`, `append`, `len`, `panic`) | the go arm emits NO `unresolved` rows at all; only ts does (`src/lang/ts.rs:1194`), so a named builtin reason has no seat on the go plane today. Report row, not a fix. |
| a `_test.go` file in the same directory as the package it tests | its defs are candidates for that package's import path, so a name only the test file declares can win. Not observed in the 28,560 corpus edges; the rule would need the package clause, which no index carries. |
| `tests/golden_parity.rs:1251` | the go scip ratchet's twin re-runs `call_name_match` with no site in hand, so it takes neither the imported nor the package leg. It grades the path-less configuration, which is a real one, but the twin is no longer the arm's exact copy. File outside this lane's ownership. |
| `tests/6_kind_vocab.rs` header | still cites 946460d75 as the golden's origin after the 1-hunk regeneration (2 gamma.go site rows). File outside this lane's ownership. |
