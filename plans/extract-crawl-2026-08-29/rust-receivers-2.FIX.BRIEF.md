# Lane `fix-extract-rust-receivers-2` (opus): the ambiguous method calls #571 left, measured then fixed

Read `plans/extract-crawl-2026-08-29/rust.REPORT.md` sections 16 and 17,
`plans/extract-bench-2026-08-29/ORACLES.REPORT.md` section 13 finding 2,
`v6/sprefa-extract/src/lang/rust_receivers.rs` and `rust_modules.rs`.
After #571: recall vs `rust.oracle.call.tsv` 33.9%, precision 51.4%,
ambiguous drops 8,799 (bench universe). Section 16 projected class (a),
method call with a traceable receiver type and an in-corpus impl, at 43,940
of 52,101 before the receiver leg; the leg took less than half of it.

## First action
```
git merge --ff-only 5b435ac023d28477414497ce7a062c7dfb835a8c
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus: `/Users/chrishafley/projects/rust-analyzer`, files
`find crates -name '*.rs' -path '*/src/*'`. ONE process per run:
`timeout 30 extract --resolve --project-root <corpus> <files>` in
background with a log. Over 30 s: report the wall, keep going with the
output you have.

## Step 1: measure (commit the tsv, one table in rust.REPORT.md section 18)
Sample 300 of the remaining `ambiguous` sites (seed 7) and classify by
WHY the receiver leg did not bind: receiver is `self` in a trait default
method, receiver is a struct field (`self.x.m()`), receiver is a chain tail
whose earlier hop is a trait method, receiver type is a generic param with a
bound, receiver type is behind `&`/`Box`/`Rc`/`Option` (auto-deref),
impl is `impl Trait for T` and the site names the trait method, closure
param, other. Count, projected count, two file:line each, the rust.rs or
rust_receivers.rs fn that would take it.

## Step 2: fix the top class, fail-first
Fixture under `tests/fixtures/rust_findings/receivers/src/`, test in
`tests/68_rust_receivers.rs`, failure text pasted in the test header.
Then the fix. If the top class is auto-deref, the rule is: strip
`&`, `&mut`, `Box<`, `Rc<`, `Arc<`, `Option<` (for `.as_ref().m()`
chains only) before the impl lookup. If it is `impl Trait for T`, bind the
site to the impl block's method when exactly one impl of that trait
matches the receiver type, and keep `ambiguous` otherwise (cap 64 like go's
`FanoutCap`, reuse `UnresolvedReason::FanoutCap`).

## Step 3: the field-shadow twin
ORACLES.REPORT.md section 13 finding 2: a field or token named like a type
(`Resolver` bound to `hir-ty/src/infer/unify.rs` instead of
`hir-def/src/resolver.rs`, 17 rows) makes the `type` family ref point at
the wrong file. Filter type-ref candidates to type defs (struct, enum, trait,
type alias) before the name match. Fail-first test, same file.

## Receipt
Re-run the one-process resolve and
`plans/extract-bench-2026-08-29/bench.py <yours normalised> rust.oracle.call.tsv`
(normalise with `normalize.py`; the lane `bench-extract-single-process`
owns the `plans/extract-bench-2026-08-29/*.tsv` files, so write yours under
`plans/extract-crawl-2026-08-29/`). PR body: recall 33.9% -> n, precision
51.4% -> n, ambiguous 8,799 -> n, type misbound 17 -> n, gate counts.

## Ownership
`v6/sprefa-extract/src/lang/rust.rs`, `rust_receivers.rs`,
`rust_modules.rs`, `tests/68_rust_receivers.rs`, its fixtures,
`plans/extract-crawl-2026-08-29/rust.*`. NOT `src/types.rs` (add no enum
variants; if you need one, hail the coordinator with the variant name),
NOT `src/lang/go*.rs`, `ts*.rs`, NOT `plans/extract-bench-2026-08-29/`.
No `cargo fmt` on files you do not own. Gate `cargo test --features cli
--no-fail-fast` in background with a log; wall-ratio tests that fail under
load rerun 3x isolated, say so in the PR body. Never commit a file over 1 MB.

Push `fix/extract-rust-receivers-2`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-receivers-2 sprefa-coordinator "rust receivers 2: PR #N, top class <name>, recall x%->y%, ambiguous 8799->n, gate a/b"`.
Laws: no em dashes, no eprintln, descriptive names, comments only for what
code cannot show, no words provenance/substrate/load-bearing/regime/refusal,
never "ground truth" (say oracle).
