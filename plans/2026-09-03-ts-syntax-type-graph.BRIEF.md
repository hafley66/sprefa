# brief: the ts syntax tier's best-guess type graph (parity with rust)

Lane: `fix/ts-syntax-type-graph`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Paths relative to it unless they start with `v6/` or `v7/`.
ARCH row: `ts_syntax_type_graph_parity` (`v6/prolog/ARCH.pl`, grep it).

## Why

The rust syntax tier landed its best-guess graph (PR #678, `src/lang/rust_type_edges.rs:290` `tsi_rows` and below; brief `plans/2026-09-03-rust-syntax-type-graph.BRIEF.md`, defects D1-D7). The ts syntax tier (`src/lang/ts.rs:1137` `tsi_rows` and below) still has the pre-#678 shape. Measured at `555869ed7` with `extract --witness --family type tests/fixtures/tsi/probe.ts`: 11 `tsi.type`, 1 `tsi.called` (the alias body only), zero `tsi.primitive`, zero `tsi.has_type`. The two doors stay one shape; read `rust_type_edges.rs` first and mirror it.

Contract: `issues/extract-semantic-fact-roundtrip/item.md` `## Decisions`. Registry: `src/tsi/registry.rs`. `TsiNames` (`named`, `anonymous`, `bare_id`, `edge`, `fact`, `name`, `origin`) is `src/types.rs:425`. `tsi.name(Id, Text)` landed in PR #688: every id `named` mints carries it; an id minted through `anonymous` or `bare_id` needs an explicit `names.name(id, text)` when it has a written name (see `ts.rs:1226`, `:1318`).

## The defects, in commit order

| # | today (`ts.rs`) | best guess (the rust twin) |
|---|---|---|
| T1 | `tsi_target` (`:1197`) mints one id per written text: `Partial<User<number>>` is one bare id; `tsi.called` fires only from an alias body (`tsi_alias` `:1500-1516`) | a `tsi_type_id(ty: &TSType, ...)` recursion, the twin of `rust_type_edges.rs` `tsi_type_id`: `TSTypeReference` with `type_arguments` emits `tsi.called(result, callee, list)` + `tsi.argument(list, position, arg)` once per distinct written text (first visit set), `result` = the id of the full text, `callee` = the id of `type_name` alone; recurse into arguments. `TSUnionType` outside an alias = anonymous id + `tsi.sum` + edges labelled by member text; `TSTupleType` = anonymous id + `tsi.product` + edges `0..n`; `TSTypeLiteral` = anonymous id + `tsi.product` + one edge per property signature (key text, position), `ts.optional`/`ts.readonly` on the edge as `tsi_class` does; `TSFunctionType` = anonymous id + `tsi.callable` + `tsi.input`/`tsi.output` through `tsi_signature`. `TSArrayType` (`T[]`), indexed access, conditional, mapped, template literal keep one id per text; one comment on `tsi_type_id` names them. Every site that calls `tsi_target` on a type node (`tsi_class` fields, `tsi_interface` members, `tsi_signature` inputs/outputs, `tsi_params` bounds, `tsi_alias`, `tsi_enum`) goes through `tsi_type_id`. Scope lookup (type parameter wins) stays first, as today. |
| T2 | `tsi_var_fn` (`:1559`) skips a declarator whose init is not a function | a declarator whose `id` carries a `type_annotation` emits `tsi.has_type(span of the ident, tsi_type_id(annotation))`; function-valued declarators keep today's callable AND gain the has_type row when annotated. Registry row `tsi.has_type` `[Span, Id]`. |
| T3 | `string`, `number`, `boolean` mint `tsi.type` + `tsi.origin` at the reference span | the classes the v7 prelude declares for ts (`v7/prelude/5_tsi_primitives.dl7` `; TypeScript classes.` block: string number boolean bigint symbol void undefined null never unknown, and whatever else that block lists at the base sha; `any` and `object` only if the block has them, else report) emit `tsi.type` + `tsi.primitive(id, <keyword>)` + `tsi.name(id, <keyword>)` and NO `tsi.origin`, once per class (a `classes` map on a `TsiState`, the rust twin `rust_type_edges.rs:703` `tsi_primitive_id`). `TSStringKeyword` and friends are the oxc nodes. A scope type parameter named like a keyword cannot occur (keywords are reserved), no guard needed. |
| T4 | `tsi_interface` (`:1418`) call signatures and method signatures, `tsi_class` accessors | whatever the rust twin has no analogue for stays as it is; one comment each, no code. List them in the PR body. |

Identity rule reminder: two identical written texts in one file are one id (rule 3 by text); a type parameter in scope is its own id (rule 4). `tsi.origin` on a referenced-but-undeclared name (`Partial`, `Array`) stays the first reference span.

## Receipts

1. New fixture `tests/fixtures/tsi/probe_graph.ts`, under 60 lines: a class with fields `Map<string, Option<T>>`, `[string, number]`, `{ reason: string; code?: number }`, `(element: T) => U`; an interface with an optional and a readonly member; a union alias and an alias `Partial<User<number>>`; `const LIMIT: number = 3`, `let banner: string`; a function with `boolean`, `void`, `string[]` in its signature.
2. New `tests/111_ts_syntax_graph.rs`, SABOTAGE RECEIPT header stating the base sha and, per defect, the row the base sha lacks (T1: one `tsi.called`; T2: zero `tsi.has_type`; T3: zero `tsi.primitive`). One `#[test]` per defect, plus one asserting every `tsi.type` id carries `tsi.name` or is a tuple/literal/function anonymous id (copy the shape of `tests/110_tsi_name.rs` `every_named_id_spells_its_origin`). Assert exact small sets, never `len() > 0`. Copy helpers from `tests/106_rust_syntax_graph.rs:36-160`, do not import.
3. `cargo test --features cli --test 99_syntax_tsi_rows --test 96_witness_wire --test 97_ingest --test 105_resolve_syntax_tsi --test 110_tsi_name --test 111_ts_syntax_graph` green. `99` and `110` assertions may move (T1 adds rows to `probe.ts`); say which and why in the PR body.
4. `cargo test --features cli,ts-checker,rust-checker --test 100_tsi_intersection --test 101_ts_semantic_tsi` green: the semantic ts tier is untouched, so 101 is a guard; 100's `SHARED`/`TS_ASYMMETRIC` lists pin the SEMANTIC streams and must not need edits. If they do, STOP AND REPORT: the syntax tier leaked into `--resolve`. The build needs `node` + `typescript` on PATH the way `tests/101_ts_semantic_tsi.rs:20-60` finds them; ~7 minutes cold.
5. `extract --witness --family type tests/fixtures/tsi/probe_graph.ts | extract --ingest` rc=0.
6. `cargo test --features cli --test golden_parity` green and `git diff --stat origin/main...HEAD` lists no golden.
7. Before-and-after relation counts on `tests/fixtures/tsi/probe.ts` and on `src/lang/ts_checker.mjs`'s nearest ts sibling, `v6/tsv2/src/*.ts` (pick one file, name it) in the PR body.
8. Full battery `cargo test --features cli` in the background (10-second law), `tail -30` pasted.

## Ownership

Owned: `src/lang/ts.rs` (the `tsi_*` fns only, nothing above `:1130`), `tests/fixtures/tsi/probe_graph.ts`, `tests/111_ts_syntax_graph.rs`, `tests/99_syntax_tsi_rows.rs` and `tests/110_tsi_name.rs` (assertion moves only).
Forbidden: `src/lang/rust_type_edges.rs`, `src/lang/go*.rs` (a go lane runs beside you), `src/lang/ts_checker.mjs`, `src/types.rs`, `src/tsi/**`, `src/project.rs`, `src/wire.rs`, `src/dispatch.rs`, `tests/100_tsi_intersection.rs`, `tests/101_ts_semantic_tsi.rs`, `v7/**`, `docs/**`, `v6/prolog/ARCH.pl`.

## Style laws

No em dashes. Comments state only constraints the code cannot show; this table is not to be copied into comments. `tracing` only. Descriptive names, never single letters. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Language vocabulary: rxjs, prolog, SQL words only; "support" is banned. One commit per defect, subjects `extract: ts tsi <defect in five words>`.

## Done

Push, PR against `main` with receipts 3-8 pasted, then:
`boop beep --no-wait --as fix-ts-syntax-type-graph sprefa-coordinator "ts-graph PR #<n>: 111 <n>/<n>, 99 <n>/<n>, 110 <n>/<n>, 100+101 green, ingest rc=0, battery <pass>/<total>"`.
