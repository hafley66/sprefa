# Brief: ts member calls on typed receivers (lane `fix-extract-ts-member-calls`)

Read `plans/extract-corpus-2026-08-28/COMMON.md`,
`plans/extract-bench-2026-08-29/ORACLES.REPORT.md` sections 4, 7, 11, and
`plans/extract-crawl-2026-08-29/ts5.REPORT.md` section 10. On
~/projects/TypeScript-5.9/src (600 files): call recall against the
TypeChecker is 64.2% (35,719 of 55,611); 7,976 sites stay `ambiguous`, all
member calls (`x.f()`) whose receiver type the parse arm never reads. The go
arm's #554 and #562 receiver legs are the template: bind the receiver from
its declaration (param annotation, `const x: T`, class field, `this` inside
a class, `new T()`, and ONE hop `const x = f()` through `f`'s declared
return type), then `T.f` from the class/interface members the type plane
already emits (`--family type` nodes, `src/lang/ts.rs`).

## First action
```
git merge --ff-only c5fd7659232bbe874ec4d5c52f2e59d02b753dce
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-ts-member-calls sprefa-coordinator "<one line>"`.

## Build
1. Receiver table per function body: name -> named type, from the sources
   above, in source order, one pass. Union / generic / inferred-from-literal
   receivers stay `ambiguous` (record reason `inferred` like go).
2. `x.f()` with `x -> T`: bind to member `f` declared on `T` (class or
   interface, same file or through the module plane's `resolve_export`;
   `extends` one hop up). `this.f()` inside class `C` binds `C.f`. Static
   `T.f()` where `T` is a class binds the static member.
3. Edge kind: the existing `value_ref` / `name_resolve` vocabulary; add
   nothing to `CallEdgeKind`. Interface-typed receivers bind the interface
   member; implementer fan-out is a later arc.

## Ownership
`src/lang/ts.rs`, a new `src/lang/ts_receivers.rs`,
`tests/65_ts_member_calls.rs`, `tests/fixtures/ts5_findings/member_calls/**`,
ts5.REPORT.md (append section 11). Forbidden: `src/lang/ts_resolve.rs`
beyond calling its public fns, `src/types.rs`, `src/project.rs`, every
other language.

## Tests, fail-first
param annotation; `const x: T`; class field; `this.f()`; `new T().f()`;
one-hop `const x = f()`; interface receiver; `extends` one hop; union
receiver stays `inferred`; a member missing on `T` -> no edge. COUNT: the
receiver table is one pass per body (wall(400)/wall(200) < 2.5).

## Receipt, ONE process
`extract --resolve --project-root ~/projects/TypeScript-5.9 $(find src -name '*.ts' ! -name '*.d.ts')`,
normalize with `plans/extract-bench-2026-08-29/normalize.py`, overwrite
`ts5.parse.call.tsv`, `bench.py ts5.parse.call.tsv ts5.oracle.call.tsv`:
recall 64.2% -> n, precision 60.2% -> n; `ambiguous` 7,976 -> n. Gate in
background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-ts-member-calls sprefa-coordinator "ts member calls: PR #N, recall 64.2%-><r>, ambiguous 7,976-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, no
`cargo fmt` outside files you own, never --no-verify.
