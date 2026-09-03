# brief: the rust syntax tier's best-guess type graph

Lane: `fix/rust-syntax-type-graph`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Paths relative to it unless they start with `v6/` or `v7/`.

## Why

User 2026-09-03: import a rust file, do the best guess at its type graph, emit it as a TSI stream (the dl7 side is not this lane). The rust-analyzer tier is off unless the binary carries `--features rust-checker` and the file sits in a loadable workspace, so the syntax tier is what a hand import gets. Measured on `src/trail.rs` with `extract --witness --family type` at `39a5211a1`: 38 `tsi.type`, zero `tsi.called`, zero `tsi.primitive`, zero `tsi.has_type`, no method reachable from its owner, `std::fmt::Result` spanning the token `std`. The rows come from `src/lang/rust_type_edges.rs:275` `tsi_rows` and the fns under it. The ts twin is `src/lang/ts.rs:1197-1580` (`tsi_target`, `tsi_params`, `tsi_class`, `tsi_interface`, `tsi_alias`); read it first, the two doors stay one shape.

Contract: `issues/extract-semantic-fact-roundtrip/item.md` `## Decisions` (relations, identity rules 1-5, mode contract). Registry: `src/tsi/registry.rs:51`. The sink: `src/tsi/sink.rs`; `TsiNames` (`named` keys an id on written text, `anonymous`, `bare_id`, `edge`, `fact`) in `src/tsi/mod.rs` or wherever `grep -n "struct TsiNames"` lands.

## The seven defects, in commit order

| # | today (`rust_type_edges.rs`) | best guess |
|---|---|---|
| D1 | `tsi_type_span` (`:604`) spans `segments.first()`: `std::fmt::Result` reads `std` | span the LAST segment's ident; the id key stays the full written path text (`std::fmt::Result` and `Result` are distinct ids at this tier; the semantic tier unifies them, rule 1) |
| D2 | `tsi_called` (`:548`) fires only from `Item::Type` alias bodies (`:372`); a field, input, output, variant payload or bound written `Name<Args>` mints one bare id and nothing inside it | every written `Name<Args>` anywhere `tsi_type_id` is reached emits `tsi.called(result, callee, list)` + one `tsi.argument(list, position, arg)` per type argument, ONCE per distinct written text (first visit; a `BTreeSet<u32>` of ids already called). `result` = the id `tsi_type_id` returns for the full text (so two `Result<Trail, TrailError>` share it, rule 3 approximated by text). `callee` = the id of the path text without arguments, spanned per D1. Recurse into arguments (`Vec<Option<T>>` is two calls). Lifetime and const arguments are skipped as today. `Type::Tuple` becomes an anonymous id + `tsi.product` + edges labelled `0..n` (rule 2); the unit tuple `()` is the primitive `unit` (D6). Arrays, slices, bare fns, `impl Trait`, `dyn Trait` keep today's one-id-per-text shape; say so in one comment on `tsi_type_id`. |
| D3 | enum variants (`:339`) are named `Enum::Variant` ids with no shape | a tuple variant gets `tsi.product(variant)` + edges labelled `0..n` to its payload types; a struct variant the same with field labels; a unit variant stays bare. Payload types go through `tsi_type_id` (so D2 applies). |
| D4 | `tsi_callable` (`:509`) mints a callable no owner reaches | mirror `ts.rs` `tsi_class` member edges (`ts.rs:1363`, `:1386`): `impl Type { fn m }` and `impl Trait for Type { fn m }` emit `tsi.edge(self_type, "m", callable, position)` with `position` the method's index among the block's `ImplItem::Fn`s; a trait declaration's methods emit the same edge from the trait id. Free fns stay ownerless. The callable id also gets `tsi.origin` (today it has one from `anonymous`; keep). |
| D5 | `Item::Const` and `Item::Static` fall to `_ => {}` (`:383`) | `tsi.has_type(span of the ident, type_id of the written type)`. Registry row `tsi.has_type` `[Span, Id]`. Nothing else. |
| D6 | `u64`, `bool`, `str`, `String` all mint `tsi.type` + `tsi.origin` with the reference span as "declaration range" | the 17 builtins the v7 prelude declares (`v7/prelude/5_tsi_primitives.dl7`: i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 bool char str usize isize) plus `unit` for `()` emit `tsi.type` + `tsi.primitive(id, <name>)` and NO `tsi.origin`. The atom is the builtin's own name, the same atom the semantic tier writes (`src/lang/rust_checker_ra.rs:715` `builtin.name()`). A type parameter in scope named like a builtin wins (scope lookup first, as today). `String` is not a builtin. |
| D7 | `FnArg::Receiver` (`&self`) is skipped (`:530`) | stays skipped; `rust.ownership` is the semantic tier's. One comment, no code. |

