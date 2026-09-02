# brief: TSI A6, Rust semantic mode

Lane: `feature/tsi-a6-rust-semantic`. Base: the `origin/main` sha AFTER the A5 PR merges (coordinator states it; A5 owns `src/tsi/semantic.rs` and the `project.rs` hook you plug into).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Contract

- `issues/extract-semantic-fact-roundtrip/item.md`, `## Decisions`: relation list; identity rules 1-4; mode contract; `rust.trait`, `rust.impl`, `rust.lifetime`, `rust.ownership` named; `rust.assoc` added by PLAN.md section 4.
- `plans/2026-09-02-extract-syntax-semantic-modes.PLAN.md` section 7 (the `rust_checker_ra.rs` walk), section 10 (cases "associated type", "lifetime and ownership").
- Landed: A3 `REGISTRY` (`rust.assoc [Id, Text, Id]`, `rust.impl [Id, Id, Id]`, `rust.lifetime [Id, Atom]`, `rust.ownership [Id, Atom]`); A5 `src/tsi/semantic.rs` (`SemanticRows`, `CoverageClaim`, `emit_semantic`) and its `project.rs` hook for the tsc index.

Delivers criteria 5 (rust half) and 7.

## What is wrong today

`rust_checker_ra.rs:13-110` walks call and type reference sites and answers per site (`method_call_ref`, `path_call_ref`, `type_ref`). No `hir::Adt`, `hir::Trait` or `hir::Impl` is enumerated; `CheckerAnswers` (`rust_checker.rs:37`) carries `calls` and `types` only.

## Files you own

| file | change |
|---|---|
| `v6/sprefa-extract/src/lang/rust_checker_ra.rs` | a `tsi_walk(sema, db, files, sink)` after the reference walk, gated by a `tsi: bool` argument on `answer()` |
| `v6/sprefa-extract/src/lang/rust_checker.rs` | `CheckerAnswers` gains `pub tsi: Vec<FactOut>` and `pub coverage: Vec<CoverageClaim>`; `RustCheckerIndex` implements `SemanticRows`; the `NotBuilt` arm of `answer()` returns empty vectors |
| `v6/sprefa-extract/src/project.rs` | ONE hunk beside A5's: `emit_semantic(run_id, &rust_index, &mut out)` under the rust-analyzer `run` row |
| `v6/sprefa-extract/tests/102_rust_semantic_tsi.rs` (new) | tests below |
| `v6/sprefa-extract/tests/fixtures/tsi/rust_probe/` (new) | a cargo package: `Cargo.toml`, `src/lib.rs` with `trait Mapper<T> { type Output; fn map(&self, t: T) -> Self::Output; }`, `struct User<T> { id: T, name: Option<String> }`, `impl<T> Mapper<T> for User<T> { type Output = Vec<T>; ... }`, `struct View<'a> { text: &'a str, owned: Box<User<u32>>, shared: std::rc::Rc<View<'a>> }`, `enum Shape { Circle(f64), Square { side: f64 } }` |

Forbidden: `src/tsi/**`, `src/lang/ts_checker*`, `src/lang/rust.rs`, `src/lang/rust_type_edges.rs`, `src/wire.rs`, `src/bin/extract.rs`, `src/types.rs`, `tests/fixtures/resolve/**`, `tests/fixtures/tsi/probe.*`, `v6/tsv2/**`, `v6/prolog/**`, `v7/**`, the issue file.

## The walk, `rust_checker_ra.rs`

Ids run-local across the workspace: `HashMap<hir::Type, u32>` is not available (`hir::Type` is not `Hash`); key on `hir::Type::display(db).to_string()` per crate for structural identity and on `hir::ModuleDef` for nominal identity (rule 1). Recursion: `Box<List>` inside `List` reaches `List`'s own `ModuleDef` id; no type is expanded twice.

Walk every module of every crate that owns a supplied file (`sema.to_module_def(file_id)`, then `module.declarations(db)`):

| hir item | rows |
|---|---|
| `ModuleDef::Adt(Adt::Struct)` | `tsi.type(T)`, `tsi.product(T)`, `tsi.origin(T, rust, nameSpan)`, `tsi.denotes(S, T)`; `struct.fields(db)`: `tsi.edge(E, T, fieldName, fieldT, pos)`; `struct.generic_params`... see generics row |
| `Adt::Enum` | `tsi.type`, `tsi.sum`, `tsi.origin`; per `variant`: `tsi.edge(E, T, variantName, variantT, pos)` where the variant's own id is a `tsi.product` with its fields as edges (tuple fields labeled `"0"`, `"1"`) |
| `Adt::Union` | as struct |
| `ModuleDef::Trait` | `tsi.type(T)`, `rust.trait(T)`, `tsi.origin`; `trait.items(db)`: `AssocItem::TypeAlias` -> `rust.assoc(T, aliasName, boundT or a fresh opaque id)`; `AssocItem::Function` -> the callable rows below, owned by the trait |
| `hir::Impl` (`hir::Impl::all_in_crate`) with `impl.trait_(db)` | `rust.impl(I, selfT, traitT)`, `tsi.conforms(selfT, traitT, declared)`; `impl.items(db)` `TypeAlias` -> `rust.assoc(selfT, name, targetT)` |
| `ModuleDef::Function`, `AssocItem::Function` | `tsi.type(F)`, `tsi.callable(F)`, `tsi.origin`; `f.params_without_self(db)`: `tsi.input(F, pos, paramT)`; `f.ret_type(db)`: `tsi.output(F, 0, retT)` |
| generic params (`hir::GenericDef::type_or_const_params`, `lifetime_params`) | type param: `tsi.type(P)`, `tsi.parameter(P, ownerT, pos, invariant)`, bounds -> `tsi.edge(E, P, "bound", traitT, k)`; lifetime param: `rust.lifetime(ownerT, name)` |
| field types (`hir::Type`) | `ty.is_reference()` -> `rust.ownership(E, shared)` or `exclusive` when `is_mutable_reference()`; `ty.as_adt()` named `Box`, `Rc`, `Arc` -> `rust.ownership(E, owned)` for Box, `shared` for Rc/Arc; a plain value -> `rust.ownership(E, owned)` |
| `ty.type_arguments()` non-empty | `tsi.called(T, ctorT, L)`, `tsi.argument(L, i, argT)` |
| `ty.as_builtin()` | `tsi.primitive(T, <builtin name>)`: `i32`, `u32`, `f64`, `bool`, `char`, `str`, ... as the atom |
| `ty.as_callable(db)` | `tsi.callable` plus input/output for closure and fn-pointer field types |

