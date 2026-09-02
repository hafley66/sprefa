# brief: TSI A5, TypeScript semantic mode

Lane: `feature/tsi-a5-ts-semantic`. Base: the `origin/main` sha AFTER BOTH the A2 and A4 PRs merge (coordinator states it; A2 owns the semantic `run` row on the resolve path, A4 owns the syntax rows and `tests/fixtures/tsi/probe.ts`).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, `## Decisions`: the relation list; identity rules 1-4; mode contract lines 2-5 (semantic emits every reachable fact; native operators in `ts.*`; `complete` only after enumerating every reachable row; unsupported stays `partial` with a diagnostic; recursion closes through ids).
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` section 7 (the `ts_checker.mjs` `tsi` arm), section 10 (cases "generic argument", "mapped type" semantic half, "two witnesses").
- Landed: A1 `src/tsi/types.rs`; A3 `src/tsi/{registry,sink}.rs` (`REGISTRY`, `TsiSink`); A2 `--witness` over `--resolve`, the `run mode=semantic tool=tsc` row (`src/project.rs`); A4 `tests/fixtures/tsi/probe.ts`.

Delivers criteria 5 (ts half) and 6.

## What is wrong today

`ts_checker.mjs:180-217` answers per reference site: one `[start, end, name, dstPath, dstName, dstOffset]` row per call or type reference. It never enumerates a type's shape. `TsCheckerAnswers` (`ts_checker.rs:40`) carries `calls` and `types` only.

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/lang/ts_checker.mjs` | a `tsi` arm: `emitTsi(ts, checker, state)` per wanted file, output on a `tsi` key of the per-file JSON (`{path, calls, types, tsi, coverage}`), gated by `request.tsi === true` so the existing tiers' output is byte-identical |
| `v6/sprefa-extract/src/lang/ts_checker.rs` | `TsCheckerAnswers` gains `pub tsi: Vec<FactOut>` and `pub coverage: Vec<(String, bool, Option<String>)>` (relation, complete, diagnostic); `answer()` sends `tsi: true` when asked; `TsCheckerIndex` gains `pub fn semantic_rows(&self) -> &[FactOut]` and `coverage()` |
| `v6/sprefa-extract/src/tsi/semantic.rs` (new) | `pub trait SemanticRows { fn facts(&self) -> &[FactOut]; fn coverage(&self) -> &[CoverageClaim]; }`, `pub struct CoverageClaim { pub relation: String, pub complete: bool, pub diagnostic: Option<String> }`, and `pub fn emit_semantic(run: u32, rows: &dyn SemanticRows, out: &mut Vec<FlatFact>)`: one `Fact` per row, one `Witness { method: CheckerWalk }` per fact on `run`, one `Coverage` per claim, one `Diagnostic` beside every `partial` claim |
| `v6/sprefa-extract/src/tsi/mod.rs` | `pub mod semantic;` |
| `v6/sprefa-extract/src/project.rs` | ONE hunk: in `resolve_project`, under `witness`, after A2's semantic `run` row for tsc, call `emit_semantic(run_id, &ts_index, &mut out)`. Nothing else in this file |
| `v6/sprefa-extract/tests/101_ts_semantic_tsi.rs` (new) | tests below |
| `v6/sprefa-extract/tests/fixtures/tsi/tsconfig.json` (new, only if `compilerOptions` (`ts_checker.mjs:83`) needs one for `probe.ts`) | |

