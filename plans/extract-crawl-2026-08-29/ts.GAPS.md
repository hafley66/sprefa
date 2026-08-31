# ts call-edge gap classes (lane `chore-ts-gap-classes`, measure only)

Binary at the #566 receipt commit (58811b9bd, ed833559a on top; no src/ changes
this lane). One-process rerun over `~/projects/TypeScript-5.9/src`
(`extract --resolve --project-root`, timeout 10, rc=0), normalized with
`normalize.py`, `ts5.parse.call.tsv` overwritten, benched against the
TypeChecker oracle: |a| 59,311, |b| 59,356, both 41,547, recall 70.05%,
precision 70.00% - matches the #566 receipt exactly.

Sets derived from `bench.py ts5.parse.call.tsv ts5.oracle.call.tsv`:

| set | rows | sample |
|---|---:|---:|
| oracle-only (`ts.gaps.oracle_only.tsv`) | 17,809 | 300 (seed 42) |
| ours-only (`ts.gaps.ours_only.tsv`) | 17,764 | 200 (seed 42) |

Per-row classes beside this file: `ts.gaps.oracle_only.classes.tsv`,
`ts.gaps.ours_only.classes.tsv`, `ts.gaps.unresolved.classes.tsv`;
classifier `ts.gaps.classify.py`.

Count reconciliation: section 11.2's "7,567 ambiguous by name" is the
section-10.3 name-match re-judge. The drops channel this lane classifies
records 7,638 `unresolved` rows, each carrying the site
(7,095 reason `inferred` + 543 reason `ambiguous`); all 7,638 were
classified, bucket by the receiver declaration at the site.

## Contents

