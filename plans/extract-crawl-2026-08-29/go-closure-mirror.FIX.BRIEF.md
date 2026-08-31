# Brief: go closure-caller mirror + current-binary receipt (lane `fix-extract-go-closure-mirror`)

Read `plans/extract-corpus-2026-08-28/COMMON.md`,
`plans/extract-bench-2026-08-29/ORACLES.REPORT.md` section 12, and
`plans/extract-crawl-2026-08-29/rust.REPORT.md` section 11 kink 3 (the rust
arm mirrors every `closure@<n>`-caller edge onto the enclosing fn,
`tests/52_rust_crawl_kinks.rs::a_closure_caller_edge_mirrors_onto_the_enclosing_fn`).

## First action
```
git merge --ff-only abb64ef46615f92e8d05430983114ad6af581647
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-go-closure-mirror sprefa-coordinator "<one line>"`.

## Build
1. Go call edges whose caller is a `func_literal` (`closure@<n>`, 4,132 rows
   on typescript-go) get ONE mirror edge onto the innermost enclosing named
   func/method, the same shape and the same edge kind the rust arm uses
   (grep `mirror` in `src/lang/rust.rs`). Package-level func literals
   (`var f = func(){}`) have no enclosing def and get no mirror.
2. Nothing else in the resolve arms.

## Ownership
`src/lang/go.rs`, `tests/64_go_closure_mirror.rs`,
`tests/fixtures/go_findings/closure_mirror/**`, go.REPORT.md (append
"Fixes 6"), `plans/extract-bench-2026-08-29/go.parse.call.tsv` (overwrite
with the current-binary run) and `go.parse.module.tsv`. Forbidden:
everything else under `src/`.

## Tests, fail-first
one mirror edge per closure-caller edge; nested closures mirror to the
named fn, not the outer closure; a package-level literal mirrors nothing;
COUNT: mirrors == closure-caller edges.

## Receipt, ONE process, never xargs
`extract --resolve --project-root /Users/chrishafley/projects/typescript-go $(cat gofiles.txt)`
over all 5,096 files. Normalize with
`plans/extract-bench-2026-08-29/normalize.py` (bare names), overwrite
`go.parse.call.tsv`, then `bench.py go.parse.call.tsv
go.oracle.call.vta.bare.tsv`: recall 45.3% -> n, precision 50.9% -> n.
`go.crawl.py`: reachable 7,786 -> n. Gate in background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-go-closure-mirror sprefa-coordinator "go closure mirror: PR #N, vta recall 45.3%-><r>, reachable 7,786-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, no
`cargo fmt` outside files you own, never --no-verify.
