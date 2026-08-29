# sprefa-extract over microsoft/TypeScript release-5.9

Lane `crawl-extract-typescript-5`. Analysis only: no file under
`v6/sprefa-extract/src/**` was touched.

Binary `v6/sprefa-extract/target/release/extract`, built `--features cli` at
base `483c055a3`, `Finished release profile in 1m 57s`.
`cargo test --release --features cli` with the new fixtures in the tree:
80 suites, 389 passed, 0 failed.
Corpus `/Users/chrishafley/projects/TypeScript-5.9`, branch `release-5.9`,
head `7e133bea1`, read-only. This is the last TypeScript-in-TypeScript
compiler, so `src/` is a real 20.6 MB program rather than the npm shim the
first ts crawl (PR #538) landed on.

## Contents

1. [Corpus and exclusions](#1-corpus-and-exclusions)
2. [Step 1: per-file battery](#2-step-1-per-file-battery)
3. [Step 2: whole-project --resolve](#3-step-2-whole-project---resolve)
4. [Step 3: entrypoint crawl](#4-step-3-entrypoint-crawl)
5. [Step 4: scip comparison](#5-step-4-scip-comparison)
6. [Step 5: kinks](#6-step-5-kinks)
7. [Recount of the eight kinks from PR #538](#7-recount-of-the-eight-kinks-from-pr-538)
8. [What stays untested and why](#8-what-stays-untested-and-why)
9. [Fixes](#9-fixes-lane-fix-extract-ts-crawl-pr-against-originmain-b9b98e3af)

## 1. Corpus and exclusions

| set | files | bytes | used in |
|---|---|---|---|
| `src/**` (`.ts`, includes 101 `src/lib/*.d.ts`) | 701 | 20,577,682 | steps 1-5 |
| `tests/cases/**` (`.ts`, `.tsx`) | 19,117 | 22,306,847 | step 1 only, as its own stress row |
| `tests/baselines/**` | excluded | | not the program |
| `node_modules` | absent from the checkout | | |
| `scripts/**` | 0 `.ts` files (all `.mjs`) | | |

`src/` by directory: testRunner 279, services 168, lib 101, compiler 77,
harness 38, server 15, jsTyping 6, typingsInstallerCore 4, typescript 3,
tsserver 3, deprecatedCompat 3, tsc 2, watchGuard 1, typingsInstaller 1.

Every `extract` call in steps 1-3 ran under `timeout 10`. Step 4's index build
is the named SCIP exception.

## 2. Step 1: per-file battery

One process per file, `xargs`-equivalent parallelism 8
(`plans/extract-crawl-2026-08-29/ts5.battery.py`). Raw table:
`ts5.runs.tsv` (19,818 rows).

| corpus | files | rc!=0 | rc=124 | size_skip | zero-fact | facts emitted |
|---|---|---|---|---|---|---|
| `src/**` | 701 | 0 | 0 | 0 | 0 | 5,021,067 |
| `tests/cases/**` | 19,117 | 0 | 0 | 0 | 9 | 5,823,866 |

No crash, no timeout, no `size_skip` anywhere. The 16 MB ceiling is never
approached: the largest input in the whole corpus is 4,024,809 B.

### 2.1 The nine zero-fact test files

| path | bytes | `file -b` |
|---|---|---|
| `tests/cases/compiler/bom-utf16be.ts` | 24 | UTF-16 big-endian |
| `tests/cases/compiler/bom-utf16le.ts` | 24 | UTF-16 little-endian |
| `tests/cases/compiler/collisionCodeGenModuleWithUnicodeNames.ts` | 718 | UTF-16 little-endian |
| `tests/cases/compiler/instanceofOperator.ts` | 1,120 | UTF-16 little-endian |
| `tests/cases/compiler/promiseTest.ts` | 528 | UTF-16 little-endian |
| `tests/cases/compiler/targetTypeBaseCalls.ts` | 786 | UTF-16 little-endian |
| `tests/cases/compiler/unicodeIdentifierNames.ts` | 674 | UTF-16 little-endian |
| `tests/cases/compiler/corrupted.ts` | 8 | invalid UTF-8 (`c6 1f bc 03 c1 03 19 1f`) |
| `tests/cases/compiler/TransportStream.ts` | 564 | binary, valid UTF-8 control bytes |

Seven UTF-16 and one invalid-UTF-8 file exit 0 with an empty stream. Recount of
PR #538 kinks 7 and 8; see section 7.

### 2.2 Largest 20 in `src/**`, remeasured serially with `/usr/bin/time -l`

Full table: `ts5.serial_top20.tsv`. Head:

| path | ms | bytes | facts | max RSS |
|---|---|---|---|---|
| `src/compiler/checker.ts` | 388 | 3,121,747 | 630,064 | 104,054,784 |
| `src/lib/dom.generated.d.ts` | 155 | 1,874,048 | 235,659 | 43,302,912 |
| `src/lib/webworker.generated.d.ts` | 71 | 608,704 | 86,420 | 21,659,648 |
| `src/compiler/parser.ts` | 97 | 539,588 | 151,166 | 26,918,912 |
| `src/compiler/utilities.ts` | 101 | 510,858 | 153,376 | 28,049,408 |

Serial throughput across those 20 spans 3,235 to 12,091 B/ms; the slowest is
`src/services/utilities.ts` and the fastest is the generated DOM lib. The
parallel-8 column in `ts5.runs.tsv` is inflated 3x to 8x by contention and
carries no perf claim on its own.

### 2.3 `tests/cases/**` outliers, remeasured serially

Full table: `ts5.serial_testcases_slow.tsv`.

| path | ms | bytes | max RSS | RSS per input byte |
|---|---|---|---|---|
| `tests/cases/compiler/binderBinaryExpressionStress.ts` | 170 | 39,935 | 98,680,832 | 2,471x |
| `tests/cases/compiler/binderBinaryExpressionStressJs.ts` | 174 | 39,969 | 98,009,088 | 2,452x |
| `tests/cases/fourslash/reallyLargeFile.ts` | 435 | 3,502,391 | 242,040,832 | 69x |
| `tests/cases/compiler/largeControlFlowGraph.ts` | 84 | 140,192 | 24,363,008 | 174x |
| `tests/cases/fourslash/codeFixClassImplementInterfaceNoTruncation.ts` | 61 | 4,024,809 | 34,603,008 | 9x |

Under parallel 8 the two `binderBinaryExpressionStress` files cost 8.3 s and
8.0 s wall, inside the 10 s cap but the closest anything came. Serially they
cost 170 ms. Peak RSS is driven by nesting depth, not by size: a 40 KB file of
deeply nested binary expressions peaks higher than the 3.12 MB checker
(98.7 MB against 104.1 MB), while a 4.0 MB file of flat member declarations
peaks at 34.6 MB. `--max-bytes` bounds the wrong axis for this shape; see
section 6.

## 3. Step 2: whole-project `--resolve`

One call over all 701 files, no split needed:

```
extract --resolve --family call,type <701 paths>    rc=0, 2 s, 95,972 rows
```

| record | rows |
|---|---|
| `resolved_edge` | 75,089 (all `kind=name_resolve`) |
| `resolved_type_edge` | 20,883 |

`resolved_type_edge` by kind: param 5,286, field 4,964, uses 4,330,
returns 2,539, generic 2,242, variant 1,479, impl 43.

| measure | count |
|---|---|
| distinct callers (path, name) | 18,122 |
| distinct callees (path, name) | 12,380 |
| caller files | 507 of 701 |
| callee files | 407 of 701 |
| cross-file edges | 38,315 of 75,089 (51.0%) |

Per-file phase-1 facts came from 701 separate `extract --family call FILE`
runs (`ts5.callfam.py`): 99,542 `site`, 23,944 `node`
(function 12,332, lambda 9,643, method 1,969), 19,089 `specifier`
(named 18,113, reexport 534, namespace 414, default 18, require 9,
dynamic_import 1), 235 `unresolved`
(spread-call-args 228, computed-member-call 5, dynamic-import 2).

### 3.1 Unresolved sites by cause

Sites joined to edges on `(path, caller_site_start)`. Ambiguity is judged the
way `call_name_match` judges it: the callee name has defs in two or more
distinct files. Table: `ts5.unresolved_class.tsv`.

| class | sites | share of the 23,894 unresolved |
|---|---|---|
| covered by a def, callee ambiguous | 11,658 | 48.8% |
| covered by a def, callee has no def in the universe | 10,196 | 42.7% |
| no covering def, callee unique | 740 | 3.1% |
| covered by a def, callee unique, still no edge | 682 | 2.9% |
| no covering def, callee has no def in the universe | 508 | 2.1% |
| no covering def, callee ambiguous | 110 | 0.5% |

Overall: 99,542 sites, 75,089 edges, 23,894 unresolved sites (24.0%).
1,358 sites lie outside every `node` span in their file and are dropped before
any name match runs.

Re-judging the 11,768 ambiguous sites with the named import plus the
`export * from` reexport closure (`ts5.resolve_analysis.py`, closure taken from
`--deps` output):

| outcome | sites | share |
|---|---|---|
| the name is not imported in this file (member call on a receiver) | 8,126 | 69.1% |
| still ambiguous inside the barrel closure | 2,399 | 20.4% |
| **narrows to exactly one def** | **1,241** | **10.5%** |
| the module resolves outside the universe | 2 | 0.0% |

`--deps` over the same 701 files already computes that closure: rc=0, 2,022
`file_edge` rows, 42 `file_unresolved` (41 `node_modules_boundary`, 1
`relative_unresolved` for `../diagnosticInformationMap.generated.js`, a file
`Herebyfile.mjs:82` generates at build time and the checkout does not carry).
`--resolve` reads neither the `specifier` rows nor that graph.

### 3.2 Files ranked by unresolved ratio (sites >= 100)

Each of the top 10 was opened and every one of its unresolved sites bucketed by
whether the site text carries a receiver and whether the callee name has any def
in the universe.

| ratio | unres | sites | file | bare, absent | member, absent | member, ambiguous | bare, ambiguous | top unresolved callees |
|---|---|---|---|---|---|---|---|---|
| 0.966 | 632 | 654 | `.../evaluation/esDecorators.ts` | 302 | 264 | 66 | 0 | `it`, `describe`, `main`, `throws`, `strictEqual` |
| 0.873 | 186 | 213 | `.../services/convertToAsyncFunction.ts` | 158 | 9 | 12 | 5 | `_testConvertToAsyncFunction`, `createTestWrapper`, `fn`, `find` |
| 0.858 | 121 | 141 | `.../tsserver/versionCache.ts` | 66 | 31 | 24 | 0 | `it`, `validateEditAtPosition`, `equal`, `substring` |
| 0.783 | 112 | 143 | `.../unittests/compilerCore.ts` | 14 | 63 | 35 | 0 | `isFalse`, `equal`, `isTrue`, `add`, `has` |
| 0.767 | 135 | 176 | `.../evaluation/usingDeclarations.ts` | 79 | 56 | 0 | 0 | `it`, `deepEqual`, `main`, `strictEqual`, `slice` |
| 0.761 | 143 | 188 | `.../evaluation/awaitUsingDeclarations.ts` | 88 | 55 | 0 | 0 | `it`, `main`, `deepEqual`, `strictEqual` |
| 0.757 | 84 | 111 | `.../services/languageService.ts` | 11 | 17 | 44 | 0 | `getProgram`, `deepEqual`, `it`, `set`, `writeFile` |
| 0.747 | 124 | 166 | `.../tsserver/session.ts` | 61 | 34 | 24 | 5 | `expect`, `it`, `equal`, `describe`, `onMessage` |
| 0.682 | 507 | 743 | `.../unittests/paths.ts` | 15 | 364 | 128 | 0 | `strictEqual`, `getNormalizedAbsolutePath`, `deepEqual`, `resolvePath` |
| 0.646 | 117 | 181 | `.../parallel/host.ts` | 18 | 44 | 49 | 4 | `log`, `emit`, `exit`, `_color`, `on` |

Causes, in order of size:

| cause | evidence |
|---|---|
| mocha and chai globals (`it`, `describe`, `expect`) and `assert.*` members have no def in the universe | the largest bucket in 9 of the 10; the documented and correct stop |
| a name destructured from a runtime evaluation has no static def | `esDecorators.ts:1124` is `const { main } = exec\`...\``, then 4 `main(...)` calls; `main` cannot have a def |
| a callable bound to a factory call rather than to a function literal mints no def | `convertToAsyncFunction.ts:428` `const _testConvertToAsyncFunction = createTestWrapper(...)`, 158 of that file's 186 unresolved sites |
| receiver methods on a compiler type resolved by name alone | `paths.ts` 128 and `compilerCore.ts` 35 member-call sites whose bare name is ambiguous |
| node builtins reached through a namespace import | `parallel/host.ts` `log`, `emit`, `exit`, `on` from `mocha`, `readline`, `tty` |

All ten worst files are `src/testRunner/**`, and no row is an interface- or
trait-dispatch miss: TypeScript's compiler passes concrete function values, so
the misses are library boundaries, runtime-evaluated names, and factory-bound
callables.

## 4. Step 3: entrypoint crawl

`plans/extract-crawl-2026-08-29/ts5.crawl.py`, BFS over `resolved_edge`.
A graph node is `(path, name)` because `resolved_edge` carries no callee span;
same-name defs in one file fold together. Denominator: 14,047 `node` rows of
kind `function` or `method` with a non-null name.

Seed sets:

| set | definition | seeds |
|---|---|---|
| A_strict | every def in `src/tsc/tsc.ts`, `src/tsserver/server.ts`, `src/typescript/typescript.ts`, plus every `export function` in `src/compiler/program.ts` | 44 |
| A_patched | A_strict plus the uniquely-named callees of the entrypoint files' top-level sites, which the resolver drops | 49 |
| B_testRunner | every def under `src/testRunner/**` | 1,014 |

Two variants of the edge relation: `strict` uses the caller key the stream
carries, `closure_folded` reattaches every `closure@<offset>` caller to its
nearest enclosing named function or method.

| set | edges | seeds | reachable | of 14,047 | max depth |
|---|---|---|---|---|---|
| A_strict | strict | 44 | 3,509 | 25.0% | 20 |
| A_patched | strict | 49 | 3,851 | 27.4% | 20 |
| A_patched | closure-folded | 49 | 5,276 | 37.6% | 16 |
| B_testRunner | strict | 1,014 | 5,758 | 41.0% | 19 |
| B_testRunner | closure-folded | 1,014 | 7,230 | 51.5% | 14 |
| **union A_strict + B_testRunner** | strict | | **5,854** | **41.7%** | |
| **union A_patched + B_testRunner** | closure-folded | | **7,344** | **52.3%** | |

Folding the closure callers recovers 1,490 defs, a 25.5% relative gain, and
shortens the deepest chain by 4 to 5 hops.

`src/tsc/tsc.ts`, the compiler's actual `main`, yields **zero** `resolved_edge`
rows. Its eight call sites are all top-level statements, its only `node` row is
an anonymous lambda, and `executeCommandLine` has exactly one def in the
universe. The seed set A_strict reaches anything only because
`src/tsserver/server.ts` and `src/compiler/program.ts` carry real functions.

### 4.1 Depth histogram

```
depth   0    1    2    3    4    5    6    7    8    9   10   11   12   13   14   15   16   17   18   19   20
A_str  44   90  105  199  266  199  146  101  114  133  182  356  445  405  295  192  123   63   38   10    3
A_fold 49  183  203  406  674  813  792  685  545  376  245  131   91   50   20    9    4    -    -    -    -
B_str 1014 252  313  450  538  481  320  219  235  346  428  336  328  197  151   76   50   17    5    2    -
B_fold 1014 319  458  718  971 1015  871  682  518  332  190   71   44   23    4    -    -    -    -    -    -
```

The strict curve is bimodal, with a second hump at depth 11 to 14. That hump is
the crawl routing around closure callers: a chain that should pass through one
lambda instead detours through whatever named function happens to call the same
target. Folding collapses it into a single mode at depth 5.

### 4.2 Twenty largest unreachable defs (strict union)

| bytes | reached when folded | path:offset | name |
|---|---|---|---|
| 309,595 | no | `src/compiler/factory/nodeFactory.ts:11660` | `createNodeFactory` |
| 215,275 | no | `src/compiler/transformers/es2015.ts:16088` | `transformES2015` |
| 174,571 | no | `src/compiler/binder.ts:17005` | `createBinder` |
| 144,311 | no | `src/compiler/transformers/classFields.ts:10072` | `transformClassFields` |
| 125,864 | **yes** | `src/compiler/checker.ts:521411` | `symbolTableToDeclarationStatements` |
| 117,676 | no | `src/compiler/transformers/esDecorators.ts:9317` | `transformESDecorators` |
| 110,835 | no | `src/compiler/transformers/generators.ts:12959` | `transformGenerators` |
| 110,368 | no | `src/compiler/transformers/ts.ts:5833` | `transformTypeScript` |
| 107,506 | no | `src/services/completions.ts:136940` | `getCompletionData` |
| 107,079 | no | `src/compiler/transformers/module/module.ts:4133` | `transformModule` |
| 94,112 | no | `src/services/services.ts:58905` | `createLanguageService` |
| 88,105 | no | `src/compiler/transformers/declarations.ts:6745` | `transformDeclarations` |
| 82,055 | no | `src/compiler/transformers/module/system.ts:3128` | `transformSystemModule` |
| 66,124 | no | `src/compiler/transformers/es2018.ts:3625` | `transformES2018` |
| 64,445 | **yes** | `src/compiler/checker.ts:352272` | `typeToTypeNodeWorker` |
| 57,112 | **yes** | `src/compiler/parser.ts:438398` | `parseJSDocCommentWorker` |
| 51,118 | no | `src/compiler/scanner.ts:143666` | `scanRegularExpressionWorker` |
| 45,662 | **yes** | `src/services/formatting/formatting.ts:18077` | `formatSpanWorker` |
| 42,807 | no | `src/compiler/transformers/es2017.ts:2716` | `transformES2017` |
| 42,332 | no | `src/services/codefixes/fixMissingTypeAnnotationOnExports.ts:8748` | `withContext` |

Ten opened, with the cause each time:

| # | def | call sites | cause |
|---|---|---|---|
| 1 | `createNodeFactory` | 4, all outside every def span | dropped: no covering def. `src/compiler/factory/nodeFactory.ts:7409` is `export const factory: NodeFactory = createNodeFactory(...)`, `src/compiler/parser.ts:441` is `export const parseNodeFactory = createNodeFactory(...)` |
| 2 | `transformES2015` | 0 | not a call anywhere. `src/compiler/transformer.ts:182` is `transformers.push(transformES2015)` |
| 3 | `createBinder` | 2; `src/compiler/binder.ts:510` is `const binder = createBinder()` at top level | dropped: no covering def, and a second def in `src/deprecatedCompat/deprecations.ts:106` makes the name ambiguous besides |
| 4 | `transformClassFields` | 0 | `src/compiler/transformer.ts:155`, function reference |
| 5 | `symbolTableToDeclarationStatements` | 4; three covered by an anonymous lambda | reachable only once closures are folded. `src/compiler/checker.ts:6417` is an object literal of arrow properties |
| 6 | `transformESDecorators` | 0 | `src/compiler/transformer.ts:152`, function reference |
| 7 | `transformGenerators` | 0 | `src/compiler/transformer.ts:183`, function reference |
| 8 | `transformTypeScript` | 0 | `src/compiler/transformer.ts:137`, function reference |
| 9 | `getCompletionData` | 2, both covered by named functions, both edges minted | transitively dead: its callers sit behind `createLanguageService`, row 11 |
| 10 | `createLanguageService` | 9 | ambiguity drop: `src/testRunner/unittests/services/languageService.ts:29` declares a local function of the same name, so `call_name_match` sees two blobs and returns None. This one name makes the whole `src/services/**` subtree unreachable |

None of the twenty is dead code. Five are the transform pipeline, reached
through a function-reference table. Four are reachable once closure callers are
folded. The rest are top-level initializers or a single ambiguous name.

### 4.3 Twenty highest out-degree

| out | path | name |
|---|---|---|
| 174 | `src/compiler/emitter.ts` | `pipelineEmitWithHintWorker` |
| 78 | `src/compiler/checker.ts` | `checkSourceElementWorker` |
| 77 | `src/compiler/checker.ts` | `structuredTypeRelatedToWorker` |
| 75 | `src/compiler/expressionToTypeNode.ts` | `visitExistingNodeTreeSymbolsWorker` |
| 70 | `src/services/symbolDisplay.ts` | `getSymbolDisplayPartsDocumentationAndSymbolKindWorker` |
| 68 | `src/compiler/transformers/declarations.ts` | `transformTopLevelDeclaration` |
| 68 | `src/compiler/transformers/esDecorators.ts` | `transformClassLike` |
| 62 | `src/compiler/transformers/declarations.ts` | `visitDeclarationSubtree` |
| 61 | `src/compiler/checker.ts` | `typeToTypeNodeWorker` |
| 60 | `src/compiler/binder.ts` | `bindWorker` |
| 55 | `src/compiler/checker.ts` | `getTypeOfVariableOrParameterOrPropertyWorker` |
| 54 | `src/services/refactors/extractSymbol.ts` | `extractFunctionInScope` |
| 53 | `src/compiler/checker.ts` | `checkVariableLikeDeclaration` |
| 52 | `src/compiler/factory/nodeFactory.ts` | `replaceModifiers` |
| 51 | `src/compiler/checker.ts` | `checkPropertyAccessExpressionOrQualifiedName` |
| 49 | `src/compiler/checker.ts` | `getSymbolAtLocation` |
| 48 | `src/compiler/checker.ts` | `checkExpressionWorker` |
| 48 | `src/compiler/checker.ts` | `getPropertyTypeForIndexType` |
| 48 | `src/compiler/checker.ts` | `getTypeForVariableLikeDeclaration` |
| 47 | `src/compiler/checker.ts` | `checkObjectLiteral` |

Every one is a `switch`-over-`SyntaxKind` dispatcher; eleven of the twenty are
in `checker.ts`.

## 5. Step 4: scip comparison

`scip-typescript 0.4.0` at `~/.nvm/versions/node/v24.15.0/bin/scip-typescript`,
run through `extract --family scip --scip-timeout 1500 .` in a scratch copy of
the repo root (`tests/`, `.git` and `node_modules` excluded, then
`npm ci --ignore-scripts`, 365 packages). The corpus itself was never written
to. Index built fresh, `reused: false`, 801 documents, no `scip_skip` row.

| record | rows |
|---|---|
| `scip_fn_edge` | 186,935 |
| `scip_ref` | 109,526 |
| `scip_def` | 82,639 |
| `scip_name` | 82,639 |
| `scip_edge` | 11,318 |
| `scip_impl` | 1,880 |
| `scip_skip` | 0 |

### 5.1 The index silently omits the two largest files

801 documents: 699 under `src/`, 101 under the repo's prebuilt `lib/`, 1 script.
`src/` holds 701 files. The two absent from the index:

| path | bytes | in index |
|---|---|---|
| `src/compiler/checker.ts` | 3,121,747 | no |
| `src/lib/dom.generated.d.ts` | 1,874,048 | no |
| `src/lib/webworker.generated.d.ts` | 608,704 | yes |

Verified at the index level with
`extract --scip-facts --scip-record scip_document`. Everything at or below
608 KB is present; the two files above 1 MB are not. `checker.ts` is 22.2% of
the diet side's caller edges (16,680 of 75,089), so exact mode drops the single
most important file in the corpus and emits no named skip for it. `scip_skip`
exists per root, not per document, and the root succeeded.

`--scip-facts` also refuses a project root that is not a git worktree:

```
Error: Read(".", Custom { kind: Other, error: ". is not inside a Git worktree" })
```

`--family scip` over the same root works. `--help` states no such precondition
for either flag.

### 5.2 Side by side over the same nodes

Both graphs keyed on `(repo-relative file, name)`. Shared def set: names that
are a `function`/`method` node on the diet side and a function-descriptor
symbol on the scip side, in a file both sides indexed: **7,326 defs**.

| side | seeds A | seeds B | A reachable | B reachable | union | share | max depth A |
|---|---|---|---|---|---|---|---|
| diet (`resolved_edge`) | 44 | 1,014 | 1,326 | 2,699 | 2,752 | 37.6% | 20 |
| scip (`scip_fn_edge`) | 44 | 407 | 4,141 | 5,259 | 5,276 | 72.0% | 22 |

Two caveats on the scip column, both measured below: `scip_fn_edge` is a
reference edge rather than a call edge, and scip attributes a call made inside
a nested function to the outermost enclosing top-level function, which makes
its graph denser and its node set coarser (407 testRunner seeds against the
diet side's 1,014).

Edges restricted to shared defs on both ends:

| set | edges |
|---|---|
| diet | 16,576 |
| scip | 24,508 |
| in both | 14,612 |
| diet only | 1,964 |
| scip only | 9,896 |

### 5.3 Thirty sampled edges (`ts5.scip_samples.tsv`)

Fifteen from each side, seed 11.

**diet-only, 15 of 15 wrong:**

| # | edge | why it is wrong |
|---|---|---|
| 1 | `harnessIO.ts:runMultifileBaseline` -> `collectionsImpl.ts:keys` | site text is `Object.keys` |
| 2 | `symbolDisplay.ts:...Worker` -> `tracing.ts:push` | site text is `displayParts.push`, 92 of 92 sites carry a receiver |
| 3 | `scriptVersionCache.ts:getTextChangesBetweenVersions` -> `tracing.ts:push` | `this.startPath.push` |
| 4 | `stringCompletions.ts:...RelativeModules` -> `project.ts:getCompilerOptions` | `program.getCompilerOptions`, a `Program` method, not `Project`'s |
| 5 | `helpers.ts:jsonToReadableText` -> `fourslashImpl.ts:stringify` | `JSON.stringify` |
| 6 | `organizeImports.ts:organizeImports` -> `core.ts:filter` | `sourceFile.statements.filter` |
| 7 | `sourceMapRecorder.ts:recordSourceMapSpan` -> `tracing.ts:push` | `decodeErrors.push` |
| 8 | `harnessIO.ts:runMultifileBaseline` -> `tracing.ts:push` | `paths.push` |
| 9 | `editorServices.ts:openExternalProject` -> `tracing.ts:push` | `this.externalProjects.push` |
| 10 | `stringCompletions.ts:getBaseDirectoriesFromRootDirs` -> `core.ts:map` | `completion.types.map` |
| 11 | `session.ts:getNameOrDottedNameSpan` -> itself | `languageService.getNameOrDottedNameSpan` |
| 12 | `sys.ts:disableCPUProfiler` -> `vfsUtil.ts:isDirectory` | `stat.isDirectory`, a node `fs.Stats` method |
| 13 | `semver.ts:parseHyphen` -> `tracing.ts:push` | `alternatives.push` |
| 14 | `extractSymbol.ts:getEnclosingTextRange` -> `services.ts:getStart` | `startNode.getStart` |
| 15 | `editorServices.ts:sendProjectTelemetry` -> `editorServices.ts:convertTypeAcquisition` | two defs of the name in one file (`:484` exported, `:2846` nested inside the caller); the `(path, name)` key folds them, which is a limit of this comparison rather than of the tool |

Fourteen of the fifteen are the receiver-blind name match; the fifteenth is a
same-file name collision my key cannot separate.

**scip-only, 15:**

| class | count | rows |
|---|---|---|
| caller attribution differs: the diet side has the identical edge under a finer-grained caller | 12 | 16-25, 28, 30 |
| ambiguity drop on the diet side | 2 | 26 (`forEach`, 4 defs), 29 (`toPath`, 8 defs) |
| not a call inside the caller span | 1 | 27 |

Of the twelve, six name the diet caller `closure@<offset>` (rows 16, 18, 20,
23, 25, 28) and six name a nested function
(`hoistClassDeclaration`, `tryGetObjectLikeCompletionSymbols`,
`addPrivateIdentifierClassElementToEnvironment`, `visitNode`,
`getSourceMappingURL`, `findRenameLocations`). Checked directly: scip carries
none of those inner edges (`getSourceMappingURL -> getBaseFileName` is absent
from `scip_fn_edge`), and scip has zero callers named `closure*`. So the two
sides disagree on caller granularity rather than on the call, and the diet side
is the finer of the two.

Tally of the thirty: 14 wrong diet edges, 1 key artifact, 12 caller-granularity
disagreements, 2 genuine diet ambiguity drops, 1 non-call.

## 6. Step 5: kinks

| class | count in `src/**` | example | owner fn | fixture |
|---|---|---|---|---|
| a top-level statement's call site has no covering def and is dropped | 1,358 sites, 740 with a uniquely-named callee; `src/tsc/tsc.ts` is 8 of 8 and 0 edges | `src/compiler/factory/nodeFactory.ts:7409` `export const factory = createNodeFactory(...)` | `resolve_calls`, `src/lang/ts.rs:3383` (`covering_def` -> `continue`) | `ts5_findings/top_level_call.ts` + `top_level_callee.ts` |
| a `closure@<offset>` caller has no `node` row to join to | 17,592 of 75,089 edges (23.4%), 6,476 distinct closures; folding them raises reachability 5,854 -> 7,344 of 14,047 | `src/compiler/checker.ts` `closure@2973761` | `caller_name`, `src/project.rs:1004-1013` | `ts5_findings/closure_caller_key.ts` + `closure_callee.ts` |
| `export * from` barrels are not followed, so imported names stay ambiguous | 1,241 of 11,768 ambiguous sites narrow to one def with the closure `--deps` already emits | `src/compiler/binder.ts:11770` `forEachChild` through `./_namespaces/ts.js` | `call_name_match`, `src/lang/ts.rs:3307-3318` (the `[blob] = blobs.as_slice() else` bail) | `ts5_findings/barrel_reexport/` |
| the `node` record carries no exported flag, so a module-private def keeps an exported one ambiguous | 2,399 sites; `isIdentifier` has 465 sites and 24 edges, all same-file | `src/compiler/parser.ts:2318` private `isIdentifier` against `src/compiler/factory/nodeTests.ts:318` exported | `call_name_match`, `src/lang/ts.rs:3292`; the `node` shape at `--schema` | `ts5_findings/private_shadows_export/` |
| a function named as a value is not a call site | 5 of the 10 largest unreachable defs, each with 0 call sites in the corpus | `src/compiler/transformer.ts:182` `transformers.push(transformES2015)` | `visit_call_expression`, `src/lang/ts.rs:1726` (only Call/New/JSX mint a site) | `ts5_findings/function_ref_as_value.ts` |
| receiver-blind name match mints a wrong edge | 3,175 of 75,089 edges (4.2%); `tracing.ts:push` alone captures 2,064 array pushes and `binder.ts:bind` captures 54 `fn.bind(...)` | `src/compiler/binder.ts:63889` `(label.antecedent \|\| (...)).push` | `call_name_match`, `src/lang/ts.rs:3292` | `ts5_findings/receiver_blind_prototype.ts` + `tracing_like.ts` |
| exact mode silently drops a document over roughly 1 MB | 2 of 701, including `src/compiler/checker.ts` (22.2% of the corpus's caller edges) | index has 801 documents, `checker.ts` absent | outside this crate: `scip-typescript 0.4.0`. The gap it exposes is that `scip_skip` is per root, never per document (`FlatFact::ScipSkipRow`, `src/types.rs:2610`) | none, see below |
| `--scip-facts` requires a git worktree, `--help` does not say so | 1 (the scratch copy) | `Error: Read(".", ... ". is not inside a Git worktree")` | `--scip-facts` arm, `src/bin/extract.rs`; the doc string is `SCIP_FACTS_LONG`, `src/bin/extract/help.rs:182` | none, see below |
| peak RSS scales with nesting depth, and `--max-bytes` bounds size | 2 of 19,117 test files at 2,471x and 2,452x RSS per input byte | `tests/cases/compiler/binderBinaryExpressionStress.ts`, 39,935 B, 98,680,832 B RSS | `MAX_BYTES_LONG`, `src/bin/extract/help.rs:281` states the ceiling is a byte count | none, see below |

Three rows carry no fixture, with the reason:

| row | why no fixture |
|---|---|
| exact mode drops a document over ~1 MB | the repro is a >1 MB TypeScript file; committing one to `tests/fixtures` costs more than the finding is worth, and the defect is in `scip-typescript`, not in this crate. The reproducible claim is the `scip_document` list, which `ts5.scip_compare.py` regenerates |
| `--scip-facts` needs a git worktree | environmental, not a source shape; the command and its exact error are above |
| RSS scales with nesting depth | a fixture would need thousands of nested binary expressions to move the number, which is a stress input rather than a minimal repro; `tests/cases/compiler/binderBinaryExpressionStress.ts` in the corpus already is it |

Every fixture was verified against this binary. Two examples:

```
$ extract --resolve --family call ts5_findings/top_level_call.ts ts5_findings/top_level_callee.ts
{"record":"resolved_edge", ... "caller_name":"insideFn", "callee_name":"entry", ...}
```

Three `site` rows, one edge.

```
$ extract --resolve --family call ts5_findings/barrel_reexport/*.ts
(empty)
$ extract --deps --project-root ts5_findings/barrel_reexport ts5_findings/barrel_reexport/*.ts
{"record":"file_edge","src_path":"barrel.ts","dst_path":"helpers.ts","kind":"reexport","symbols":1}
{"record":"file_edge","src_path":"consumer.ts","dst_path":"barrel.ts","kind":"named","symbols":1}
```

## 7. Recount of the eight kinks from PR #538

The first ts crawl ran on `~/projects/TypeScript` main, which is now the Go port
and holds only the npm shim (2,517 defs). Its eight kinks, recounted here
against 14,047 defs of real compiler.

| # (PR #538) | kink | count there | count here |
|---|---|---|---|
| 1 | exported-declaration initializer bodies are not call defs | 413 sites | 53 exported composite initializers in `src/**` (non-`.d.ts`); the loss folds into the 1,358 no-covering-def sites of section 6 row 1. Matrix reprobed on this binary and byte-identical to PR #538's |
| 2 | class-field arrow bodies are not call defs | 0, verified by fixture only | 31 class-field arrows in `src/**`, first real instances |
| 3 | receiver-blind method binding mints a wrong edge | 642 of 8,025 (8.0%) | 3,175 of 75,089 (4.2%). Top target moves from `generate-encoder.ts:push` to `src/compiler/tracing.ts:push`, 2,064 edges |
| 4 | a bodiless `.d.ts` declaration wins the name match | 172 edges, 135 into `lib.es2015.reflect.d.ts` | 105 edges: `src/lib/es5.d.ts` 54, `es2015.reflect.d.ts` 29, `esnext.iterator.d.ts` 12, `es2020.bigint.d.ts` 8, `dom.generated.d.ts` 2 |
| 5 | a default import's local alias never resolves | 3 sites, buried 8 of 20 largest unreachable | **zero cost here**. 18 `default` specifiers, every one naming an npm or node package outside the universe, 2 call sites total, 0 lost edges |
| 6 | a lambda caller has no `node` row to join to | 2,993 of 8,025 (37.3%) | 17,592 of 75,089 (23.4%), 6,476 distinct closures. Mechanism named: the resolver mints `closure@<byte offset>` and the call plane mints `name: null` |
| 7 | a UTF-16 source yields zero facts, rc 0, no diagnostic | 7 of 12,967 | 7 of 19,117 `tests/cases` files, 0 of 701 `src/` files |
| 8 | invalid UTF-8 exits 0 where `--help` promises 1 | 1 of 12,967 | 1 of 19,117: `tests/cases/compiler/corrupted.ts`, 8 bytes, `c6 1f bc 03 c1 03 19 1f` |

Reprobed exported-initializer matrix on this binary:

| declaration | def minted |
|---|---|
| `const c1 = arrow` | `function` named `c1` |
| `export const c2 = arrow` | `function` named `c2` |
| `const o1 = { k: arrow }` | `lambda` |
| `export const o2 = { k: arrow }` | none |
| `const a1 = [arrow]` | `lambda` |
| `export const a2 = [arrow]` | none |
| `export default { k: arrow }` | none |
| `class C { handler = arrow }` | none |
| `class C { method() {} }` | `method` |

`src/lang/ts.rs:1570-1574` states the exclusion and attributes it to v5
emission-set parity, so this remains a ported decision rather than an accident.

The brief's guess that `export * from` barrels would be the shape here is
confirmed and is the largest new finding after the top-level drop. The
`namespace ts { }` body shape from PR #528 does not occur: the compiler has
been ES modules since 5.0.

## 8. What stays untested and why

| area | why |
|---|---|
| any change under `v6/sprefa-extract/src/**` | forbidden to this lane; two fix lanes own that tree |
| `--family df`, `--family cfg`, `--family data` | the brief scopes this lane to the call plane and the crawl. The one df fact this crawl needed, the `var_read` for a function reference, is recorded in the `function_ref_as_value.ts` fixture |
| `--family diet_scip` as its own step | it is the same name-match arm over the same files as `--resolve --family call`; running both measures one thing twice |
| `--scip-deps` and `--deps` graded against madge | `--help` already carries a graded recall/precision number for `--scip-deps`, and this lane used `--deps` only as an oracle for the barrel closure |
| `tests/baselines/**` | not the program, and the brief excludes it |
| `tests/cases/**` in steps 2 to 5 | the brief scopes it to step 1 as a parser stress row; the files are standalone snippets with no shared resolution universe |
| a crawl over `scip_fn_edge` at full corpus width | `src/compiler/checker.ts` is absent from the index (section 5.1), so any whole-corpus scip reachability number would be measured over a different program. The side-by-side in 5.2 is restricted to the 7,326 defs both sides carry |
| a serial rerun of all 19,818 files | the `ms_parallel8` column is inflated 3x to 8x; the 20 largest `src/` files and the 8 slowest `tests/cases` files were remeasured serially and the rest carry no timing claim |
| scip over `tests/**` | `scip-typescript` needs one tsconfig per root; the test corpus has none and is not the program |
| a fix for any row in section 6 | analysis lane |

## 9. Fixes (lane `fix-extract-ts-crawl`, PR against `origin/main` b9b98e3af)

Section 6's rows 1, 5, 6 and 8 landed. Row 3 is blocked and row 4 is out of
scope, both stated below. Every number rerun over the same corpus
(`/Users/chrishafley/projects/TypeScript-5.9` @ `7e133bea1`, 701 `src/**` files)
with `ts5.crawl.py` unchanged, plus `ts5.crawl.module.py`, one `sed` off it that
adds `module` to `DEF_KINDS` so the new `<module>` def is a graph node.

### 9.1 The receipt

| binary | `resolved_edge` | defs | A_strict | union strict | union folded |
|---|---|---|---|---|---|
| `origin/main` b9b98e3af | 75,089 | 14,047 | 3,509 | 5,854 | 7,344 |
| + kink 1 only (`fa300d2c8`) | 75,893 | 14,438 | **3,854** | **6,110** | **7,683** |
| + kinks 1, 3, 4, 5 (this branch) | 62,755 | 14,438 | **977** | **2,497** | 6,146 |

Rows 2 and 3 read `ts5.crawl.module.py`; row 1 has no `module` def to see, so
both scripts give it the same numbers. This branch under the STOCK script reads
A_strict 566 / union strict 2,244, because a `<module>` caller is not a
`function` or a `method` and the stock `DEF_KINDS` cannot seed or traverse it.

### 9.2 Kink 1 is a clean win, kink 3 is a 4x reachability regression

Kink 1 alone: +804 edges, +391 defs, A_strict 3,509 -> 3,854, union strict
5,854 -> 6,110. Every gained edge carries `caller_name: "<module>"`.

Kink 3 costs 13,138 edges and takes A_strict 3,854 -> 977. The lost edges,
classed by the kind of the def they named:

| callee def kind | lost edges | leaders |
|---|---|---|
| `function` | 8,618 | `push` 2,064, `map` 481, `getTypeChecker` 173, `createExpressionStatement` 135 |
| `method` | 4,346 | `runQueuedTimeoutCallbacks` 711, `executeCommandSeq` 683, `getStart` 184 |
| no def row (a builtin) | 50 | `getOwnPropertyDescriptor` 13, `next` 11, `setPrototypeOf` 9 |

Section 6 measured 3,175 edges as WRONG under this rule. The rule removes
13,138, so roughly 9,900 of the removals were edges a call graph wanted:
`program.getTypeChecker()` and `factory.createCallExpression()` go the same way
`out.push(x)` does. The def kind does not separate them — TypeScript builds its
public API out of free functions closed over by a factory object, so
`getTypeChecker` and `tracing.ts:push` are both `kind=function`, and
`collectionsImpl.ts:keys` is a `method` that was wrong before.

The `src/lib/*.d.ts` route was probed and does not work: a bodiless declaration
mints no CallF def, so `push` has 0 def rows under `src/lib/` and the corpus
cannot be asked which names are ECMAScript builtins.

**The discriminator this needs is kink 2** (the import closure): the receiver's
type is out of reach, but "the file that declares this name is a file I import"
keeps `program.getTypeChecker()` and still drops `out.push(x)`. Kink 3 landing
before kink 2 is what produces the regression. Reverting only the block, not
the phase-1 `callee_path` it rides on, is the `unknown_receiver` call in
`Resolve<CallF>` (`src/lang/ts.rs`), three lines.

### 9.3 Kink 2 was not attempted: it does not fit inside a lang arm

`Resolve<CallF>::resolve(&self, output, cx)` sees one file's `ExtractOutput` and
the `ProjectCx`. Following `./barrel.js` needs the importing file's own PATH and
the barrel file's specifier rows, and neither is reachable:

| what is needed | where it would come from | state |
|---|---|---|
| this file's project-relative path | not a `Resolve` parameter | `src/project.rs:817-832` passes `output` and `cx` only |
| the corpus file list | `ProjectCx.files` | `pub struct FileSet;`, a unit struct (`src/types.rs:1428-1430`) |
| another file's specifier rows | `ProjectCx.indexes` | `IndexBag` carries `def_index`, `scip_index`, `joined_documents` and nothing else (`src/types.rs:1449-1453`) |

The shape it wants is a module-graph slot on `IndexBag`, built once per refresh
from the phase-1 outputs beside `build_def_index`, which is `src/types.rs` plus
`src/project.rs`. Both are outside this lane's ownership. Hailed to the
coordinator as `m-e66dc63d`.

### 9.4 Kink 4 landed half

The `position=value` reference row is in the stream. The resolve leg is not:
the edge needs a `CallEdgeKind::ValueRef`, and `CallEdgeKind` is matched
EXHAUSTIVELY at `tests/golden_parity.rs:781-798` and `:987-1005`, so a third
variant does not compile without editing a file outside this lane's ownership.
This is why the table in 9.1 shows no edge gain from kink 4.

### 9.5 Rows not touched

| row | why |
|---|---|
| the `node` record carries no exported flag | brief scopes it out (a wire change) |
| exact mode drops a document over ~1 MB | brief scopes it out; the defect is in `scip-typescript` |
| a `closure@<offset>` caller has no `node` row | the rust lane owns the closure fold (`src/project.rs` `caller_name`); a generic fold is a later decision |
| peak RSS scales with nesting depth | not in the brief |

### 9.6 Two registrations this lane could not make

| gap | file | what is missing |
|---|---|---|
| the `ts::MODULE` ext tag is unpinned | `tests/6_kind_vocab.rs` `EXT_KINDS` | `("ts::MODULE", ts::MODULE.as_str())`; the tag-collision and byte-stability tests iterate that list, so a new ext tag not on it is unasserted |
| the `reference` schema line still says `position=<goal\|head_arg\|term_arg>` | `src/schema.rs:37` | it was already missing `closure`; `value` makes four |
