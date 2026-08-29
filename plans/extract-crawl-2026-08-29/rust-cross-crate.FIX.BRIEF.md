# Brief: rust cross-crate ambiguity (lane `fix-extract-rust-cross-crate`)

Read `plans/extract-corpus-2026-08-28/COMMON.md`,
`plans/extract-crawl-2026-08-29/rust.REPORT.md` sections 4, 13, 14, 15 and
`plans/extract-bench-2026-08-29/ORACLES.REPORT.md` section 7 (rust call
recall vs `ra_ap_ide` 31.0%, 12,624 of 40,686). After #552 the rust arm still
drops 15,184 sites as `ambiguous` (2+ corpus defs of the name, none bound
by a `use`) and 65,992 as `no_corpus_def`. Corpus:
/Users/chrishafley/projects/rust-analyzer (read-only), 873 src files.

## First action
```
git merge --ff-only eda356c419dbb911474189582ebe866e5e233ebd
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.
Failure: STOP, `boop beep --no-wait --as fix-extract-rust-cross-crate sprefa-coordinator "<one line>"`.

## Measure first (one process, never xargs)
`extract --resolve --project-root <corpus> $(find crates -name '*.rs' -path '*/src/*')`;
take the `ambiguous` rows, sample 300, classify by what the compiler would
use: (a) method call `x.m()` where `x`'s type is declared in scope (param
annotation, `let x: T`, `Self`, field type) and `impl T { fn m }` or
`impl Trait for T` is in the corpus; (b) associated fn `T::new()` /
`Self::f()`; (c) trait method through a generic bound; (d) free fn shadowed
by a glob (`use super::*`, `use crate::prelude::*`); (e) other. Table
with counts, projected totals, 2 file:line each. Write it as section 16 of
rust.REPORT.md BEFORE the fix.

## Build the top two classes only
1. Receiver typing for (a)/(b), the go #554/#562 shape: receiver table per fn
   body from param annotations, `let x: T`, `Self`, struct field types, and
   one hop `let x = f()` through `f`'s declared return type (`-> T`,
   `-> Result<T, _>` takes `T`, `-> Option<T>` takes `T`); then `T::m` from
   the impl blocks the parse arm already emits (`impl` nodes, trait impls
   included, same file or through the module plane). `Self::f()` inside
   `impl T` binds `T::f`. `T::new()` binds the inherent assoc fn.
2. Glob resolution (d): a name with no explicit `use` binding but reachable
   through exactly one `use path::*` in the file binds through the module
   plane (`rust_modules.rs` already resolves the glob target; extend the
   lookup to the glob's exported set). Two globs offering the name stays
   `ambiguous`.

## Ownership
`src/lang/rust.rs`, new `src/lang/rust_receivers.rs`, `src/lang/rust_modules.rs`
(glob export sets only), `tests/68_rust_receivers.rs`,
`tests/fixtures/rust_findings/receivers/**`, rust.REPORT.md (sections 16, 17).
Forbidden: `src/types.rs`, `src/project.rs`, every other language.

## Tests, fail-first
param-typed receiver; `let x: T`; `Self::f`; `T::new`; one hop through
`-> Result<T, E>`; trait impl method; glob single source binds; two globs
stays ambiguous; unknown receiver stays `inferred` (add the reason if the
rust arm lacks it, in `call_drops`, one line). COUNT: receiver table one
pass per body, impl map one pass per corpus (wall(400)/wall(200) < 2.5).

## Receipt
Same one-process run; normalize with `plans/extract-bench-2026-08-29/normalize.py`,
overwrite `rust.parse.call.tsv`, `bench.py rust.parse.call.tsv rust.oracle.call.tsv`:
recall 31.0% -> n, precision 46.8% -> n; `ambiguous` 15,184 -> n;
`rust.crawl.py` union 12,924 -> n. Gate in background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-rust-cross-crate sprefa-coordinator "rust receivers: PR #N, recall 31.0%-><r>, ambiguous 15,184-><n>, union 12,924-><n>, gate <p>/<f>"`.
Laws: no em dashes, no eprintln, comments state constraints only, no
`cargo fmt` outside files you own, never --no-verify.