Every declared type keeps `tsi.origin(id, rust, decl span)`; a referenced-but-undeclared type (`Connection`, `Path`) keeps origin on its first reference span, as today. That is the accepted best guess for a single-file import; the resolve leg's intersection (`tests/100_tsi_intersection.rs`) is where cross-file identity lands, not here.

## Receipts

1. New fixture `tests/fixtures/tsi/probe_graph.rs`, under 60 lines, holding every shape above: an enum with a unit, a tuple and a struct variant; a struct with `Vec<Option<T>>`, `Result<u64, Error>`, a tuple field `(String, u32)`, a `std::fmt::Result` path; a `const` and a `static`; an inherent impl with two methods; a trait with one method and its impl for the struct; `bool`, `str`, `()` in signatures.
2. New `tests/106_rust_syntax_graph.rs`, SABOTAGE RECEIPT header stating the base sha and, per defect, the row the base sha lacks (D1: origin span text `std`; D2: zero `tsi.called` outside the alias; D3: no `tsi.product` on a variant; D4: no edge from the struct to its method; D5: zero `tsi.has_type`; D6: zero `tsi.primitive`). One `#[test]` per defect, named for the defect, asserting on the parsed `FactOut` rows with span text resolved against the fixture bytes (the helpers in `tests/99_syntax_tsi_rows.rs:152-188` are the shape; copy, do not import). Assert exact small sets, never `len() > 0`.
3. `cargo test --features cli --test 99_syntax_tsi_rows --test 96_witness_wire --test 97_ingest --test 100_tsi_intersection --test 106_rust_syntax_graph` green. `99`'s `rust_struct_carries_fields_and_parameter` (`:331`) may gain rows from D2 on `probe.rs` `name: Option<String>`; adjust its assertion and say why in the PR body.
4. `extract --witness --family type tests/fixtures/tsi/probe_graph.rs | extract --ingest` rc=0 (the round trip; `tests/97_ingest.rs` shows the invocation).
5. v7 load of the same stream: from the repo root, `swipl -q -g "..."` the way `v7/test/4_extract_loader.test.pl:38` `install_streams` does it (`load_tsi_stream/3` then `install_tsi_graph/6` with the test's `prelude_stub`), pasting node count, edge count and the `Diagnostics` list. `tsi_primitive_class_absent(unit)` will appear if the prelude lacks `unit`: report it, do not edit the prelude (not owned).
6. `cargo test --features cli --test golden_parity` green and `git diff --stat origin/main...HEAD` lists no golden: the flag-off stream is byte-identical.
7. Before-and-after relation counts on `src/trail.rs` (`extract --witness --family type src/trail.rs`, count `record=fact` by `relation`) in the PR body.
8. Full battery `cargo test --features cli` in the background (10-second law), `tail -30` pasted.

## Ownership

Owned: `src/lang/rust_type_edges.rs`, `tests/fixtures/tsi/probe_graph.rs`, `tests/106_rust_syntax_graph.rs`, `tests/99_syntax_tsi_rows.rs` (assertion moves only).
Forbidden: `src/lang/ts.rs` (ts parity for D2/D5/D6 is a follow-up row, not this lane), `src/lang/rust.rs`, `src/tsi/**`, `src/project.rs`, `src/wire.rs`, `src/dispatch.rs`, `tests/31_tracing.rs`, `tests/98_resolve_witness.rs`, `v7/**`, `docs/**`, `v6/prolog/ARCH.pl`. Other lanes own them.

## Style laws

No em dashes. Comments state only constraints the code cannot show; the table above is not to be copied into comments. `tracing` only. Descriptive names, never single letters. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Language vocabulary: rxjs, prolog, SQL words only; "support" is banned. Seven commits, one per defect, subjects `extract: rust tsi <defect in five words>`.

## Done

Push, PR against `main` with receipts 3-8 pasted, then:
`boop beep --no-wait --as fix-rust-syntax-type-graph sprefa-coordinator "rust-graph PR #<n>: 106 <n>/<n>, 99 <n>/<n>, ingest rc=0, v7 nodes=<n> edges=<n> diags=<list>, battery <pass>/<total>"`.
