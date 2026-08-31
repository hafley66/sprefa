# Lane `fix-extract-rust-traits` (glm53f): rust trait dispatch and the class 8/11 remainder

Read `plans/extract-crawl-2026-08-29/rust.REPORT.md` sections 18 and 19
(PRs #576, #582). After #582 one-process `ambiguous` is 11,628; classes
3a/3b/5/10 (about 8,650 rows) name external types and are the ceiling.
What is left in-corpus:

| # | class | rows | rule |
|---|---|---:|---|
| 8 | `T::f()`, T a corpus struct/enum, still 0 or 2+ impls | 1,145 | when 2+ impls survive the inherent/trait tiebreak, bind to the impl whose trait is imported by the caller's module (module plane `use` table); else stay ambiguous. When 0: `f` is a trait-provided assoc fn (default body in the trait); bind to the trait's fn def |
| 11 | module-qualified `mod::f()` | 1,207 | classify 100 (seed 7): how many are `crate::a::b::f` with the full chain, `super::`, or an alias from `use x as y`; fix the top shape |
| 12 | `T::f()`, T a corpus TRAIT (`Default::default`, `Iterator::next`) | 260 | bind to the trait's fn def (the trait declares it); when the receiver type at the call is known and exactly one `impl Trait for Type` exists in the corpus, bind to the impl fn instead |
| 4 | corpus struct/enum, method is a trait DEFAULT body | 199 | `impl_facts` gains trait default fns; a type that `impl Trait for T` without overriding `m` binds `t.m()` to the trait's default `m` |
| 6 | receiver type is a corpus trait (`dyn T`, `impl T`, `T: Trait` bound) | 161 | bind to the trait's fn def; cap fan-out to implementers at 64 like go (`UnresolvedReason::FanoutCap`), add no variants |
| 6b | receiver is a generic param with a bound | 60 | read the bound from the fn generics / where clause, then class 6 |

## First action
```
git merge --ff-only cb94fae203f1dd2f55e87cb06c68eb02daac4f2c
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/rust-analyzer`, `find crates -name '*.rs' -path '*/src/*'`,
ONE process, `--resolve --project-root <corpus>`, `timeout 30`, background, log
(2.06 s at #582).

## Order
12, 4, 6, 6b share one table: trait -> its fn defs (declared and default
bodies) and type -> traits it implements (`rust_modules.rs` `impl_target`
already knows impl blocks; extend, do not duplicate). Build that once,
fail-first test per class in `tests/71_rust_paths.rs` or a new
`tests/72_rust_traits.rs`, fixtures under `tests/fixtures/rust_findings/traits/`.
Then class 8, then the class 11 census and its top shape. Commit after each
green test; three agents on this file died mid-turn with work uncommitted.

## Receipt
Re-run `plans/extract-crawl-2026-08-29/rust.paths3.census.py` (from #582)
and `bench.py` vs `rust.oracle.call.tsv`. PR body: per-class before/after,
ambiguous 11,628 -> n, overlap 18,296 -> n, wall, gate. `just extract-ratchet`
green; `RATCHET_BUMP=1` when the rust rows improve.

## Ownership
`v6/sprefa-extract/src/lang/rust.rs`, `rust_receivers.rs`, `rust_modules.rs`,
rust test files and fixtures, `plans/extract-crawl-2026-08-29/rust*`,
`plans/extract-bench-2026-08-29/RATCHET.tsv` (rust rows only). NOT
`src/types.rs`, `src/project.rs`, `go*.rs`, `ts*.rs`, `scip*.rs` (a live lane
owns scip). No `cargo fmt` on files you do not own. Gate in background with
a log; wall-ratio flakes rerun 3x isolated. No file over 1 MB. Budget 60 min;
past it, post the PR with what is green.

Push `fix/extract-rust-traits`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-traits sprefa-coordinator "rust traits: PR #N, ambiguous 11628->n, overlap 18296->n, gate a/b"`.
Laws: no em dashes anywhere including test headers, no eprintln, descriptive
names, comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
