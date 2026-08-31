# Brief: go inferred receivers (lane `fix-extract-go-inferred`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` and
`plans/extract-crawl-2026-08-29/go.REPORT.md` Fixes 2 to 4. After #554/#558/#560
the largest unresolved bucket on typescript-go is `unresolved reason=inferred`:
19,022 sites where the receiver is a `:=` bound from a call result
(`x := pkg.New(); x.M()`), which #554 deliberately left out.

## First action
```
git merge --ff-only e90322438c2871543e1e6e339d26910590a09c6c
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-go-inferred sprefa-coordinator "<one line>"`.

## Build: one hop of return-type inference, no fixpoint
A `:=` (or `var x = `) whose right side is a call `f(...)` / `pkg.F(...)` /
`recv.M(...)` that the existing legs already bind to a corpus def D: read
D's declared result type from the signature the parse arm already emits
(`fn_sigs`, `go.rs:392`; single result, or the first result of a
`(T, error)` pair; `*T` and `T` both name `T`). Bind `x` to that named
type for the rest of the enclosing function body (the receiver-binding
table #554 added, `ReceiverBinding`). Multi-assign `a, b := f()` takes
result i for variable i. A call the legs cannot bind, a func literal, a
type parameter, or an interface result with no corpus decl: the site stays
`inferred`. No second hop (`y := x.M()` where `x` itself was inferred IS
allowed because `x`'s type is now known; chains resolve in source order
within one pass over the body). Build the def -> result-type map once per
corpus, never per site.

## Ownership
`src/lang/go.rs`, `src/lang/go_types.rs` if you split it, `tests/63_go_inferred.rs`,
`tests/fixtures/go_findings/inferred/**`, go.REPORT.md (append "Fixes 5").
Forbidden: every other file under `src/`. If a struct in `src/types.rs` needs
a field, say so in the PR body and stop at that leg.

## Tests, fail-first
`:=` from a same-package func; from an import-qualified func; from a method
on a known receiver; `(T, error)` first result; multi-assign index; chain
`a := f(); b := a.M(); b.N()`; unbound callee stays `inferred`; interface
result stays `inferred`. COUNT: the def -> result-type map is built once
(wall(400)/wall(200) < 2.5).

## Receipt
ONE process: `extract --resolve --project-root <corpus> $(cat gofiles.txt)`
(never `xargs`; #560 proved xargs partitions the corpus). `unresolved
inferred` 19,022 -> n; `bench.py go.parse.call.tsv
plans/extract-bench-2026-08-29/go.oracle.call.vta.tsv` recall 8.67% -> n;
`go.crawl.py` reachable 4,833 -> n. Gate in background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-go-inferred sprefa-coordinator "go inferred: PR #N, inferred 19,022-><n>, call recall 8.67%-><r>, reachable 4,833-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, no
`cargo fmt` outside files you own (the coordinator reverts it), never --no-verify.
