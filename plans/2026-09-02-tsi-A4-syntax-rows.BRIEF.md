# brief: TSI A4, syntax-mode TSI rows for ts and rust

Lane: `feature/tsi-a4-syntax-rows`. Base: the `origin/main` sha AFTER the A3 PR merges (coordinator states it; `TsiSink` and `REGISTRY` come from A3).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, `## Decisions`: the relation list; mode contract line 1 ("Syntax mode emits candidate facts and partial coverage").
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` section 4 (last paragraph: what the syntax tier can and cannot say), section 5 (identity rules 1, 4), section 10 (cases "readonly and optional", "mapped type" syntax half).
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.visual.human.unga.md` section 4, the construct table.
- Landed: A1 (`src/tsi/types.rs`, `--witness`, `wire.rs` flatten with the `witness` bool), A3 (`src/tsi/registry.rs` `REGISTRY`, `src/tsi/sink.rs` `TsiSink`).

Delivers criterion 4's row half for ts and rust. Go and kotlin are a follow-up arc, named in the PR as not built.

## What is wrong today

The syntax tier records a type edge as `TypeEdgeCandidate { owner, to, kind }` (`src/types.rs:342`): no label, no position, no optional or readonly. `readonly` is read at `src/lang/ts.rs:1001` as a filter and never emitted. Generic parameters are candidates only through their bounds (`src/lang/rust_type_edges.rs:177-201`, `src/lang/ts.rs:841`). Nothing spells a product, a sum, or a callable's ordered inputs.

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/types.rs` | `TypeFAux` (`:396`) gains `pub tsi: Vec<crate::tsi::FactOut>`; `TypeEdgeCandidate` untouched |
| `v6/sprefa-extract/src/lang/ts.rs` | a `tsi_rows` pass beside `edge_candidates` (`:754`) writing into a per-file `TsiSink`, then `sink.aux.tsi = tsi.facts` |
| `v6/sprefa-extract/src/lang/rust_type_edges.rs` | the rust twin beside `edge_candidates` (`:21`) |
| `v6/sprefa-extract/src/wire.rs` | `flatten_type`: when `witness` is on, one `FlatFact::Fact` per `aux.tsi` row after the `sig` rows; when off, nothing (goldens byte-identical) |
| `v6/sprefa-extract/src/schema.rs` | the `TSI ENVELOPE` paragraph lists which relations the syntax tier emits per language |
| `v6/sprefa-extract/tests/fixtures/tsi/probe.ts`, `probe.rs` (new) | the issue's reproduction fixtures: `Mapper<T>` interface / trait, `User<T>` extending / implementing it, a generic `map` method, ts `readonly id: T` and `name?: string`, rust `type Output = Vec<T>` |
| `v6/sprefa-extract/tests/99_syntax_tsi_rows.rs` (new) | tests below |

Forbidden: `src/tsi/**` (missing capability = STOP AND REPORT with the exact signature you needed), `src/project.rs`, `src/bin/extract.rs`, `src/lang/ts_checker*`, `src/lang/rust_checker*`, `src/lang/{go,kotlin,python}.rs`, `tests/fixtures/resolve/**`, `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

## Rows the syntax tier emits, per declaration

Ids are file-local, minted by `TsiSink::fresh_id`. A written type name gets ONE `tsi.type` id per distinct text per file (a `HashMap<NameId, u32>` inside the pass), with `tsi.origin(Id, <lang>, span)` at its FIRST occurrence's span. A declaration's own id is minted at the declaration and `tsi.origin` points at its name span. `<lang>` is the atom `ts` or `rust`.

| construct | ts source | rust source | rows |
|---|---|---|---|
| struct / class / interface | `ts::Class`, `TSInterfaceDeclaration` | `syn::ItemStruct` | `tsi.type(Own)`, `tsi.product(Own)`, `tsi.origin(Own, lang, nameSpan)`; ts interface adds `ts.interface(Own)` |
| enum / union alias | `TSTypeAliasDeclaration` whose type is a union; `TSEnumDeclaration` | `syn::ItemEnum` | `tsi.type`, `tsi.sum`, `tsi.origin`; one `tsi.edge(E, Own, <variantName or alternative text>, Target, pos)` per alternative |
| trait | | `syn::ItemTrait` | `tsi.type`, `tsi.origin`, `rust.trait(Own)` |
| field | `PropertyDefinition`, `TSPropertySignature`, constructor parameter properties (`ts.rs:990-1012`) | `field_candidates` (`rust_type_edges.rs:163`) | `tsi.edge(E, Own, fieldName, Target, pos)`; pos = declaration order from 0; Target = the `tsi.type` id of the written type text (whole annotation text, `T[]` stays `T[]`) |
| optional | `prop.optional` | | `ts.optional(E)` |
| readonly | `prop.readonly`, constructor `fp.readonly` | | `ts.readonly(E)` |
| generic parameter | `type_parameters` | `generics.params`, `GenericParam::Type` | `tsi.type(P)`, `tsi.parameter(P, Own, pos, unspecified)`, `tsi.origin(P, lang, span)`; each bound: `tsi.edge(E, P, "bound", Target, pos)` |
| extends / implements / impl Trait for | `interface.extends`, `class.super_class`, `class.implements` | `syn::ItemImpl` with `trait_` | `tsi.conforms(Own, Target, syntax)`; rust adds `rust.impl(ImplId, Own, Target)` |
| method / function / arrow | `ts::Function`, class methods | `syn::ItemFn`, `ImplItem::Fn` | `tsi.type(F)`, `tsi.callable(F)`, `tsi.origin`; `tsi.input(F, pos, Target)` per param; `tsi.output(F, 0, Target)` for a written return type |
| type alias, non-union | `TSTypeAliasDeclaration` | `syn::ItemType` | NO own type id: `tsi.symbol(S)`, `tsi.denotes(S, TargetT)` where TargetT is the written target's id; if the aliased type is a `TSTypeReference` with `type_arguments` (`Partial<User<number>>`): `tsi.called(Own, Callee, List)`, `tsi.argument(List, pos, Target)` per argument, Callee = the written name's id; a mapped or conditional alias body emits NO body row |

Not emitted by this tier, by contract (section 4 last paragraph): `tsi.called` for anything other than a written `Name<Args>` reference, `ts.mapped`, `ts.conditional`, `rust.assoc`, `rust.lifetime`, `rust.ownership`, `tsi.has_type`, `tsi.denotes`, `tsi.primitive`, `tsi.subtype`, `tsi.assignable`, `tsi.equivalent`, variance other than `invariant`.

Coverage: the per-file syntax run (A1's `Run` row) already emits `coverage partial` per family; add one `coverage partial` row per `tsi.*`/`ts.*`/`rust.*` relation this pass emitted at least once. Relations never emitted get no coverage row.

Every `fact()` call goes through `TsiSink::fact(relation, args)`, which checks the registry under `debug_assert!`. A relation name is a `&'static str` from `crate::tsi::registry` constants if A3 exported them, else a string literal that matches `REGISTRY` exactly.

## Tests, `tests/99_syntax_tsi_rows.rs`

All cases run `extract --witness --family type <fixture>` and parse rows with `serde_json::from_str::<FlatFact>`.

| case | fixture | expected |
|---|---|---|
| flag off unchanged | `probe.ts` without `--witness` | no `fact` record and no `fact` key; and `cargo test --test golden_parity` passes |
| product and fields | `probe.ts` `User<T>` | `tsi.product(User)`; `tsi.edge(_, User, "id", T, 0)` with `ts.readonly` on that edge id; `tsi.edge(_, User, "name", string, 1)` with `ts.optional` |
| interface | `probe.ts` `Mapper<T>` | `ts.interface(Mapper)`, `tsi.parameter(T, Mapper, 0, unspecified)` |
| conforms | `probe.ts` `User<T> extends Mapper<T>` (or `implements`) | `tsi.conforms(User, Mapper, syntax)` |
| callable | `probe.ts` `map<U>(f: (t: T) => U): U` | `tsi.callable(map)`, `tsi.input(map, 0, <written text>)`, `tsi.output(map, 0, U)`, `tsi.parameter(U, map, 0, unspecified)` |
| generic argument, written | `type Q = Partial<User<number>>` | `tsi.called(Q, Partial, L)`, `tsi.argument(L, 0, <id of "User<number>">)`; zero `tsi.edge` rows owned by Q; `coverage partial` for `tsi.edge` |
| rust struct | `probe.rs` `struct User<T> { id: T, name: Option<String> }` | `tsi.product`, two edges positions 0 and 1, `tsi.parameter(T, User, 0, unspecified)` |
| rust trait and impl | `probe.rs` `trait Mapper<T>`, `impl<T> Mapper<T> for User<T>` | `rust.trait(Mapper)`, `rust.impl(_, User, Mapper)`, `tsi.conforms(User, Mapper, syntax)`; zero `rust.assoc` rows |
| one id per written name | `probe.ts` | every `tsi.type` id has exactly one `tsi.origin`; two fields typed `string` share one target id |
| registry clean | both fixtures | every `fact` row's relation is in `REGISTRY` with matching arity; and piping the stream into `extract --ingest /dev/stdin` returns rc=0 |
| dangling free | both fixtures | every `{"id"}` argument names a `tsi.type` or an edge id minted in the same stream |

Header carries a SABOTAGE RECEIPT: on the base sha `--witness --family type probe.ts` emits zero `fact` records.

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli --test 99_syntax_tsi_rows --test golden_parity --test 1_resolve_cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo run -q --features cli --bin extract -- --witness --family type tests/fixtures/tsi/probe.ts | cargo run -q --features cli --bin extract -- --ingest /dev/stdin > /dev/null; echo rc=$?
```

## Cost law

With `--witness` off the pass still runs (the rows are cheap and the flatten skips them), UNLESS the ratchet in `tests/bench` moves; if it moves, gate the pass on a `witness` bool threaded through `ExtractRequest` and say so in the PR. No per-row allocation beyond the `Vec<Arg>`; type-name lookups go through the one `HashMap` per file.

## Style laws

- No `eprintln!`; `tracing` only.
- Comments: constraints only. No dates, no arc names.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- Variable names descriptive, never single letters, in tests and fixtures alike (`mapper_trait_id`, not `m`).

## Done

PR titled `extract: syntax-tier TSI rows for ts and rust under --witness (TSI A4)`.
`git diff --stat <base>...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A4 PR #<n>: 99_syntax_tsi_rows N tests, goldens byte-identical, ingest rc=0"`.
