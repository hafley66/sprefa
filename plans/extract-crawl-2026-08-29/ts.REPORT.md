# sprefa-extract over microsoft/TypeScript: entrypoint crawl

Binary `v6/sprefa-extract/target/release/extract` at base `cec3d5c1d`.
Corpus `/Users/chrishafley/projects/TypeScript` @ `9a8581c3`, read-only.

## Contents

1. [Corpus: the brief's entrypoints do not exist](#1-corpus-the-briefs-entrypoints-do-not-exist)
2. [Build: `--features cli` does not compile in this worktree](#2-build---features-cli-does-not-compile-in-this-worktree)
3. [Step 1: per-file battery](#3-step-1-per-file-battery)
4. [Step 2: whole-project `--resolve`](#4-step-2-whole-project---resolve)
5. [Step 3: entrypoint crawl](#5-step-3-entrypoint-crawl)
6. [Step 4: scip comparison](#6-step-4-scip-comparison)
7. [Step 5: kinks](#7-step-5-kinks)
8. [What stays untested and why](#8-what-stays-untested-and-why)

---

## 1. Corpus: the brief's entrypoints do not exist

`microsoft/TypeScript` main is now the Go-native port. The repository has no
`src/` tree, so `src/tsc/tsc.ts`, `src/tsserver/server.ts`,
`src/typescript/typescript.ts` and `src/compiler/program.ts` are all absent.
The compiler and tsserver mains are Go (`5102` `.go` files). The TypeScript
that remains is the npm package that talks to the Go binary, the VS Code
extension, and the code generators.

| brief entrypoint | corpus reality | substitution used |
|---|---|---|
| `src/tsc/tsc.ts` | absent, Go | none |
| `src/tsserver/server.ts` | absent, Go | none |
| `src/typescript/typescript.ts` exports | absent | the 11 source entries in the `exports` map of `packages/typescript/package.json` |
| `export function` in `src/compiler/program.ts` | absent | every exported callable plus every method of an exported class in those 11 modules |
| `src/testRunner/**` | absent | `packages/*/test/**`, `tools/scripts/tsc/*.ts`, `packages/typescript/scripts/generateSync.ts` |

Root set A expands the 11 entries one level through `export * from` to 18
modules. Root set B is the 27 test and generator files. Both root sets are
derived by `ts.crawl.py`, not hand-written.

### File inventory

| set | files | note |
|---|---|---|
| program | 271 | `.ts/.tsx/.mts/.cts`, excluding `testdata/`, `vendor/` (20 files), `.git` |
| testdata | 12967 | `tsc/testdata/**`, the parser stress corpus |
| javascript | 6 | `.js/.mjs/.cjs`, not measured further |

`node_modules` does not exist in the checkout, so no exclusion was needed for it.

| program subdirectory | files |
|---|---|
| `tsc/internal/bundled/libs` | 108 |
| `packages/typescript/src` | 108 |
| `packages/vscode-typescript/src` | 22 |
| `packages/typescript/test` | 18 |
| `tools/scripts/tsc` | 5 |
| `tsc/internal/lsp` | 3 |
| `packages/vscode-typescript/test` | 3 |
| `packages/typescript/lib` | 2 |
| `tsc/internal/stringutil` | 1 |
| `packages/typescript/scripts` | 1 |

## 2. Build: `--features cli` does not compile in this worktree

```
cargo build --release --features cli
error: failed to load manifest for workspace member `v6/sprefa-extract/.`
Caused by: failed to load manifest for dependency `hafley-observe`
Caused by: failed to read
  `/Users/chrishafley/projects/sprefa/.boop-worktrees/crawl/hafley-rs/crates/hafley-observe/Cargo.toml`
Caused by: No such file or directory (os error 2)
```

`Cargo.toml:166` points `hafley-observe` at `../../../hafley-rs/crates/hafley-observe`,
which resolves to `.boop-worktrees/crawl/hafley-rs`. That sibling checkout does
not exist for the `crawl/` lane kind; eleven other lane kinds have it. The
sibling lanes `crawl/extract-typescript-go` and `crawl/extract-rust-analyzer`
carry the same broken path and the same prebuilt binary.

Every measurement below uses the binary boop-start placed in this worktree
(`target/release/extract`, 55423440 B, 2026-08-28 23:59, cache key
`1b94595a6cae7899`). This lane changes no `src/**` file, so the binary matches
the tree.

## 3. Step 1: per-file battery

One process per file, `timeout 10`, eight at a time.
Raw table: `ts.runs.tsv` (13238 rows, `ms_parallel8` is wall time under 8-way
parallelism and is inflated 3x to 8x against a serial run; section 3.3 remeasures).

### 3.1 Outcomes

| set | files | rc=0 | rc!=0 | rc=124 | `size_skip` | non-empty stderr | zero fact lines |
|---|---|---|---|---|---|---|---|
| program | 271 | 271 | 0 | 0 | 0 | 0 | 0 |
| testdata | 12967 | 12967 | 0 | 0 | 0 | 0 | 8 |

No crash, no timeout, no error line anywhere in 13238 files. The largest input
is 3151774 B against the 16777216 B ceiling, so `--max-bytes` never fired.

The 8 zero-line files account exactly: 7 are UTF-16 (BOM `FF FE` or `FE FF`),
1 is invalid UTF-8. Both are silent, both exit 0. A UTF-8 BOM is handled
correctly (facts begin at byte offset 3).

| path | bytes | encoding |
|---|---|---|
| `tsc/testdata/tests/cases/compiler/bom-utf16be.ts` | 24 | UTF-16BE |
| `tsc/testdata/tests/cases/compiler/bom-utf16le.ts` | 24 | UTF-16LE |
| `tsc/testdata/tests/cases/compiler/collisionCodeGenModuleWithUnicodeNames.ts` | 724 | UTF-16LE |
| `tsc/testdata/tests/cases/compiler/instanceofOperator.ts` | 1236 | UTF-16LE |
| `tsc/testdata/tests/cases/compiler/promiseTest.ts` | 566 | UTF-16LE |
| `tsc/testdata/tests/cases/compiler/targetTypeBaseCalls.ts` | 826 | UTF-16LE |
| `tsc/testdata/tests/cases/compiler/unicodeIdentifierNames.ts` | 674 | UTF-16LE |
| `tsc/testdata/tests/cases/compiler/regexInvalidUtf8WithUnicodeFlag.ts` | 24 | invalid UTF-8 |

### 3.2 Largest inputs and max RSS

Twenty largest files under `/usr/bin/time -l`, full table in `ts.rss.tsv`.

| path | bytes | max RSS bytes | RSS / bytes |
|---|---|---|---|
| `tsc/testdata/fixtures/compiler/checker.ts` | 3151774 | 103546880 | 32.9 |
| `tsc/internal/bundled/libs/lib.dom.d.ts` | 2349483 | 48840704 | 20.8 |
| `tsc/testdata/fixtures/lib/dom.generated.d.ts` | 2348669 | 47366144 | 20.2 |
| `tsc/internal/bundled/libs/lib.webworker.d.ts` | 787076 | 23117824 | 29.4 |
| `tsc/testdata/fixtures/compiler/parser.ts` | 539685 | 27983872 | 51.9 |
| `packages/typescript/src/api/sync/api.ts` | 274167 | 20496384 | 74.8 |

Peak is 103546880 B on a 3.15 MB file. No RSS finding.

### 3.3 Throughput

Percentiles over the 865 files of at least 2000 B, from the parallel battery:
p5 = 42 B/ms, p50 = 127 B/ms, p95 = 1059 B/ms, max = 5138 B/ms.

Serial remeasure, three runs each, median reported. Process floor is 4 ms
(`extract /tmp/empty.ts`, median of seven), so these are parse cost.

| path | bytes | median ms | B/ms |
|---|---|---|---|
| `tsc/testdata/fixtures/compiler/checker.ts` | 3151774 | 418 | 7538 |
| `packages/typescript/src/api/sync/api.ts` | 274167 | 64 | 4281 |
| `tsc/testdata/tests/cases/compiler/largeControlFlowGraph.ts` | 140212 | 80 | 1761 |
| `tsc/testdata/tests/cases/compiler/binderBinaryExpressionStress.ts` | 39935 | 212 | 189 |
| `tsc/testdata/tests/cases/compiler/binderBinaryExpressionStressJs.ts` | 39969 | 213 | 188 |

The two `binderBinaryExpressionStress` files are one deeply nested binary
expression chain each and cost 40x per byte against the largest normal file.
212 ms is far inside the 10-second law; this is a shape to know, not a defect.

## 4. Step 2: whole-project `--resolve`

One call over all 271 program files, no split needed.

```
extract --resolve --family call,type $(cat prog.list)
rc=0   real 1.926s
```

| measure | count |
|---|---|
| `resolved_edge` | 8025 |
| `resolved_type_edge` | 9298 |
| distinct callers | 2893 |
| distinct callees | 1489 |
| same-file edges | 5819 |
| cross-file edges | 2206 |
| `site` rows (per-file `--family call`) | 20289 |
| unresolved sites (`site` with no edge at its span) | 12264 (60.4%) |
| `unresolved` records | 46 |
| call-plane defs, kind function or method | 2659 spans, 2517 distinct (path, name) |
| call-plane defs, kind lambda | 2425, every one `name: null` |

Edges are joined to sites on the exact `(caller_path, caller_site_start,
caller_site_end)` triple, so the 8025 and 12264 partition the 20289 sites with
no double counting.

### 4.1 Unresolved sites by cause

| bucket | sites | share |
|---|---|---|
| callee name has no def in the resolution universe | 6477 | 52.8% |
| name defined in exactly 2 corpus files, dropped as ambiguous | 3521 | 28.7% |
| name defined in 3 or more corpus files, dropped as ambiguous | 1672 | 13.6% |
| site has no covering call def, so no caller exists | 413 | 3.4% |
| name unique to one file and still unresolved | 181 | 1.5% |

The ambiguity buckets are the documented diet trade (`extract --help`, FAST
MODE). The 413 with no covering def are the kink in section 7.1. The 181 are
`has`, `set` and friends whose only function-or-method def is
`packages/typescript/src/api/sourceFileCache.ts`; they carry a type-plane def
elsewhere that the def index counts and this table does not.

Breakdown of the 6477 with no def in the universe:

| sub-cause | sites | share | example |
|---|---|---|---|
| bare name, no import row, no corpus def | 3640 | 56.2% | `replaceAll`, `log` |
| JS builtin prototype method | 1455 | 22.5% | `replace`, `split`, `trim` |
| imported from a package outside the corpus | 750 | 11.6% | `node:path` `join`, `node:fs` `readFileSync` |
| JS global | 629 | 9.7% | `Set`, `Map` |
| imported from a corpus-relative module, still no def | 3 | 0.0% | `tools/scripts/tsc/generate.ts:7` `generateEncoder` |

The last 3 are the default-import miss in section 7.3.

### 4.2 Files ranked by unresolved ratio

79 of the 271 files emit any `site` row; the other 192 are `.d.ts` declaration
files and generated tables with no calls. Files with at least 40 sites:

| path | sites | resolved | unresolved |
|---|---|---|---|
| `packages/typescript/test/async/api.test.ts` | 3993 | 202 | 94.9% |
| `packages/typescript/test/sync/api.test.ts` | 3927 | 200 | 94.9% |
| `packages/typescript/test/diagnosticFormatter.test.ts` | 77 | 13 | 83.1% |
| `packages/vscode-typescript/src/languageFeatures/onAutoInsert.ts` | 46 | 12 | 73.9% |
| `packages/typescript/test/async/astnav.test.ts` | 42 | 11 | 73.8% |
| `packages/typescript/src/api/async/client.ts` | 87 | 23 | 73.6% |
| `packages/vscode-typescript/src/extension.ts` | 93 | 25 | 73.1% |
| `tools/scripts/tsc/generate-ts-ast.ts` | 846 | 242 | 71.4% |
| `packages/typescript/src/api/node/msgpack.ts` | 45 | 13 | 71.1% |
| `packages/vscode-typescript/src/client.ts` | 161 | 48 | 70.2% |

Opened, cause per file:

| path | cause |
|---|---|
| `test/async/api.test.ts`, `test/sync/api.test.ts` | `node:test` and `node:assert` (`ok` 1471, `equal` 731, `test` 653, `strictEqual` 331, `deepEqual` 217): external package, and the two files define the same helper names as each other so every shared helper is ambiguous |
| `test/diagnosticFormatter.test.ts` | `spawnAPI` is defined in 6 corpus files; ambiguous |
| `languageFeatures/onAutoInsert.ts`, `extension.ts`, `client.ts` | the `vscode` package: external, no manifest in the corpus |
| `test/async/astnav.test.ts` | `node:test` plus `astnav` helpers defined twice (sync and async mirrors) |
| `src/api/async/client.ts` | `node:net`, `node:child_process`, plus `apiRequest` defined in 3 files |
| `tools/scripts/tsc/generate-ts-ast.ts` | `node:fs`, `node:path`, plus `push`/`pop` array methods |
| `src/api/node/msgpack.ts` | `TextEncoder`, `DataView`, typed-array methods: JS globals |

Every one of the top 10 is external package, JS builtin, or a name the corpus
defines more than once. None is a parser gap.

## 5. Step 3: entrypoint crawl

BFS over `resolved_edge`, caller to callee, keyed `(path, name)`.
Script: `ts.crawl.py`. Def universe is the call-plane `node` rows with kind
`function` or `method`: 2517 distinct keys over 2659 spans, 105 keys carrying
more than one span.

`resolved_edge.caller_name` reads `closure@<def span start>` for a lambda body,
and the call plane names no lambda, so 2993 of 8025 edges (37.3%) have a caller
that no `node` row spells. `--fold-closures` re-homes the offset onto the
innermost named def covering it. 2122 of those 2993 (70.9%) have no named def
covering them at all and stay dropped even then.

| root set | fold closures | roots | reachable | total defs | reachable share | max depth |
|---|---|---|---|---|---|---|
| A: package `exports` map | no | 1294 | 1534 | 2517 | 60.9% | 8 |
| A | yes | 1294 | 1566 | 2517 | 62.2% | 8 |
| B: tests and generators | no | 110 | 231 | 2517 | 9.2% | 8 |
| B | yes | 110 | 423 | 2517 | 16.8% | 11 |
| A + B | yes | 1404 | 1849 | 2517 | 73.5% | 11 |

Folding closures raises root set B's reach by 83% and its depth from 8 to 11.
That difference is the direct cost of the unnamed-lambda caller, measured.

### 5.1 Depth histogram, A + B folded

| depth | defs |
|---|---|
| 0 | 1404 |
| 1 | 136 |
| 2 | 89 |
| 3 | 69 |
| 4 | 37 |
| 5 | 30 |
| 6 | 24 |
| 7 | 16 |
| 8 | 20 |
| 9 | 12 |
| 10 | 9 |
| 11 | 3 |

### 5.2 Twenty highest out-degree

| out | node |
|---|---|
| 28 | `tsc/internal/lsp/lsproto/_generate/generate.mts:generateCode` |
| 25 | `packages/typescript/src/ast/scanner.ts:scan` |
| 22 | `tools/scripts/tsc/generate-encoder.ts:generateTSNodeGenerated` |
| 22 | `tools/scripts/tsc/generate-go-ast.ts:generate` |
| 19 | `tools/scripts/tsc/generate-ts-ast.ts:generateAstGenerated` |
| 17 | `tools/scripts/tsc/generate-ts-ast.ts:generateFactory` |
| 15 | `packages/typescript/test/async/api.bench.ts:runBenchmarks` |
| 15 | `packages/typescript/test/sync/api.bench.ts:runBenchmarks` |
| 15 | `packages/vscode-typescript/src/extension.ts:activate` |
| 14 | `packages/typescript/scripts/generateSync.ts:addGeneratorEdit` |
| 14 | `packages/typescript/scripts/generateSync.ts:visit` |
| 14 | `tools/scripts/tsc/generate-encoder.ts:generateTSGetNodeCommonData` |
| 14 | `tools/scripts/tsc/generate-encoder.ts:generateGoGetNodeCommonData` |
| 13 | `packages/vscode-typescript/src/client.ts:start` |
| 13 | `tools/scripts/tsc/schema.ts:resolveType` |
| 12 | `tools/scripts/tsc/generate-encoder.ts:generateGoCreateStringNode` |
| 11 | `packages/typescript/src/ast/astnav.ts:getTokenAtPositionImpl` |
| 11 | `packages/typescript/src/ast/scanner.ts:scanJsDocToken` |
| 11 | `tools/scripts/tsc/schema.ts:validate` |
| 10 | `packages/typescript/src/api/sync/api.ts:createProgram` |

### 5.3 Twenty largest unreachable defs by span

668 defs are unreachable from A + B folded. The largest twenty, with the ten
opened ones classified.

| span bytes | node | verdict |
|---|---|---|
| 63105 | `tsc/internal/lsp/lsproto/_generate/generate.mts:generateCode` | root-set gap: `generate.mts` is a fourth program with its own module-level main; 1 call site, 1 resolved-in edge, so the edge exists and only the root is missing |
| 28385 | `tools/scripts/tsc/generate-ts-ast.ts:generateFactory` | missed edge, upstream: reached only through `main`, and `main` is a default export nothing resolves to (section 7.3) |
| 20774 | `tsc/internal/lsp/lsproto/_generate/generate.mts:patchAndPreprocessModel` | root-set gap, same program as row 1 |
| 13846 | `tools/scripts/tsc/generate-encoder.ts:emitRemoteNodeClassOpen` | missed edge, upstream: same `main` break |
| 13767 | `tools/scripts/tsc/generate-ts-ast.ts:generateIsGenerated` | missed edge, upstream: same `main` break |
| 9583 | `packages/vscode-typescript/src/extension.ts:activate` | dynamically dispatched: 0 call sites in the corpus, the VS Code host invokes it |
| 9227 | `tools/scripts/tsc/generate-ts-ast.ts:generateVisitor` | missed edge, upstream |
| 8312 | `tools/scripts/tsc/generate-encoder.ts:generateTSNodeGenerated` | missed edge, upstream |
| 7650 | `tools/scripts/tsc/generate-encoder.ts:emitRemoteNodeList` | missed edge, upstream |
| 6333 | `tools/scripts/tsc/generate-go-ast.ts:generate` | missed edge, upstream |
| 5980 | `tools/scripts/tsc/generate-ts-ast.ts:generateAstGenerated` | not opened |
| 5631 | `packages/vscode-typescript/src/client.ts:Client` | ambiguity drop: 3 call sites, 0 resolved, `Client` is defined in 3 corpus files |
| 4395 | `tsc/internal/lsp/lsproto/_generate/generate.mts:handleOrType` | not opened |
| 4343 | `tsc/internal/lsp/lsproto/_generate/generate.mts:resolveType` | not opened |
| 4187 | `packages/typescript/src/ast/scanner.ts:reScanSlashToken` | dead in this universe: 0 call sites anywhere in the 271 files; the Go side is the only caller |
| 4036 | `packages/typescript/src/ast/scanner.ts:scanJsDocToken` | root-set gap: 1 site, 1 resolved-in edge, caller itself unreachable |
| 3441 | `packages/typescript/src/api/syncChannel.ts:SyncRpcChannel` | root-set gap, same shape |
| 3392 | `tools/scripts/tsc/generate-go-ast.ts:generateKind` | missed edge, upstream |
| 3066 | `packages/vscode-typescript/src/extension.ts:warnAboutTsServerPlugins` | root-set gap: reached only from `activate`, which the host dispatches |
| 2937 | `tools/scripts/tsc/generate-encoder.ts:emitCommonDataDecode` | not opened |

Eight of the ten opened are downstream of one defect: the three code generators
are entered through a default import, `generate.ts:2-9` calls
`generateEncoder()` / `generateGoAST()` / `generateTSAST()` where the exported
declaration is `export default function main()`, and no edge is minted. One
missing rule buries the entire `tools/scripts/tsc` subtree.

## 6. Step 4: scip comparison

`scip-typescript` 0.4.0, run over a copy of the corpus in scratch so the
checkout stays clean.

```
extract --family scip --scip-timeout 300 .   # cwd = scipcopy/packages/typescript
```

| measure | value |
|---|---|
| `scip_index.reused` | false |
| documents indexed | 108 |
| `scip_skip` rows | 0 |
| `scip_def` | 11697 |
| `scip_name` | 11697 |
| `scip_ref` | 11440 |
| `scip_fn_edge` | 20159 |
| `scip_edge` | 283 |
| `scip_impl` | 130 |

The index covers `packages/typescript` only (`tsconfig.json` excludes `test/`
and `scripts/`), so the diet side is re-run over the same root: 127 files,
5116 `resolved_edge` rows, 0.144 s.

### 6.1 `scip_fn_edge` is a reference edge, not a call edge

| caller kind | callee kind | rows |
|---|---|---|
| fn | term | 7008 |
| fn | type | 6542 |
| fn | fn | 3237 |
| fn | param | 2607 |
| fn | other | 765 |

Only the fn to fn slice is call-shaped. Even inside it, 747 of the 1331
scip-only edges name a callee that has no call site anywhere in the caller's
file: `createGetCanonicalFileName` returning `toLowerCase` by name
(`src/api/path.ts:398`) and `createScanner` listing `setLanguageVariant` as a
shorthand property in its returned object (`src/ast/scanner.ts:960`) are both
value references that `scip_fn_edge` files as edges.

### 6.2 Side by side

Both sides normalized: scip class qualifiers, `<get>`/`<set>` prefixes and
`typeLiteralNNN:gen` suffixes stripped; diet `closure@N` callers folded onto
the innermost named def; diet restricted to the 108 indexed documents.

| measure | value |
|---|---|
| scip fn to fn distinct pairs | 2798 |
| diet pairs over indexed documents | 2138 |
| agree | 1467 |
| scip-only | 1331 |
| diet-only | 671 |
| diet precision against scip fn plus term | 0.689 |
| diet recall against all scip fn to fn | 0.524 |
| diet recall against call-shaped scip edges only (1467 / 2051) | 0.715 |
| diet edges dropped because a closure caller had no named parent | 2089 |

### 6.3 Reachability side by side

The crawl cannot be re-run over `scip_fn_edge` on equal terms: the symbol
descriptor is the only key it carries, so a class method is
`API#transpileModule` where the call plane has `transpileModule`, and 44% of
`scip_fn_edge` rows are not calls. Re-keying to `(path, bare name)` collides
every same-named method across classes in one file. The two reachability
numbers that are defensible on identical keys are the diet crawl in section 5
and the edge-level agreement in 6.2.

### 6.4 Thirty sampled edges

Fifteen present only in scip, fifteen only in diet.

| # | side | caller | callee | classification |
|---|---|---|---|---|
| 1 | scip-only | `src/ast/factory.generated.ts:createIndexedAccessTypeNode` | `<constructor>` | naming: scip files a `new` under `<constructor>`, diet under the class name |
| 2 | scip-only | `src/api/node/node.generated.ts:forEachChild` | `src/api/node/node.infrastructure.ts:next` | ambiguity: 2 corpus files define `next` |
| 3 | scip-only | `src/api/async/api.ts:getBaseConstraintOfType` | `src/api/async/client.ts:apiRequest` | ambiguity: 3 corpus files define `apiRequest` |
| 4 | scip-only | `src/ast/visitor.generated.ts:visitEachChild` | `src/ast/factory.generated.ts:updateObjectLiteralExpression` | caller naming: diet has this edge with caller `closure@N`, and the fold could not name it |
| 5 | scip-only | `src/ast/factory.generated.ts:createJSDocTypeLiteral` | `<constructor>` | naming, as row 1 |
| 6 | scip-only | `src/ast/is.generated.ts:Handle` | `src/ast/is.generated.ts:isJSDocText` | not a call: shorthand property reference in an object literal |
| 7 | scip-only | `src/ast/is.generated.ts:Handle` | `src/ast/is.generated.ts:isConstKeyword` | not a call, as row 6 |
| 8 | scip-only | `src/ast/scanner.ts:createScanner` | `src/ast/scanner.ts:setLanguageVariant` | not a call: `scanner.ts:960` lists it as a shorthand property in the returned object |
| 9 | scip-only | `src/ast/is.generated.ts:Handle` | `src/ast/is.generated.ts:isAccessorDeclaration` | not a call, as row 6 |
| 10 | scip-only | `src/api/path.ts:createGetCanonicalFileName` | `src/api/path.ts:toLowerCase` | not a call: `path.ts:398` returns the function by name |
| 11 | scip-only | `src/api/sync/api.ts:getReferencesToSymbolInFile` | `src/api/sync/client.ts:apiRequest` | ambiguity, as row 3 |
| 12 | scip-only | `src/ast/factory.generated.ts:createCaseBlock` | `<constructor>` | naming, as row 1 |
| 13 | scip-only | `src/ast/is.generated.ts:Handle` | `src/ast/is.generated.ts:isModuleBlock` | not a call, as row 6 |
| 14 | scip-only | `src/api/async/api.ts:fetchTypeParameterAtPosition` | `src/api/async/client.ts:apiRequest` | ambiguity, as row 3 |
| 15 | scip-only | `src/api/sync/api.ts:isArrayType` | `src/api/sync/client.ts:apiRequest` | ambiguity, as row 3 |
| 16 | diet-only | `src/ast/scanner.ts:scanJsDocToken` | `src/ast/scanner.ts:isIdentifierStart` | correct diet edge scip's fn-edge fold missed; the call is at `scanner.ts:1102` and scip carries 21 occurrences of the symbol |
| 17 | diet-only | `src/ast/factory.generated.ts:createNamedImports` | `NodeObject` | naming: `new NodeObject(...)`, scip files it under `<constructor>` |
| 18 | diet-only | `src/api/async/api.ts:fetchIndexInfosOfType` | `NodeHandle` | naming, as row 17 |
| 19 | diet-only | `src/api/node/node.generated.ts:RemoteNodeList` | `src/api/node/node.generated.ts:at` | wrong edge: `Array.prototype.at` bound to a same-named corpus method |
| 20 | diet-only | `src/api/fs.ts:removeFile` | `src/api/fs.ts:getNodeFromPath` | correct diet edge, same-file call scip's fold missed |
| 21 | diet-only | `src/ast/factory.generated.ts:createTemplateLiteralTypeNode` | `NodeObject` | naming, as row 17 |
| 22 | diet-only | `src/ast/factory.generated.ts:createJSDocText` | `NodeObject` | naming, as row 17 |
| 23 | diet-only | `src/api/async/api.ts:clearSourceFileCache` | `src/api/async/api.ts:clear` | wrong edge: `Map.prototype.clear` bound to a same-named corpus method |
| 24 | diet-only | `src/ast/factory.generated.ts:createTypeOperatorNode` | `NodeObject` | naming, as row 17 |
| 25 | diet-only | `src/ast/scanner.ts:scanJsxAttributeValue` | `src/ast/scanner.ts:scan` | correct diet edge, same-file call |
| 26 | diet-only | `src/ast/factory.generated.ts:createJsxAttributes` | `NodeObject` | naming, as row 17 |
| 27 | diet-only | `src/ast/factory.generated.ts:createDebuggerStatement` | `NodeObject` | naming, as row 17 |
| 28 | diet-only | `src/api/async/api.ts:getSourceFileMetadataByPath` | `src/api/sourceFileCache.ts:set` | wrong edge: `Map.prototype.set` bound to `SourceFileCache.set` |
| 29 | diet-only | `src/api/sync/api.ts:getIndexInfosOfType` | `src/api/sync/api.ts:gen` | ambiguity survivor: `gen` has 6 defs in `api.ts` alone, all one blob, so the blob-uniqueness test passes and the wrong one is picked |
| 30 | diet-only | `src/api/async/api.ts:getDefaultProjectForFile` | `src/api/path.ts:toPath` | wrong edge: the receiver is `this.toPath`, a class field scip names `API#toPath`; diet bound it to the module function |

Tally: of the 15 scip-only, 5 are ambiguity drops the diet mode documents,
5 are not calls at all, 3 are naming, 2 are the closure caller. Of the 15
diet-only, 7 are naming, 4 are wrong edges, 3 are correct edges scip's fold
missed, 1 is a within-file ambiguity survivor.

## 7. Step 5: kinks

| class | count in corpus | example | owner fn | fixture |
|---|---|---|---|---|
| exported-declaration initializer bodies are not call defs | 413 call sites emit no edge | `packages/typescript/src/api/node/msgpack.ts:52` | `lambda_entry_decl`, `src/lang/ts.rs:1597` | `ts_findings/exported_const_initializer.ts` |
| class-field arrow bodies are not call defs | 0 in this corpus, verified by fixture | no instance: `grep` for a field-bound arrow over `packages/*/src` returns none | `lambda_entry_class`, `src/lang/ts.rs:1611` | `ts_findings/class_field_initializer.ts` |
| receiver-blind method binding mints a wrong edge | 642 of 8025 edges (8.0%) | `push` to `tools/scripts/tsc/generate-encoder.ts:44` (87 edges), `add` to `src/api/node/encoder.ts` (114) | `call_name_match`, `src/lang/ts.rs:3292` | `ts_findings/receiver_blind_method/` |
| a bodiless `.d.ts` declaration wins the name match | 172 edges, 135 into `lib.es2015.reflect.d.ts` | `Reflect.get` captures 125 plain `.get(` calls | `call_name_match`, `src/lang/ts.rs:3292` | `ts_findings/ambient_dts_target/` |
| a default import's local alias never resolves | 3 sites, and they bury 8 of the 20 largest unreachable defs | `tools/scripts/tsc/generate.ts:7` | `call_name_match`, `src/lang/ts.rs:3292` | `ts_findings/default_import_alias/` |
| a lambda caller has no `node` row to join to | 2993 of 8025 edges (37.3%), 2122 with no named parent | `src/ast/visitor.generated.ts` 169-entry table | `caller_name`, `src/project.rs:1004` | covered by `exported_const_initializer.ts` |
| a UTF-16 source yields zero facts, rc 0, no diagnostic | 7 of 12967 testdata files | `tsc/testdata/tests/cases/compiler/bom-utf16le.ts` | `run_one`, `src/bin/extract.rs:404` | `ts_findings/utf16_source.ts` |
| invalid UTF-8 exits 0 where `--help` promises 1 | 1 of 12967 | `tsc/testdata/tests/cases/compiler/regexInvalidUtf8WithUnicodeFlag.ts` | `run_one`, `src/bin/extract.rs:404` | `ts_findings/invalid_utf8.ts` |

### 7.1 Exported-declaration initializers, in full

Two identical object literals one line apart:

```ts
const        a = { k: (n: string): string => { return target(n); } };  // lambda def minted
export const b = { k: (n: string): string => { return target(n); } };  // NO node row
```

Both call sites reach the stream. Only the first gets a covering def, so only
the first yields a `resolved_edge`; the second is dropped in silence.

Verified matrix:

| declaration | def minted | edge |
|---|---|---|
| `const c1 = arrow` | `function`, name `c1` | yes |
| `export const c2 = arrow` | `function`, name `c2` | yes |
| `const o1 = { k: arrow }` | `lambda` | yes |
| `export const o2 = { k: arrow }` | none | no |
| `const arr1 = [arrow]` | `lambda` | yes |
| `export const arr2 = [arrow]` | none | no |
| `export default { k: arrow }` | none | no |
| `class C { handler = arrow }` | none | no |
| `class C { method() {} }` | `method` | yes |

A function bound directly to the const survives the `export`. A function nested
inside a composite initializer does not. `src/lang/ts.rs:1570-1574` states the
exclusion and attributes it to v5 emission-set parity, so this is a ported
decision rather than an accident; the corpus cost is what is new. It also
explains why `src/ast/visitor.generated.ts` works at all: its 169-entry
dispatch table is a non-exported `const`.

### 7.2 Receiver-blind binding

`call_name_match` (`src/lang/ts.rs:3292`) matches the callee spelling against
the corpus def index and never reads the receiver. When exactly one file
defines the name, the edge is minted. One class method named `push` therefore
captures every `Array.prototype.push` call in the resolution universe.

The documented trade says an ambiguous name yields no edge and a unique name
yields an edge. A unique-but-unrelated method makes that edge point at the
wrong definition rather than at nothing, which is a different failure than the
one the help text describes.

`tests/fixtures/ts_findings/corpus_2.ts` (PR #528) already carries this class
for a member call on a plain object. The fixture added here carries the builtin
prototype face of it, where the receiver is an `Array` or a `Map` and the
capturing definition is a class method.

Top wrong-edge targets:

| callee | target | edges |
|---|---|---|
| `get` | `tsc/internal/bundled/libs/lib.es2015.reflect.d.ts` | 125 |
| `add` | `packages/typescript/src/api/node/encoder.ts` | 114 |
| `push` | `tools/scripts/tsc/generate-encoder.ts` | 87 |
| `find` | `packages/typescript/src/ast/astnav.ts` | 80 |
| `push` | `tools/scripts/tsc/generate-go-ast.ts` | 52 |
| `pop` | `tools/scripts/tsc/generate-encoder.ts` | 47 |
| `pop` | `tools/scripts/tsc/generate-go-ast.ts` | 40 |
| `at` | `packages/typescript/src/api/node/node.generated.ts` | 35 |
| `toLowerCase` | `packages/typescript/src/api/path.ts` | 16 |
| `parseInt` | `tsc/internal/bundled/libs/lib.es5.d.ts` | 15 |

### 7.3 Default import alias

`tools/scripts/tsc/generate.ts:2` reads
`import generateEncoder from "./generate-encoder.ts"`, and
`generate-encoder.ts:2001` reads `export default function main()`. The
`specifier` record already carries the whole join: `kind` default, `module`
`./generate-encoder.ts`, `imported` default, `name` `generateEncoder`.
`call_name_match` reads none of it and matches on the callee spelling alone,
so the call cannot resolve. `main` has 5 call sites in the corpus and 0
resolved-in edges.

Three unresolved sites, and they make the entire `tools/scripts/tsc` generator
subtree unreachable from every entrypoint.

### 7.4 Encoding

A UTF-16 file exits 0 with an empty stream and empty stderr. `--file-fact`
still prints a `file` row with a digest, a byte count and a line count, which
reads as "this file exists and has no code". `extract --help` EXIT CODES
promises 1 for a UTF-8 read failure and the `--max-bytes` documentation states
the principle directly: "A silent timeout is a defect and a named skip is a
fact". Encoding has no such named skip. A UTF-8 BOM works correctly.

## 8. What stays untested and why

| area | why |
|---|---|
| the Go half of the corpus, 5102 `.go` files | owned by lane `crawl/extract-typescript-go` |
| `--family df`, `--family cfg`, `--family data` | the brief scopes this lane to the call plane and the crawl |
| `--family diet_scip` as its own step | `--resolve --family call` is the same name-match arm over the same files; running it twice measures one thing twice |
| `--deps` and `--scip-deps` module graph | the brief's five steps do not reach the module plane, and `--scip-deps` already carries a graded recall/precision number in `--help` |
| a crawl over `scip_fn_edge` | section 6.3: no shared key survives normalization, and 44% of the rows are not calls |
| `scip` over `packages/vscode-typescript` and `tools` | `scip-typescript` needs one tsconfig per root and the checkout has no `node_modules`; the one root that indexes cleanly is `packages/typescript` |
| any fix to `v6/sprefa-extract/src/**` | forbidden to this lane; two fix lanes own that tree |
| a serial rerun of all 13238 files | the parallel `ms_parallel8` column is inflated 3x to 8x; the files that matter were remeasured serially in section 3.3 and the rest carry no claim |
| `tests/baselines/**` and `tests/cases/**` as separate sets | the corpus has neither path; the equivalent tree is `tsc/testdata/**`, measured in step 1 and excluded from steps 2 to 4 |

---

## 9. Jelly as a second call oracle (lane feat-extract-jelly-comparator, 2026-08-31)

Tool: `@cs-au-dk/jelly` 0.13.0 via `npm exec`, run over the ts5 corpus
(`/Users/chrishafley/projects/TypeScript-5.9`, `src/**` minus `src/lib`).
Full details, flags, failures, and per-chunk numbers:
`plans/extract-bench-2026-08-29/jelly.ORACLE.md`; conversion:
`plans/extract-bench-2026-08-29/jelly_convert.py`; oracle rows:
`plans/extract-bench-2026-08-29/ts5.jelly.call.tsv` (49,290 rows).

| fact | value |
|---|---|
| single run, 600 entries | node heap OOM at 4 GB cap, exit 134, no output |
| chunked workaround | 4 chunks (compiler 77 / misc 76 / services 168 / testRunner 279 files), all ok, worst 80 s |
| files analyzed | 595 of 600; 5 dropped by Babel TS transform failures (checker.ts, debug.ts, factory/nodeFactory.ts, utilities.ts, harness/harnessIO.ts) |
| rows | 49,290 unique 4-col rows after union + ts5 file rule + source-text name recovery |
| ours (`ts5.parse.call.tsv`) vs jelly | recall 38.58% / precision 32.06% |
| jelly vs tsc oracle | recall 35.69% / precision 42.98% |
| jelly vs codeql2 | recall 36.79% / precision 39.66% |
| context: tsc vs codeql2 | recall 98.91% / precision 88.56% |

Verdict: no discriminating signal beyond tsc + codeql2. 28,096 jelly-only
rows reduce to mostly `sys.ts callback` natives-model fan-in, module-init
`<module>` edges, and edges into the 4 Babel-dropped files; jelly misses
33,021 rows tsc and codeql2 agree on. Negative result, report-only.
