# Brief: the go type plane (lane `fix-extract-go-type-plane`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law)
and `plans/extract-crawl-2026-08-29/go.REPORT.md` (Kinks + Fixes tables).
User decision (2026-08-29): go reaches 100% resolve coverage FIRST, its spec is
the smallest. The package plane landed (PR #546, `tests/51_go_package_resolve.rs`,
reachability 2.8% -> 22.6%). This lane builds the three remaining legs.

## First action
```
git merge --ff-only 7712a40b83a726981a736ef5dda424cea4bf49e3
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-go-type-plane sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree. Never a
globally installed `extract`; `which extract` output is not your binary.

## Ownership
Yours: `src/lang/go.rs` (split a `src/lang/go_types.rs` if go.rs passes
2,600 lines), `tests/55_go_type_plane.rs`, `tests/fixtures/go_findings/**`,
`plans/extract-crawl-2026-08-29/go.REPORT.md` (append only).
Forbidden: `src/project.rs`, `src/types.rs`, `src/schema.rs`, `src/lang/ts*.rs`,
`src/lang/rust.rs` (another lane owns them right now). If a leg needs a new
`IndexBag` slot, write the slot as a go-local `OnceLock` inside go.rs keyed
off `cx.indexes.paths` and say so in the PR body; the coordinator lifts it.

## What exists (read before writing)
- `Resolve<CallF> for GoSource` (`go.rs:2112`): name-match leg + scip leg,
  import-qualified sites go through `go_package_dir` / `call_name_match_in_package`.
- `go_receiver_type` (`go.rs:600`) reads a method's receiver type name.
- `go_walk_call_defs` (`go.rs:796`) mints one def per func/method/lambda.
  Interface method specs (`method_spec` under `interface_type`) mint NOTHING,
  so `c.w.Write()` in `tests/fixtures/go_findings/corpus_interface_dispatch.go`
  has no target.
- `Resolve<TypeF>` (`go.rs:1875`) resolves type-ref candidates by name.
  `resolved_type_edge` was 0 over all of typescript-go (go.REPORT.md step 2).

## Build, three legs, one commit each, fail-first test each
1. **Receiver types.** A method site `x.M()` where `x` is a local var,
   param, field, or receiver whose declared type (or `&T`, `[]T` element,
   map value) is a named type `T` in the corpus binds to `T.M` (method with
   receiver `T` or `*T`), same package first, then the package the type's
   import names. Type source: the innermost `var`/`:=`-with-composite-literal
   /param/field/receiver declaration in scope; a `:=` from a call result is
   OUT OF SCOPE (record it as an `unresolved` row, reason `inferred`, through
   the drops channel PR #548 added). Ambiguous (two `T` in scope) -> no edge,
   `unresolved` reason `ambiguous`.
2. **Interface dispatch.** Mint a CallF def node for every `method_spec`
   (kind Method, name = spec name, span = the spec). A site whose receiver
   type is an interface `I` binds to `I.M` (the spec). Then one
   `impl` edge per implementer: for every named type `T` in the same
   module whose method set covers all of `I`'s specs, emit
   `resolved_edge` `I.M -> T.M` with a new `CallEdgeKind::Implements`
   variant. Adding the variant touches `src/types.rs` ONE line plus the
   `_` arms in `tests/golden_parity.rs`; that is the ONLY permitted
   edit outside your ownership, and it goes in its own commit named
   `types: CallEdgeKind::Implements`. Cross-check the implementer set
   against `scip_impl` rows on the fixture (`--family scip`).
3. **Builtins.** A table of go builtins (`append cap clear close complex
   copy delete imag len make max min new panic print println real recover`)
   plus predeclared type conversions (`int int8 ... float64 string byte rune
   bool error any`). A site whose callee is in the table and has no local
   shadow emits an `unresolved` row with reason `builtin` (never a corpus
   edge), so the unresolved ratio stops counting them as gaps.

## Tests
`tests/55_go_type_plane.rs`, fixtures under `tests/fixtures/go_findings/type_plane/`:
receiver on local var, on param, on pointer, on struct field, on slice
element; `:=` from call result -> `inferred`; interface method call ->
spec; two implementers -> two `Implements` edges; a type missing one spec
-> no edge; builtin `len` -> `unresolved builtin`; a local func named
`len` shadows the builtin -> corpus edge. COUNT tests: edges == the
fixture's written bindings; interface implementer scan is one pass per
module (assert wall(200 types)/wall(100 types) < 2.5 on a generated
fixture), never per site.

## Receipt
Rerun `plans/extract-crawl-2026-08-29/go.crawl.py` over
`/Users/chrishafley/projects/typescript-go` with YOUR binary, whole project,
104 program roots: reachable 4,253/18,849 -> n; unresolved sites 113,685 -> n
with the breakdown by reason (`builtin`, `inferred`, `ambiguous`, none).
Append a "Fixes 2" table to go.REPORT.md. Gate:
`cargo test --features cli --no-fail-fast` (background, log file, poll), SUM
line. Push, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-go-type-plane sprefa-coordinator "go type plane: PR #N, reachable 4,253-><n>, unresolved 113,685-><n>, gate <p>/<f>"`.

## Laws (inline)
No em dashes. No `eprintln!`. Comments state constraints only, no history.
Descriptive names. No per-site scans of the whole def index: build the
per-module method-set map once. Every `extract` call under `timeout 10`.
No `cargo fmt` across files you do not own. Commit per leg; never
`--no-verify`.
