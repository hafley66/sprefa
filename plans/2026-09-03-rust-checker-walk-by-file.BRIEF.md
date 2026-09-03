# brief: the rust checker walk is priced by the supplied files, never by the crate

Lane: `fix/rust-checker-walk-by-file`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Build and test with `--features cli,rust-checker`.

## The measurement

Hand probe 2026-09-03, one supplied file, `RUST_LOG=info extract --witness --resolve --family type --project-root . --rust-checker src/trace.rs`:

```
rust checker tier loaded load_ms=1914 walk_ms=187159 files=1 unjoined=0 external=77 method_sites=113 method_unresolved=105
```

Run 1 emitted 1129 `rust.impl`, 1129 `tsi.conforms`, 983 `tsi.callable`, 984 `tsi.symbol`, 1501 `tsi.type` for a 254-line file. `src/lang/rust_checker_ra.rs:404` in `run`: after the supplied modules' own declarations, `Impl::all_in_crate(self.db, krate)` for every crate a supplied file belongs to, and `implementation` (`:593`) describes each one (its trait, its self type, every assoc item, so every method signature in the crate). The walk costs the crate. ARCH row `rust_checker_walk_scales_with_crate`. The 10-second law: the workspace LOAD carries the SCIP exception (`project.rs` `CHECKER_BUDGET` comment); the walk does not.

## The shape

```rust
// rust_checker_ra.rs, Walker::run
// pseudo:
//   modules, crates as today (:377-390)
//   declared: BTreeSet<Adt|Trait> = every Adt and Trait among the supplied modules' declarations
//   for module in modules { for def in module.declarations { self.declaration(def, krate) } }   // as today
//   impls: Vec<Impl> =
//        declared adts   .flat_map(|adt| Impl::all_for_type(db, adt.ty(db)))        // ra_ap_hir 0.0.349 lib.rs:4601
//     ++ declared traits .flat_map(|t|   Impl::all_for_trait(db, t))                // lib.rs:4644
//     dedup by Impl (it is Copy + Eq + Hash)
//   for item in impls { self.implementation(item, krate) }
```

An impl for a type declared outside the supplied files, of a trait declared outside them, is not walked: those types stay leaves, which the `tsi.edge` coverage diagnostic already says (`enumerated for workspace-declared owners; std and dependency types are leaves`). Reword that diagnostic to `enumerated for owners declared in the supplied files`. `tsi.conforms` coverage text likewise: `declared impls of supplied types and traits; blanket and auto traits not enumerated`.

`all_for_type` on a generic Adt: build the type with `Adt::ty(db)` (the type with its own parameters), which is what `all_for_type` matches on. If a fixture impl is missed that way (`impl Trait for User<u32>`), fall back to `all_in_crate` FILTERED by `self_ty().as_adt() in declared || trait_() in declared`, and say which in the PR body with the count difference. Either way the described set is a function of the supplied files.

## Receipts

1. `tests/108_rust_checker_walk_by_file.rs` under `#![cfg(feature = "rust-checker")]`, SABOTAGE RECEIPT header stating the base sha and the row counts it emits for the case below.
   - `the_walk_describes_impls_of_supplied_types_only`: the `rust_probe` fixture gains a second module `src/other.rs` (declared in `lib.rs`, ungated) holding `pub struct Other; impl core::fmt::Debug for Other { ... }` and `pub trait Elsewhere {}` with `impl Elsewhere for Other {}`. Supplying `src/lib.rs` alone emits NO `rust.impl` whose owner is `Other`; supplying `src/other.rs` alone emits exactly its two impls; supplying both emits both sets.
   - `every_supplied_impl_is_still_described`: `102_rust_semantic_tsi`'s pinned `COMPLETE`/`PARTIAL` sets and row sets over `rust_probe/src/lib.rs` are unchanged (that file declares its own types and traits, so the new filter keeps every impl it had).
   - `walk_time_is_priced_by_the_file`: the hand probe on THIS crate, `src/trace.rs`, `walk_ms` read from the `rust checker tier loaded` line (or from `extract --bench`), asserted under 10 000 ms, `#[ignore]` with `--ignored` in the receipt so CI load never fails it; run it three times and paste the three `walk_ms`.
2. `cargo test --features cli,rust-checker --test 102_rust_semantic_tsi --test 107_rust_checker_features --test 108_rust_checker_walk_by_file --test 100_tsi_intersection` green.
3. `src/trace.rs` before-and-after run-1 relation counts in the PR body (before: the block above).
4. Full battery `cargo test --features cli` in the background, `tail -30` pasted; `git diff --stat origin/main...HEAD` lists no golden.

## Ownership

Owned: `src/lang/rust_checker_ra.rs`, `src/lang/rust_checker.rs`, `tests/108_rust_checker_walk_by_file.rs`, `tests/fixtures/tsi/rust_probe/**`, `tests/102_rust_semantic_tsi.rs` (only if a pinned set has to move, with the reason).
Forbidden: `src/project.rs`, `src/lang/rust_type_edges.rs`, `src/tsi/**`, `src/wire.rs`, `src/lang/ts*.rs`, `v7/**`, `docs/**`, `v6/prolog/ARCH.pl`.

## Style laws

No em dashes. Comments state only constraints the code cannot show. `tracing` only. Descriptive names. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth"; "support" is banned. Commit subject: `extract: the rust checker walk is priced by the supplied files`.

## Done

Push, PR against `main` with receipts, then:
`boop beep --no-wait --as fix-rust-checker-walk-by-file sprefa-coordinator "walk-by-file PR #<n>: 108 <n>/<n>, 102 <n>/<n>, trace.rs walk_ms <a> <b> <c>, rust.impl <before> -> <after>, battery <pass>/<total>"`.
