# brief: dl7 side of TSI, takeover from codex-tsi

Lane: `feat/dl7-tsi-render`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`, then `git merge origin/feature/dl7-source-intelligence` (codex-tsi's pushed work, tip `f24ece60e`, PR #689 open, Chris merges it himself). A conflict on either = STOP AND REPORT. You continue from that tip; your PR will carry its commits, that is expected.
Paths relative to the repo root.

## Why

codex-tsi (a codex session) built the dl7 consumer of the TSI wire: loader `v7/src/2_comptime/0c_extract_loader.pl` (accepts `tsi.name`), emitter `v7/src/3_emit/2_rust_type_emitter.pl` (prints written Rust names from `tsi.name`, owner-qualifies collisions as `User_T`, maps `()` to `unit`), tests `v7/test/4_extract_loader.test.pl`, `v7/test/6_rust_type_emitter.test.pl`, E2E `v7/test/7_rust_type_region.e2e.pl`. Chris stopped codex for limits; you take the rest. Gate: `cd v7 && just build` (`v7/README.md:26`); the v7 battery was 57/57 at PR #671 and 12/12 on codex's loader+emitter slice, re-measure yourself.

The extract side is NOT yours: `v6/sprefa-extract/**` has two live lanes (`fix/ts-syntax-type-graph`, `feat/go-syntax-type-graph`). The wire you consume: `v6/sprefa-extract/src/tsi/registry.rs` (every relation and arity), `issues/extract-semantic-fact-roundtrip/item.md` `## Decisions` (contract, and the 2026-09-03 `tsi.name` entry). Emit rows with `v6/sprefa-extract/target/debug/extract --witness --family type <file>` (build with `cargo build --features cli` in that crate, or `--features cli,ts-checker,rust-checker` for the checker tiers, ~7 min cold); fixtures `v6/sprefa-extract/tests/fixtures/tsi/probe_graph.rs`, `probe.ts`, `probe.rs`, `rust_probe/`.

## User decisions (Chris, 2026-09-03, in this order)

| # | decision | today | what you build |
|---|---|---|---|
| U1 | Rust `()` renders as lispy nil `()`, "not unit, () is funnier" | reader reads a bare `()` form (`v7/test/fixtures/0_minimal.dl7:9`), but as a type target `(: X (* (: value ())))` the lowerer answers `unresolved_expression_form`; the emitter substitutes `unit` (`2_rust_type_emitter.pl:72` `source_name_identifier("()", ok(unit))`) | give the empty form a type meaning in the lowerer: `()` is the empty product (the same node the prelude's `(: unit (* ))` declares today, `v7/prelude/5_tsi_primitives.dl7`, so `tsi.primitive(Id, unit)` binds to it). The emitter prints `()`. Find the throw site (`grep -rn unresolved_expression_form v7/src`), cite it in the commit body. Every prelude spelling that says `unit` for the empty tuple becomes `()`; other languages' `void`/`undefined` stay their own classes. |
| U2 | qualified names are dotted, `User.T` and `Shape.Circle`, "i thought dot was edge target follow"; dotted paths are the decided modscope shape (`plans/2026-08-03-modscope-plan.md:90,131,172`; `v6/prolog/conformance/rulings.pl:614` decision 8 "dotted heads contribute") | dl7 atom rule `[A-Za-z_][A-Za-z0-9_-]*` (`v7/src/0_reader/README.md:47`, `v7/src/0_reader/0_parser.pl:303-309`), no `.`; emitter joins with `_` (`2_rust_type_emitter.pl:87-100` `derived_type_name`) | extend the reader: `.` admitted INSIDE an atom (never first or last, never doubled), `node(_, atom('User.T'))` with the dotted text preserved; the lowerer treats a dotted atom as a qualified name whose segments are the owner path (modscope decision 8); the emitter derives `User.T`, `Shape.Circle`; the derived impl name becomes `User.Mapper.impl`, the anonymous product `anonymous_product_<n>` stays. `README.md:47` and the reader tests (`v7/test/0_reader.test.pl`) change with it. If any existing dl7 program or fixture already uses `.` for something else, STOP AND REPORT with the file:line before changing the reader. |
| U3 | "use another fable to take over what codex was doing": generalize the emitter past rust | `2_rust_type_emitter.pl` renders only a rust stream; ts and go streams (ts has `tsi.name` today, go lands in the live lane) have no renderer | one type emitter over the shared `tsi.*` projection (`tsi.type/name/origin/product/sum/callable/edge/parameter/called/argument/input/output/primitive/has_type/symbol/denotes`), with per-language native rows (`ts.*`, `rust.*`, `go.*`) rendered as metadata the way `write_impl_metadata` does `rust.impl` (`:320`). Rename the file if the name lies; keep `6_rust_type_emitter.test.pl` cases green as the rust half and add a ts half over `probe.ts`. The language atom comes from `tsi.origin` arg 2 and from the `run` record's language (`v6/sprefa-extract/src/tsi/types.rs` `RunRow`). |
| U4 | Go primitives | the prelude has a TypeScript block and a Rust block, no Go block | add `; Go builtins.` to `v7/prelude/5_tsi_primitives.dl7`: bool string int int8 int16 int32 int64 uint uint8 uint16 uint32 uint64 uintptr float32 float64 complex64 complex128 byte rune, each `(: <name> (* ))`, the same shape as the rust block. `error` and `any` are not primitives. |

## Receipts

1. `cd v7 && just build` green, count pasted, three runs (measure three times, never once).
2. `v7/test/0_reader.test.pl`: new cases for a dotted atom, a leading/trailing/doubled dot rejected with a named diagnostic, and the bare `()` form as a type target lowering without diagnostic.
3. `v7/test/6_rust_type_emitter.test.pl` (or its renamed twin): the rust probe renders `User`, `User.T`, `Shape.Circle`, `User.Mapper.impl`, `()`; a ts case over `probe.ts` renders `User`, `Mapper`, `User.T`, `map`. No `_`-joined derived name, no `unit`, no `rust_type_N`, no `rust__` anywhere in the output: `grep -c` receipts in the PR body.
4. `v7/test/7_rust_type_region.e2e.pl` green, the real Extract-to-DL7-to-Soopy stream, the exact command line pasted (codex-tsi's E2E; ask via `boop beep --no-wait --as feat-dl7-tsi-render codex-tsi "<question>"` only if the test file does not show it).
5. `swipl -g go -t halt v6/prolog/ARCH.pl` unchanged (you do not edit ARCH; the coordinator adds your row).
6. `git diff --stat origin/main...HEAD` lists only `v7/**` files plus codex's already-pushed commits.

## Ownership

Owned: `v7/**`.
Forbidden: `v6/**` (two extract lanes are live; the extract binary is read-only to you), `plans/**`, `docs/**`, `issues/**`, `v6/prolog/ARCH.pl`, the main tree at `/Users/chrishafley/projects/sprefa` (it is checked out on `feature/dl7-source-intelligence` with a dirty file; never touch it, your worktree is your world).

## Style laws

No em dashes. Comments state only constraints the code cannot show; this brief is not to be copied into comments. Descriptive names, never single letters. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Language vocabulary: rxjs, prolog, SQL words only; "support" is banned. Language design beyond U1-U4 is Chris's: a fork you find goes back as a cited question, not a change. One commit per U row, subjects `dl7: <what in five words>`.

## Done

Push, PR against `main` with receipts 1-6 pasted, then:
`boop beep --no-wait --as feat-dl7-tsi-render sprefa-coordinator "dl7-render PR #<n>: just build <n>/<n> x3, reader <n>/<n>, emitter rust+ts <n>/<n>, e2e green, forks: <list or none>"`.
