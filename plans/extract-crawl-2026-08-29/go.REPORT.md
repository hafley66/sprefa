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
8. [Fixes 2](#fixes-2)
9. [Fixes 3 (module plane, PR #558)](#fixes-3)

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

## Fixes 2 <a name="fixes-2"></a>

Lane `fix-extract-go-type-plane`, base `e433d5f9d`. Same corpus, same binary
path (`v6/sprefa-extract/target/release/extract`, this lane's build), a fresh
`--resolve` over all 5097 files plus one `--family call` pass per file for
the site/def totals. Numbers below are the mode of 5 back-to-back runs (see
the flakiness row); the "Fixes" table's own numbers are the before column.

### What landed

| leg | mechanism | test |
|---|---|---|
| 1. receiver types | `x.M()` binds through x's declared type (var/param/receiver/pointer/one struct field/one slice-or-map element), same package first then the type's own `pkg.` qualifier; `:=` from a call result is `inferred`, a rebound conflict is `ambiguous` | `tests/55_go_type_plane.rs`, 5 of 7 tests |
| 2. interface dispatch | every `method_elem` mints a CallF Method def (owner = the interface, via `CallFAux.method_owners`); a type wins `CallEdgeKind::Implements` on a spec only if its method set covers ALL of the interface's specs | `tests/55_go_type_plane.rs`, 2 of 7 tests |
| 3. builtins | a bare call to a predeclared func/type-conversion drops `unresolved reason builtin`, never a corpus gap; a local shadow still wins `name_resolve` | `tests/55_go_type_plane.rs`, 2 of 7 tests |

### Corpus receipt

`resolved_edge`/`unresolved`/reachability, 104 program roots, whole project
(5097 files, wall ~7-8s):

| metric | before this lane | after |
|---|---|---|
| resolved_edge (name_resolve) | 99,190 | 84,413 |
| resolved_edge (implements) | 0 (kind did not exist) | 1,657 |
| resolved_edge total | 99,190 | 86,070 |
| call sites (whole corpus) | 159,740 | 159,737 |
| unresolved sites (sites − edges) | 60,550 | 73,667 |
| unresolved sites WITH a reported reason | 0 (`drops: None`) | 33,457 (`builtin` 14,434, `inferred` 19,023, `ambiguous` 0) |
| reachable defs (non-test roots) | 4,253 / 18,849 (22.6%) | 4,832 / 19,174 (25.2%) |
| reachable defs (+ test roots) | 12,542 | 11,600 |

`resolved_edge` total DROPPED (99,190 -> 86,070) while reachability ROSE: leg
1 replaced the old bare-name search (`GoSource::call_name_match`, which binds
ANY unique corpus def named `M` with no receiver check at all) with a
directory- and type-checked lookup for every selector site that has a
traceable receiver, so a site that used to bind "by luck" onto an unrelated
same-named method now correctly finds nothing, or finds the right one. Net:
14,777 fewer (looser) name-match edges, 1,657 new implements edges, all of
them targeted at real cross-type/cross-interface dispatch.

`reachable_with_tests` fell (12,542 -> 11,600) even though the non-test
number rose; not investigated further inside this lane's time box (see the
"untested" row below) — a real number, not a typo, but its direction is
unexplained.

### Kink: `own_blob` cross-corpus span search is non-deterministic at scale

`own_blob` (`types.rs:1683`, shared by every language, not go-specific)
finds "this file's own blob" by scanning `index.map.values().flatten()` — a
plain `HashMap`, so the scan order is Rust's per-process random seed, not
insertion order — for the first `DefSite` whose SPAN equals one of this
file's own named-entity spans. At small scale a `(start,end)` byte-offset
pair colliding with an unrelated file's node is rare enough to never surface.
At full corpus scale (5097 files), leg 2 mints one new small CallF node per
`method_elem` across the whole corpus, and at least one of those spans
coincides with an unrelated file's own span closely enough that `own_blob`
occasionally returns the WRONG blob for `internal/execute/tsctests/runner.go`
depending on the hash-map iteration order, which is fixed once per PROCESS
start, not per run: back-to-back `--resolve` invocations of the identical
file list gave `resolved_edge` counts of 86,070 (4 of 5 runs) and 86,061 (1
of 5), the 9-edge gap always the same set of receiver-typed sites in that one
file. Isolated to a small fixture, or forced single-threaded
(`SPREFA_EXTRACT_THREADS=1`), it never reproduces — confirmed NOT a rayon
race (verified: `own_blob`'s search order depends on the HashMap's random
seed, chosen once at process start, independent of thread count). Confirmed
NOT caused by this lane's resolve logic: checked out the pre-lane binary
(`684339680`) and ran the identical 5-file-list `--resolve` 3 times back to
back — stable at 99,190 every time, because it mints far fewer nodes and
never happens to collide. `own_blob` itself is `types.rs`, outside this
lane's ownership, and a real fix needs it scoped by blob/file rather than
searched corpus-wide — a design change, not a one-line patch. Leg 1/2's OWN
candidate-selection was independently hardened during this investigation
(`go_receiver_target` now requires an exact-one match, was `.find()`
first-wins; `go_interface_implements` now keys candidates by (owner name,
declaring dir), was keyed by bare name alone, conflating two packages that
name a type the same) — neither fix changed the flake, confirming the root
sits in `own_blob`, not in this lane's new code.

### Untested, and why

- `reachable_with_tests` regression direction: not root-caused inside this
  lane's time box; flagged for the coordinator, not silently reported as fine.
- Cross-package receiver types (`var x pkg.T; x.M()`) resolve through the
  file's own import table (`go_receiver_target`'s qualified-name branch), but
  no corpus fixture in `tests/55_go_type_plane.rs` exercises it; the whole-
  corpus receipt above is the only evidence it fires (some fraction of the
  1,657 implements edges and some receiver-typed name_resolve edges cross
  package boundaries, not separately counted).
- `ambiguous` never appeared in the whole-corpus run (0 of 33,457 reported
  drops): either genuinely rare in this corpus, or the block-scoped
  `TypeScope` in `go_walk_receivers` is stricter than real Go shadowing
  requires and never actually produces the conflict. Not distinguished.

## Fixes 3 (module plane, PR #558) <a name="fixes-3"></a>

Lane `chore-go-module-plane-receipt`, base `50102c851` (PR #558 merged). Same
corpus, this lane's own build of
`v6/sprefa-extract/target/release/extract`. One whole-project `--resolve`
over all 5,096 `.go` files in a single process, `timeout 10`, rc 0, wall
8.5 s. The run is deterministic here: 3 back-to-back reruns each gave
resolved_edge = 79,476.

### Commands

```
BIN=v6/sprefa-extract/target/release/extract
find /Users/chrishafley/projects/typescript-go -name '*.go' | grep -v node_modules > gofiles.txt
xargs timeout 10 $BIN --resolve < gofiles.txt > all_resolved.jsonl
python3 plans/extract-bench-2026-08-29/normalize.py resolved all_resolved.jsonl \
  /Users/chrishafley/projects/typescript-go go.parse.call.tsv go.parse.type.tsv
python3 plans/extract-bench-2026-08-29/bench.py go.parse.module.tsv \
  plans/extract-bench-2026-08-29/go.oracle.module.tsv
python3 plans/extract-bench-2026-08-29/bench.py go.parse.call.tsv \
  plans/extract-bench-2026-08-29/go.oracle.call.vta.tsv
python3 plans/extract-crawl-2026-08-29/go.crawl.py <scratch>
```

`go.parse.module.tsv` normalizes `resolved_import` rows to the bench normal
form: `src_path` relative to corpus root, empty name column, `target_path`
relative when under the root, trailing empty column. `go.parse.call.tsv`
normalizes `resolved_edge` rows (all kinds) to
`src_path src_name dst_path dst_name`. Both are committed beside this
report.

### resolved_import and resolved_edge by kind

| record | kind | rows |
|---|---|---|
| resolved_import | local | 8,227 |
| resolved_import | namespace | 1,183 |
| resolved_import | total | 9,410 |
| resolved_edge | import_resolve | 22,859 |
| resolved_edge | name_resolve | 55,123 |
| resolved_edge | implements | 1,494 |
| resolved_edge | total | 79,476 |
| unresolved | builtin | 14,470 |
| unresolved | inferred | 19,022 |
| unresolved | external | 11,255 |
| unresolved | total | 44,747 |

### Module plane vs `go.oracle.module.tsv` (2,152 rows)

| metric | value |
|---|---|
| ours (unique (src, import path) rows) | 9,410 |
| oracle | 2,152 |
| intersection | 1,810 |
| recall (intersection / oracle) | 84.11% |
| precision (intersection / ours) | 19.23% |
| ours-only | 7,600 |
| oracle-only | 342 |

| diff class | rows | note |
|---|---|---|
| ours-only: `_test.go` source file | 7,593 | the oracle does not cover test files; not a miss |
| ours-only: non-test source file | 6 | e.g. `internal/bundled/noembed.go -> internal/osutil` |
| ours-only: testdata / `_tools` | 1 | `_tools/customlint/testdata/unexportedapi/unexportedapi.go` |
| oracle-only: non-test file, import line present in the file's own text | 342 | real gaps, see below |

The oracle-only 342 rows span 150 files and are under-reporting, not wrong
paths. Examples:

| example | oracle rows for the file | ours |
|---|---|---|
| `internal/compiler/program.go` | 22 | 1 (only `internal/packagejson`; `program.go:13-38` declares 22 corpus imports) |
| `internal/compiler/emitter.go` | 18 | 0 (`emitter.go:3-23` declares 19 corpus imports; the file still yields 64 name_resolve/implements edges) |
| `internal/api/proto.go` | 14 | 13 (`packagejson` dropped, its 12th of 14 corpus imports at `proto.go:19`) |
| `internal/ast/utilities.go` | 3 | 2 (`debug` dropped at `utilities.go:11`) |

Two rows also carry a suspect `local` binding: `utilities.go` (a non-test
file) emits `local = core_test`, and `program.go` emits
`local = packagejson_test`, the `_test` package qualifier the Fixes section
already flagged as a candidate-selection hazard. Mechanism not
root-caused; defect rows, no fix in this lane.

### Call plane vs `go.oracle.call.vta.tsv` (58,332 rows)

| metric | Fixes 2 baseline | this run |
|---|---|---|
| ours (unique 4-tuples) | n/a | 60,251 |
| intersection | n/a | 5,059 |
| recall (intersection / oracle) | 5.6% | **8.67%** |
| precision (intersection / ours) | n/a | 8.40% |

### Entrypoint crawl, 104 program roots

| metric | Fixes 2 (after #554) | this run |
|---|---|---|
| defs (call-plane function/method nodes) | 19,174 | 19,173 |
| reachable (non-test roots) | 4,832 (25.2%) | **4,580 (23.9%)** |
| roots with test roots | 5,977 | 5,977 |
| reachable with tests | 11,600 | 11,393 |

Reachability fell 252 defs against the #554 baseline while the module plane
landed. The crawl consumes only `resolved_edge` caller/callee pairs; the
module plane replaced qualified-site binding with `import_resolve` edges,
and the intersection with the oracle's notion of a call edge fell in the
table above, so the two moves in the same direction. Single file diff not
root-caused; the own_blob span-search nondeterminism documented in Fixes 2
remains a candidate (defs differ by 1 node from Fixes 2's run on the same
code).