- [oracle-only classes](#oracle-only-classes)
- [ours-only classes](#ours-only-classes)
- [unresolved drop classes (all 7,638)](#unresolved-drop-classes-all-7638)
- [Which leg takes each class](#legs)
- [Classifier caveats](#classifier-caveats)

## oracle-only classes

Project = sample count x (17,809 / 300), rounded.

| class | sample | projected | example (src L# \| dst) |
|---|---:|---:|---|
| caller naming: the site's enclosing fn is a closure we name `closure@N`; the oracle names the enclosing fn, so the rows can never intersect (87/104 bare rows carry a `closure@N` ours-row on the same callee) | 87 | 5,165 | `compiler/builder.ts` getNonIncrementalBuildInfoRoots L2448 \| `path.ts` toPath |
| method on a namespace-merged / imported-module declaration: receiver `ts`, `factory`, `Debug`, `tracing` through `./_namespaces` barrels; our module plane never joins the member to the underlying fn | 43 | 2,553 | `compiler/binder.ts` getDeclarationName L743 \| `debug.ts` formatSyntaxKind |
| interface receiver needing implementer fan-out (`TypeCheckerHost`, `Program`, `SourceFile`) | 29 | 1,722 | `compiler/checker.ts` getDefaultResolutionModeForFile L4721 \| `types.ts` getDefaultResolutionModeForFile |
| unannotated receiver: type comes from the initializer result (`const printer = createPrinter()`), initializer callee cross-file | 29 | 1,722 | `compiler/checker.ts` typePredicateToStringWorker L6121 \| `types.ts` writeNode |
| interface receiver via destructuring: `const { factory } = context`; decl shape the walker does not read | 26 | 1,543 | `compiler/checker.ts` serializeEnum L6761 \| `types.ts` createStringLiteral |
| other: receiver decl not found in file (destructured const, `!`-suffixed field, outer-scope binding) | 21 | 1,247 | `compiler/parser.ts` parseArrayLiteralExpression L2455 \| `scanner.ts` hasPrecedingLineBreak |
| other: site not locatable by scan (site line picks a def or comment; needs the walker's real span) | 14 | 831 | `emitter.ts` emitListItemWithParenthesizerRule \| same-file callee |
| bare call, callee def is a method ref / property fn (`performance.enter`, `reportNonlocalAugmentation` as value) | 9 | 534 | `compiler/emitter.ts` emitFiles L759 \| `performance.ts` enter |
| other: `this.<field>` member read still unbound (`this.program`, `this.projectService`) | 8 | 475 | `server/editorServices.ts` onWildCardDirectoryWatcherInvoke L2044 \| getConfiguredProjectByCanonicalConfigFilePath |
| bare call, single def, still missed (mixed: interface-impl edges, callback args) | 8 | 475 | `harness/fourslashInterfaceImpl.ts` select \| `fourslashImpl.ts` select |
| union-typed receiver (`Scanner \| undefined`, `SourceMapGenerator \| undefined`) | 5 | 297 | `factory/nodeFactory.ts` getCookedText L7246 \| `scanner.ts` setText |
| callback / func-typed param or field (`sys` object literal members) | 4 | 237 | `unittests/tsc/incremental.ts` edit L461 \| `virtualFileSystemWithWatch.ts` readFile |
| other: concrete declared type (`textChanges.ChangeTracker` namespace-qualified anchor, `ConfiguredProject`) | 8 | 475 | `services/codefixes/importFixes.ts` promoteImportClause L1811 \| `textChanges.ts` insertImportSpecifierAtIndex |
| receiver from a call result, one hop (`parenthesizerRules().x()`, `getProgram()!`) | 3 | 178 | `factory/nodeFactory.ts` createAwaitExpression L3143 \| parenthesizeOperandOfPrefixUnary |

## ours-only classes

Project = sample count x (17,764 / 200). Edges the oracle lacks; mostly
naming and scope, matching the go lane's shape.

| class | sample | projected | example |
|---|---:|---:|---|
| caller naming: our caller is `closure@N`, the oracle records the enclosing fn (or the edge is closure-internal and the oracle has no site for it) | 162 | 14,389 | `compiler/binder.ts` closure@116812 L1060 \| isPropertyAccessExpression |
| other: receiver decl not found in file (same shape as oracle-only, site inside closures) | 26 | 2,309 | `unittests/helpers/virtualFileSystemWithWatch.ts` closure@17750 \| ensureLib |
| other: site not locatable by scan | 5 | 444 | `utilities.ts` suppressLeadingTrivia \| getFirstChild |
| bare call, callee def not found (same-file private helpers, test helpers) | 4 | 355 | `transformers/esDecorators.ts` visitAssignmentElement L2106 \| isAnonymousClassNeedingAssignedName |
| singletons (namespace-merged, `this.field`, interface fan-out) | 3 | 267 | `server/scriptVersionCache.ts` insertLines L131 \| LineNode |

## unresolved drop classes (all 7,638)

Actual counts, every row. `inferred:` rows are 7,095; `ambiguous:` rows are
543 (the walker saw the receiver but typed it `Ambiguous`).

| class | rows | share |
|---|---:|---:|
| unannotated receiver, type inferred from initializer (`q = createQueue()`, `printer`, `writer`, `sourceFile` consts) | 3,049 | 39.9% |
| interface receiver needing implementer fan-out (`Map.get/set`, arrays `.push`, `TypeChecker`, `SymbolTable`) | 2,140 | 28.0% |
| other: concrete declared type (class/alias/interface-qualified; `ChangeTracker` 276, `TestServerHost` 77, `WriterAggregator`, `ScriptInfo`, long tail of ~200 names) | 1,092 | 14.3% |
| union-typed receiver (`ambiguous` reason; `Map \| undefined`, `X \| null`) | 530 | 6.9% |
| other: receiver decl not found in file (`!`-suffixed fields, destructured bindings) | 470 | 6.2% |
| generic receiver `T extends X` (`T[]`, `V[]` in core.ts helpers) | 122 | 1.6% |
| other: primitive-typed receiver (builtin members on `string`/`Symbol`-typed) | 107 | 1.4% |
| callback / func-typed param or field (`(k, vSet)`, `() =>` annotations) | 55 | 0.7% |
| bare call traced with no receiver (rare; span aligned to bare callee) | 46 | 0.6% |
| method on a namespace-merged / imported-module declaration | 20 | 0.3% |
| other: caller naming closure@N / no-recv | 24 | 0.3% |
| receiver from a call result, one hop | 2 | <0.1% |

## Which leg takes each class

| class | owning fn | note |
|---|---|---|
| caller naming closure@N (both sets) | closure caller key, `src/project.rs:1004-1013` + the ts closure mirror | representational like go's `<fn>$N` mismatch; a `closure@N` row and its enclosing-fn row must share a canonical caller name, or one side must mirror the other. Largest single overlap blocker: ~5,165 oracle-only + 14,389 ours-only |
| namespace-merged receiver | `scan_module_specifiers` (`ts.rs:1292`) + module plane `modules.member` (`ts.rs:3519`) | the named import from a `_namespaces` barrel names a namespace object; join `recv.member` to that module's exported member defs. Projected ~2,553 oracle-only |
| interface receiver fan-out | `interface_member_defs` (`ts.rs:1540`) binds the signature already; the drop is upstream in `receiver_of` failing to bind the param decl - fix receiver binding first, implementer fan-out only where the oracle itself fans out (rare in ts: the oracle binds the interface signature) | ~1,722 oracle-only + 2,090 drops |
| unannotated const from initializer | `receiver_of` (`ts_receivers.rs:98`) one-hop `const x = f()`: the initializer callee is cross-file, so the return type needs `type_anchor` (`ts_receivers.rs:421`) + the module plane, like the go Fix-5 multi-hop extension | ~1,722 oracle-only + 3,049 drops (the biggest drop class) |
| interface receiver via destructuring | `receiver_of`: add destructuring bindings `const { factory } = context` to the scope | ~1,543 oracle-only |
| decl not found (`!` fields, for-of, destructuring) | `receiver_of` scope insertion (`ts_receivers.rs:66`) | ~1,247 oracle-only + 470 drops |
| `this.<field>` member read | `receiver_of`: the one-level field read stops at `!` suffixes and optional props | ~475 oracle-only |
| concrete declared type, namespace-qualified anchor (`textChanges.ChangeTracker`) | `type_anchor` + `receiver_member_target` (`ts_receivers.rs:421/469`): qualified type names need a two-segment module-plane bind | ~475 oracle-only + 1,092 drops |
| union receiver | `TypeBinding::Ambiguous` in `receiver_of`; narrowing is control-flow analysis, a compiler question | ~297 oracle-only + 530 drops |
| callback / func-typed param | `receiver_of`: seed func-typed params like the go field-type leg | ~237 oracle-only + 55 drops |
| generic receiver `T extends X` | `receiver_of`; substitution is a compiler question (same budget note as ts5.REPORT 11.5) | 122 drops |
| primitive receiver | `receiver_blind_builtin` (`ts.rs:3756`): extend `BUILTIN_MEMBERS` (`ts.rs:3625`) with the member names these sites spell | 107 drops |
| multi-hop call result | `receiver_of` bind plan; one hop already covered, two+ hops is the fixpoint (ts5.REPORT 11.5) | ~178 oracle-only |

## Classifier caveats

- Site spans are byte offsets; corpus files are read as bytes (CRLF shifts
  char offsets). The drops row's span covers the whole `recv.member`
  expression, so the callee is aligned at `end - len(detail)`.
- `MANUAL(no-span)` rows (14 oracle-only) had no locatable site by scan;
  their receiver class is unknown and they are shown as "other".
- Bare-call rows were re-attributed to the caller-naming class when the same
  callee has a `closure@N` row in ours (87/104 checked); the remainder keep
  the bare class.
- Receiver decl lookup is nearest-before-site text matching; nested arrows
  sharing a param name can mislabel, and the long tail of
  "concrete declared type" names was not individually opened.

## gap 2: bare calls (lane fix-extract-ts-codeql-gap-2, 2026-08-30)

Sample: 20 rows each of "other: bare call, local function" (61/300) and
"bare call, callee imported" (46/300) from
`ts.codeql_agreed_missed.sample300.classes.tsv`, site probe per caller file
(`extract --resolve --project-root`, grep the site byte span).

Reason histogram: there is none to write. A bare call that does not bind
emits NO record at all (neither resolved_edge nor unresolved; unresolved rows
carry only member calls, dynamic imports, computed members, spreads). The
miss is therefore a caller-naming or binding miss, read off the site span:

| mechanism | sample evidence | fix |
|---|---|---|
| arrow / fn-expr as the value of an `export const` object literal: lambda_entry_decl dropped VariableDeclaration, so the site fell back to `<module>` | program.ts `moduleResolutionNameAndModeGetter.getMode -> getModeForUsageLocation` | FIXED (commit 56892525c) |
| named fn-expr as an object property value (`[SyntaxKind.X]: function forEachChildInY(){}`): sites bound to `closure@N` / `<module>` instead of the fn-expr's own identifier | parser.ts forEachChildTable, 369 forEachChildIn* sites in that one file | FIXED (commit ec54f0c17) |
| bare callee bound to a destructured local whose initializer is unresolvable cross-file (`const { enter } = performance.createTimer(...)`): site dropped silently, needs one-hop return-type inference through an imported namespace | emitter.ts `emitFiles -> enter`, utilities.ts `createEvaluator` destructure f1-shape resolves, f2-shape does not | open, one-hop inference through imported namespaces |
| destructured function parameter binds fine (f1 shape) | utilities.ts `evaluate -> evaluateEntityNameExpression` | not the failing shape; the utilities miss is the f2 shape at a nested `const { ... } = <member call>` binding |