Forbidden: `src/tsi/{types,registry,sink,ingest}.rs`, `src/lang/ts.rs`, `src/lang/rust_checker*`, `src/wire.rs`, `src/bin/extract.rs`, `src/types.rs`, `tests/fixtures/resolve/**`, `tests/fixtures/tsi/probe.ts` (A4's; add a second fixture if you need more constructs), `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

## The `tsi` arm, `ts_checker.mjs`

Ids are run-local across the whole program: one `Map<ts.Type, number>` and one `Map<ts.Symbol, number>` in `state`, minted on first sight. A type seen twice is one id (this is what closes recursion: `Node<T> = { next: Node<T> }` emits the `next` edge whose target is the owner's own id and stops). Rows are `[relation, arg, arg, ...]` with args in the wire's tagged shape: `{"id":n}`, `{"span":[digest,start,end]}` (`digest` = the file's content digest the Rust side already knows; the mjs writes the `supplied` path and the Rust side substitutes the digest), `{"text":s}`, `{"int":n}`, `{"atom":s}`.

Per wanted file, walk every declaration (`ts.isInterfaceDeclaration`, `isClassDeclaration`, `isTypeAliasDeclaration`, `isEnumDeclaration`, `isFunctionDeclaration`, methods, variable declarations with a function initializer):

| ts API | rows |
|---|---|
| `checker.getDeclaredTypeOfSymbol(sym)` (declarations) / `checker.getTypeAtLocation(node)` (occurrences) | `tsi.type(T)`, `tsi.denotes(S, T)`, `tsi.origin(T, ts, nameSpan)` at the declaring name |
| `type.flags & ts.TypeFlags.Object` and `objectFlags & (Class|Interface|Anonymous|Mapped)` | `tsi.product(T)`; interface adds `ts.interface(T)`; `Mapped` adds `ts.mapped(T, keyParamT, constraintT, templateT)` from `type.typeParameter`, `constraintType`, `templateType` |
| `type.flags & Union` | `tsi.sum(T)`; one `tsi.edge(E, T, "", memberT, pos)` per `type.types` member, pos = order in `types` |
| `type.flags & (String|Number|Boolean|BigInt|Symbol|Void|Undefined|Null|Never|Unknown|Any|StringLiteral|NumberLiteral|BooleanLiteral)` | `tsi.primitive(T, <class atom>)` where the atom is the flag name in lowercase; literal types carry the atom of their widened class |
| `checker.getPropertiesOfType(type)` | `tsi.edge(E, T, propName, propT, pos)`, pos = declaration order (`prop.declarations[0].pos` sort); `ts.optional(E)` if `prop.flags & SymbolFlags.Optional`; `ts.readonly(E)` if `checker.isReadonlySymbol(prop)` or the declaration has a `readonly` modifier |
| `type.typeParameters` / `sig.typeParameters` | `tsi.type(P)`, `tsi.parameter(P, T, pos, invariant)`, `tsi.origin(P, ts, span)`; a constraint adds `tsi.edge(E, P, "bound", constraintT, 0)` |
| `checker.getSignaturesOfType(type, ts.SignatureKind.Call)` and `Construct` | `tsi.callable(T)` once if any signature; per signature index k: `tsi.input(T, k*1000+i, paramT)` per parameter i, `tsi.output(T, k, returnT)`. State the k*1000 packing in a comment; a signature with over 1000 parameters is a named stop |
| `objectFlags & ObjectFlags.Reference` with `checker.getTypeArguments(type)` non-empty | `tsi.called(T, targetT, L)`, `tsi.argument(L, i, argT)`; `targetT` = id of `type.target` |
| `type.flags & Conditional` | `ts.conditional(T, checkT, extendsT, trueT, falseT)` from `type.root.node` via `checker.getTypeFromTypeNode` |
| heritage clauses (`ts.getEffectiveBaseTypeNode`, `getEffectiveImplementsTypeNodes`) | `tsi.conforms(T, baseT, declared)` |
| every `ts.isIdentifier(node)` occurrence in a wanted file whose `getTypeAtLocation` is not `errorType` | `tsi.has_type(span, T)`; the file's own span, digest substituted on the Rust side |

Every type id an edge, argument, input, output or has_type row names MUST also have a `tsi.type` row: after the walk, a second pass emits `tsi.type` for every id in the Map that lacks one (types reached only as targets: `string`, a lib type, a dependency type). Lib and dependency types get `tsi.origin` with the declaring file's path as text if it is outside the corpus; that is still a row, never a missing id. `--ingest` on the stream is the receipt.

Coverage claims (per whole run, not per file): `complete` for `tsi.type, tsi.denotes, tsi.has_type, tsi.origin, tsi.product, tsi.sum, tsi.callable, tsi.primitive, tsi.edge, tsi.parameter, tsi.called, tsi.argument, tsi.input, tsi.output, ts.interface, ts.optional, ts.readonly, ts.mapped, ts.conditional`. `partial` with a diagnostic string for `tsi.conforms` ("declared heritage only; structural conformance not enumerated"), `tsi.subtype`, `tsi.assignable`, `tsi.equivalent` ("not enumerated"). Variance is always the atom `invariant`; `tsi.parameter` is still `complete` (every parameter is enumerated; the variance column is a fixed value, say so in the schema paragraph).

## Tests, `tests/101_ts_semantic_tsi.rs`

`#![cfg(feature = "ts-checker")]`, driven the way `tests/92_ts_checker.rs` does (`SPREFA_TS_CHECKER_TYPESCRIPT`). Command for every case: `extract --witness --resolve --family type --project-root tests/fixtures/tsi --ts-checker tests/fixtures/tsi/probe.ts`.

| case | expected |
|---|---|
| two runs | `run mode=syntax tool=extract` and `run mode=semantic tool=tsc`; every `fact` with a `checker_walk` witness names the semantic run |
| product and fields | `User`: `tsi.product`; edges `id` pos 0 with `ts.readonly`, `name` pos 1 with `ts.optional`; the `name` edge's target is `tsi.primitive(_, string)` |
| generic argument | the `User<number>` occurrence: `tsi.called(R, User, L)`, `tsi.argument(L, 0, N)`, `tsi.primitive(N, number)`, and a `tsi.has_type(span, R)` at the occurrence |
| mapped | `type Q = Partial<User<number>>`: `ts.mapped(Q, ...)`, and `getPropertiesOfType(Q)` yields two edges both `ts.optional`; the syntax run's `tsi.edge` rows owned by Q are zero and its `coverage partial` for `tsi.edge` is still present on the syntax run |
| callable | `map`: `tsi.callable`, `tsi.input(map, 0, F)` where F is itself `tsi.callable` with `tsi.input(F, 0, T)` and `tsi.output(F, 0, U)`; `tsi.output(map, 0, U)` |
| conforms | `tsi.conforms(User, Mapper, declared)`; `coverage partial` for `tsi.conforms` with one `diagnostic` row naming it |
| complete claims | `coverage complete` for exactly the relation set above, on the semantic run only |
| recursion | a second fixture `tests/fixtures/tsi/recursive.ts` with `interface Node<T> { value: T; next: Node<T> }`: the `next` edge's target id equals `Node`'s own id; the walk terminates (10s cap) |
| every id declared | every `{"id"}` in the stream has a `tsi.type` or edge/list minting row; `extract --ingest` on the stream returns rc=0 |
| tsc output unchanged | with `tsi` off (no `--witness`) the `92_ts_checker` cases still pass, and the driver's stdout has no `tsi` key |

Header carries a SABOTAGE RECEIPT: on the base sha the semantic run row exists (A2) and carries zero `checker_walk` witnesses.

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli,ts-checker --test 101_ts_semantic_tsi --test 92_ts_checker --test 98_resolve_witness 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli --test golden_parity --test 1_resolve_cli 2>&1 | tail -3
```

## Cost law

The `tsi` arm runs only when `request.tsi` is true, which is only `--witness`. `92_ts_checker`'s timing assertions, if any, must not move. `getPropertiesOfType` over lib types is bounded by the Map: a lib type reached as a target gets `tsi.type` and `tsi.origin` only, its properties are NOT walked (say so in the coverage diagnostic? No: `tsi.edge` stays `complete` for corpus-declared owners; add the sentence "edges are enumerated for corpus-declared owners; a lib or dependency type is a leaf" to the schema paragraph and to `tsi.edge`'s diagnostic as a `partial`... DECIDE: `tsi.edge` is `partial` with that diagnostic. Complete would be a false claim).

## Style laws

- No `eprintln!`; `tracing` only; the mjs writes diagnostics to stderr the way it does today (`process.stderr.write`).
- Comments: constraints only. No dates, no arc names.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- Descriptive names in the mjs too: `ownerTypeId`, never `t`.

## Done

PR titled `extract: ts semantic mode, the tsc walk emits TSI rows with complete coverage (TSI A5)`.
`git diff --stat <base>...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A5 PR #<n>: 101_ts_semantic_tsi N tests, 92 unchanged, ingest rc=0"`.