Every id named by an edge, argument, input, output or assoc row gets a `tsi.type` row in a closing pass, including std types reached as leaves (`Vec`, `String`, `Option`), each with `tsi.origin(T, rust, span)` when `nav_of` (`rust_checker_ra.rs:110`) finds one, else `tsi.origin` with the crate display name as text.

Coverage claims: `complete` for `tsi.type, tsi.denotes, tsi.origin, tsi.product, tsi.sum, tsi.callable, tsi.primitive, tsi.parameter, tsi.called, tsi.argument, tsi.input, tsi.output, rust.trait, rust.impl, rust.assoc, rust.lifetime, rust.ownership`. `partial` with a diagnostic for `tsi.edge` ("enumerated for workspace-declared owners; std and dependency types are leaves"), `tsi.conforms` ("declared impls only; blanket and auto traits not enumerated"), `tsi.has_type` ("occurrences not walked in this arc"), `tsi.subtype`, `tsi.assignable`, `tsi.equivalent` ("not enumerated").

## Tests, `tests/102_rust_semantic_tsi.rs`

`#![cfg(feature = "rust-checker")]`, driven like `tests/94_rust_checker_types.rs`. Command: `extract --witness --resolve --family type --project-root tests/fixtures/tsi/rust_probe --rust-checker tests/fixtures/tsi/rust_probe/src/lib.rs`.

| case | expected |
|---|---|
| two runs | `run mode=semantic tool=rust-analyzer`; every `checker_walk` witness names it |
| struct | `User`: `tsi.product`, edges `id` pos 0 target `T` (a `tsi.parameter` of User), `name` pos 1 whose target has `tsi.called(_, Option, L)` and `tsi.argument(L, 0, String)` |
| trait and assoc | `rust.trait(Mapper)`, `rust.assoc(Mapper, Output, _)`; the impl: `rust.impl(_, User, Mapper)`, `tsi.conforms(User, Mapper, declared)`, `rust.assoc(User, Output, V)` with `tsi.called(V, Vec, L)` and `tsi.argument(L, 0, T)` |
| lifetime and ownership | `View`: `rust.lifetime(View, a)`; `text` edge `rust.ownership(_, shared)`; `owned` edge `rust.ownership(_, owned)`; `shared` edge `rust.ownership(_, shared)` and its target id equals `View`'s own id (recursion) |
| enum | `Shape`: `tsi.sum`, two variant edges, `Square`'s target is a `tsi.product` with a `side` edge to `tsi.primitive(_, f64)` |
| callable | `map`: `tsi.callable`, `tsi.input(map, 0, T)`, `tsi.output(map, 0, <Self::Output id>)` |
| coverage | `complete` for exactly the set above; a `diagnostic` row beside every `partial` |
| every id declared | `extract --ingest` on the stream returns rc=0 |
| checker-off unchanged | `94_rust_checker_types` and `93_rust_checker_wiring` pass unchanged |

Header carries a SABOTAGE RECEIPT: on the base sha the rust-analyzer run row exists with zero `checker_walk` witnesses.

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli,rust-checker --test 102_rust_semantic_tsi --test 94_rust_checker_types --test 93_rust_checker_wiring 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli --test golden_parity --test 1_resolve_cli 2>&1 | tail -3
```

`rust-checker` is a 380s cold build (`Cargo.toml:217`); build once in the background, never foreground-wait on it. Failure-mode 105 (`docs/failure-modes.md`) records a +970 MB RSS regression on this tier: measure `/usr/bin/time -l` on the gate's second command and paste the max RSS in the PR.

## Style laws

- No `eprintln!`; `tracing` only.
- Comments: constraints only. No dates, no arc names.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- No per-item allocation of display strings for ids beyond the one `HashMap` key; `ModuleDef` keys are `Copy`.

## Done

PR titled `extract: rust semantic mode, the rust-analyzer walk emits TSI rows (TSI A6)`.
`git diff --stat <base>...HEAD` lists only the files above.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "A6 PR #<n>: 102_rust_semantic_tsi N tests, max RSS <n> MB, ingest rc=0"`.
