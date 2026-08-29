# Brief: go multi-hop receiver chains (lane `fix-extract-go-multihop`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` and
`plans/extract-crawl-2026-08-29/go.GAPS.md`: third class, multi-hop receiver
chain `a.b().c()`, 2,587 projected vta-only edges (example
`buildtask.go cleanProjectOutput -> iovfs/iofs.go FileExists` through
`orchestrator.host.FS().FileExists`). #562 binds `x := f()` through `f`'s
declared result type (`go_bind_plan_of` / `go_binding_of_rhs`,
`go.rs:983/1195`); the intermediate hop of a chain is the same lookup
applied to an expression instead of a variable.

## First action
```
git merge --ff-only 876bae08b5c5588bdaf1dd29c32c8925c2f34d83
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-go-multihop sprefa-coordinator "<one line>"`.

## Build
Type of a selector chain, left to right, one pass, no fixpoint: a receiver
name -> its bound type (#554/#562 table); `.field` -> the struct field's
declared type (`go_field_types`, `go.rs:1066`); `.M()` -> the declared
first result type of `T.M` (or of the interface method_spec); `.F()` where
`F` is an import-qualified func -> its result type. Each hop that lands on
a named corpus type continues; a hop that lands on an interface continues
through the interface's method_spec result types; anything else stops and
the site keeps its current outcome. Bind the final `.c()` the way #562
binds a one-hop receiver; the interface fan-out from #565 applies when the
last receiver type is an interface. Depth cap 8.

## Ownership
`src/lang/go.rs`, `tests/67_go_multihop.rs`,
`tests/fixtures/go_findings/multihop/**`, go.REPORT.md (append "Fixes 8"),
`plans/extract-bench-2026-08-29/go.parse.call.tsv` (overwrite on the
current binary). Forbidden: everything else under `src/`.

## Tests, fail-first
`a.b().c()` with `b` returning a struct; through a field then a method;
through an interface result then fan-out; through an import-qualified
func; a hop returning a builtin or a generic stops; depth 9 stops. COUNT:
per site the walk is bounded by chain length, no corpus scan.

## Receipt, ONE process
`extract --resolve --project-root /Users/chrishafley/projects/typescript-go $(cat gofiles.txt)`;
normalize bare names; `bench.py go.parse.call.tsv go.oracle.call.vta.bare.tsv`:
recall 75.6% -> n; `go.crawl.py` reachable 8,876 -> n. Gate in background,
SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-go-multihop sprefa-coordinator "go multihop: PR #N, vta recall 75.6%-><r>, reachable 8,876-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, no
`cargo fmt` outside files you own, never --no-verify.
