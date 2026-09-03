# brief: the go syntax tier's best-guess type graph (first tsi rows for go)

Lane: `feat/go-syntax-type-graph`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Paths relative to it unless they start with `v6/` or `v7/`.
ARCH row: `go_syntax_type_graph` (`v6/prolog/ARCH.pl`, grep it).

## Why

Rust (PR #678) and ts (`src/lang/ts.rs:1137`) syntax tiers emit a TSI type graph under `--witness --family type`. Go emits none: measured at `555869ed7`, `extract --witness --family type tests/fixtures/go_modules/sample.go` has zero `record=fact` rows (`grep -c '"tsi\.' src/lang/go.rs` is 0). The go type family today is v5's port: entities + edge candidates + arrow sigs (`src/lang/go.rs:121` `project_types`, `:135` `walk_go_entities`, `:336` `go_edge_candidates`, `:558` `go_type_refs`). This lane adds the TSI rows beside them, in a new file, the way rust keeps them in `rust_type_edges.rs`.

Read first, in this order: `src/lang/rust_type_edges.rs:290` `tsi_rows` to the end of the file (the shape to mirror, D1-D7 in `plans/2026-09-03-rust-syntax-type-graph.BRIEF.md`), `src/types.rs:425` `TsiNames`, `src/tsi/registry.rs` (the `go.*` rows `go.interface`, `go.type_set`, `go.embedding` already exist), `issues/extract-semantic-fact-roundtrip/item.md` `## Decisions`.

## The rows

New file `src/lang/go_type_edges.rs`, `pub(crate) fn tsi_rows(root: tree_sitter::Node, src: &[u8], strings: &mut Strings, sink: &mut FamilyBundle<TypeF>)`, called from `go.rs` `project_types` (one added line, the only `go.rs` edit) inside a `trace::phase_span("go", trace::Phase::TsiSyntax)` like `rust_type_edges.rs:296`. `TsiNames::new("go")`; `tsi.origin` carries the atom `go`.

| go form | rows |
|---|---|
| `type S struct { A T; B *U; V }` | `tsi.type`+`tsi.origin`+`tsi.name` (via `named`) on `S`, `tsi.product(S)`, one `tsi.edge(S, "A", id(T), 0)` per named field; an embedded field `V` is `tsi.edge(S, "V", id(V), n)` plus `go.embedding(S, id(V))`. A pointer `*U` strips to `U` (the rust `strip_type` twin); `[]T`, `map[K]V`, `chan T`, `func(...)`, `[n]T` keep one id per written text with one comment naming them. |
| `type I interface { M(x T) U; Other }` | `tsi.type`+origin+name, `go.interface(I)`, `tsi.product(I)` (the ts interface projection, `tests/100_tsi_intersection.rs` `SHARED` row `("tsi.product", ["User"])` is the precedent), one `tsi.callable` per method with `tsi.edge(I, "M", callable, position)` and `tsi.input`/`tsi.output` per written param/result type; an embedded interface `Other` is `go.embedding(I, id(Other))`. |
| `type C interface { ~int \| string }` | `go.type_set(C, id(int))`, `go.type_set(C, id(string))`; the `~` is dropped, one comment. |
| `type L[T any, K comparable] struct` and `func F[T any]` | `tsi.parameter(param, owner, position, unspecified)` per type parameter, `tsi.edge(param, "bound", id(constraint), rank)` unless the constraint is `any`; the param's `tsi.name` is its written name (`names.anonymous` + `names.name`, the ts twin `ts.rs:1226`). Scope lookup: a param in scope wins over the file text table (rule 4), the rust `TsiScope` twin. |
| `L[int]` written anywhere a type is written | `tsi.called(result, callee, list)` + `tsi.argument(list, position, arg)`, once per distinct written text; `result` = id of the full text, `callee` = id of `L`. |
| `func (r *S) M(a A) (B, error)` | `tsi.callable` (name `M`), `tsi.edge(S, "M", callable, position among S's methods in this file)`, `tsi.input` per param (receiver skipped, grouped params `a, b int` expanded, `go.rs:496` `fn_sigs` shows how), `tsi.output` per result, position 0..n. |
| `func F(...)` | `tsi.callable` with name, no owner edge. |
| `type A = B` | `tsi.symbol(A)` + `tsi.name(A, "A")` + `tsi.denotes(A, id(B))`, the ts alias shape (ARCH `tsi_a4_syntax_rows`: alias = symbol + denotes). `type A B` (defined type, not alias) is a `tsi.type` of its own with `tsi.edge(A, "underlying", id(B), 0)`. |
| `const X T = ...`, `var Y U` | `tsi.has_type(span of the ident, id(T))`; an untyped const emits nothing. |
| builtins `bool string int int8 int16 int32 int64 uint uint8 uint16 uint32 uint64 uintptr float32 float64 complex64 complex128 byte rune` | `tsi.type` + `tsi.primitive(id, <name>)` + `tsi.name(id, <name>)`, NO `tsi.origin`, once per class. `error` and `any` are NOT primitives (an interface and an alias in the universe scope): they mint a named id with origin at the first reference span, as any undeclared name does. The v7 prelude (`v7/prelude/5_tsi_primitives.dl7`) has no Go block at the base sha: the v7 load will report `tsi_primitive_class_absent(<class>)` per class. Report the list, do not edit the prelude (not owned; codex-tsi adds the block). |

Every declared type keeps `tsi.origin(id, go, span of its name)`; a referenced-but-undeclared name (`fmt.Stringer`, `error`) keeps origin at its first reference span. The written text of a qualified name is the id key (`fmt.Stringer`), the origin span is the last segment (rust D1). Spans are the crate's `Span` from `go_node_span` (`go.rs:221`), byte offsets, the same unit `go_edge_candidates` uses.

## Receipts

1. New fixture `tests/fixtures/tsi/probe_graph.go`, under 70 lines, one package, holding every row in the table: a generic struct with a pointer field, a slice field, a map field and an embedded struct; an interface with a method and an embedded interface; a type-set constraint; a method with a pointer receiver and `(T, error)` results; a free generic func; an alias and a defined type; a typed const, an untyped const, a typed var; `bool`, `string`, `int64`, `byte` in signatures.
2. New `tests/112_go_syntax_graph.rs`, SABOTAGE RECEIPT header stating the base sha and "zero tsi rows for any `.go` input". One `#[test]` per table row, named for the form, asserting on parsed `FactOut` rows with span text resolved against the fixture bytes (copy helpers from `tests/106_rust_syntax_graph.rs:36-160`, do not import), plus one test that every `tsi.type` id carries `tsi.name` or is anonymous (copy `tests/110_tsi_name.rs` `every_named_id_spells_its_origin`). Assert exact small sets, never `len() > 0`.
3. `cargo test --features cli --test 112_go_syntax_graph --test 96_witness_wire --test 97_ingest --test 99_syntax_tsi_rows --test 110_tsi_name` green. `99`'s `flag_off_emits_no_fact_row` walks fixtures; add the go probe to its list if it takes a list.
4. `extract --witness --family type tests/fixtures/tsi/probe_graph.go | extract --ingest` rc=0.
5. v7 load of the same stream the way `v7/test/4_extract_loader.test.pl:38` `install_streams` does it, pasting node count, edge count and the `Diagnostics` list (the `tsi_primitive_class_absent` rows are expected; list them).
6. `cargo test --features cli --test golden_parity` green and `git diff --stat origin/main...HEAD` lists no golden: the flag-off stream is byte-identical. Every go test in `tests/` that runs today still passes (`ls tests | grep -i go`).
7. Before-and-after relation counts on `tests/fixtures/go_modules/sample.go` in the PR body.
8. Full battery `cargo test --features cli` in the background (10-second law), `tail -30` pasted.

## Ownership

Owned: `src/lang/go_type_edges.rs` (new), `src/lang/go.rs` (the one call line in `project_types` and a `mod`/`use` line, nothing else), `src/lang/mod.rs` (the `mod go_type_edges;` line only if `go.rs` cannot declare it), `tests/fixtures/tsi/probe_graph.go`, `tests/112_go_syntax_graph.rs`, `tests/99_syntax_tsi_rows.rs` (fixture list only).
Forbidden: `src/lang/ts.rs` (a ts lane runs beside you), `src/lang/rust*.rs`, `src/lang/go_modules.rs`, `src/types.rs`, `src/tsi/**`, `src/project.rs`, `src/wire.rs`, `src/dispatch.rs`, `tests/100_tsi_intersection.rs`, `tests/110_tsi_name.rs`, `v7/**`, `docs/**`, `v6/prolog/ARCH.pl`.

## Style laws

No em dashes. Comments state only constraints the code cannot show; this table is not to be copied into comments. `tracing` only. Descriptive names, never single letters. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Language vocabulary: rxjs, prolog, SQL words only; "support" is banned. One commit per table row group (declarations, members and methods, generics and calls, primitives and has_type), subjects `extract: go tsi <group in five words>`.

## Done

Push, PR against `main` with receipts 3-8 pasted, then:
`boop beep --no-wait --as feat-go-syntax-type-graph sprefa-coordinator "go-graph PR #<n>: 112 <n>/<n>, 99 <n>/<n>, ingest rc=0, v7 nodes=<n> edges=<n> absent=<classes>, battery <pass>/<total>"`.
