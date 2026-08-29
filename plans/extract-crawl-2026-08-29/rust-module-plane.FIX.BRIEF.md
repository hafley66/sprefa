# Brief: the rust module plane (lane `fix-extract-rust-module-plane`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law),
`plans/extract-crawl-2026-08-29/rust.REPORT.md` sections 4, 11, 12, and
`plans/extract-crawl-2026-08-29/ts-module-plane.FIX.BRIEF.md` (the ts twin,
landed). User decision (2026-08-29): module resolution is the LANGUAGE'S
OWN algorithm, run once per file set as its own plane; every resolve arm
binds imported names through it; name-matching across files is the last
leg. The ts plane emits `resolved_import` rows and `import_resolve` edges;
the rust plane emits the SAME row shape (`--schema`, `src/schema.rs`
MODULE PLANE section, `src/types.rs` `ResolvedImportRow`). Do not add
columns.

## First action
```
git merge --ff-only BASE_SHA
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-rust-module-plane sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree, never a
globally installed one.

## Ownership
Yours: `src/lang/rust.rs`, a new `src/lang/rust_modules.rs`,
`tests/57_rust_module_plane.rs`, `tests/fixtures/rust_findings/module_plane/**`,
`plans/extract-crawl-2026-08-29/rust.REPORT.md` (append a section 13).
One `OnceLock` slot `rust_modules` in `src/types.rs` `IndexBag`, additive,
beside `ts_modules`, in its own commit. Forbidden: `src/lang/go*.rs`,
`src/lang/ts*.rs`, `src/project.rs` beyond the one line that builds the slot,
`src/schema.rs` beyond editing the sentence that says rust emits no row.

## What exists (read before writing)
- `collect_module_leaves` / `use_tree_leaves` (`rust.rs:1690-1770`) already
  read every `use`, `pub use`, glob, rename, `mod foo;` and
  `#[path = ".."] mod foo;` (`mod_path_attr`, `rust.rs:1801`) into
  `Specifier` rows (table at `rust.rs:1642-1660`, pinned by
  `tests/24_rust_specifiers.rs`).
- PR #548 landed the module-qualified call leg and the `unresolved` drops
  channel (`tests/52_rust_crawl_kinks.rs`, `tests/60_rust_corpus_scope.rs`).
- `Resolve<CallF> for RustSource` (`rust.rs:1170`) name-matches; the
  ambiguity rule drops a name with defs in 2+ blobs (`rust.rs:929-938`).
  On rust-analyzer that is 27,267 `ambiguous_cross_crate` + 15,021
  `ambiguous_in_crate` + 9,656 `single_def_cross_crate` sites
  (rust.REPORT.md section 4).

## Build: the Rust Reference name-resolution, as the reference writes it
1. `RustModuleIndex` (new slot): for every `.rs` input, its module path
   from the crate root (`lib.rs`/`main.rs`/`bin/*.rs` per the nearest
   `Cargo.toml`, `mod foo;` -> `foo.rs` or `foo/mod.rs`, `#[path]`
   override, inline `mod x { }` nesting). Reuse whatever PR #548's module
   path helper already computes; extend, do not duplicate.
2. `resolve_use(file, path)` per the reference (paths, `crate::`,
   `self::`, `super::`, `pub use` re-export chains to any depth, `use a::*`
   globs, `use a::{b as c}` renames, `pub(crate)` visibility is NOT
   enforced). Cycle-safe (visited set). External crates (`std`, anything
   not under a corpus `Cargo.toml`) resolve to nothing and mint an
   `unresolved` row with reason `external`. Glob ambiguity (two globs
   offering one name) -> `unresolved` reason `ambiguous`; an explicit
   `use` shadows a glob per the reference.
3. `Resolve<CallF>`: a site whose callee (or the first segment of its path)
   is a `use` binding in THIS file binds through `resolve_use` to ONE def
   and emits `kind: import_resolve`. `crate::a::b::f()` and `super::f()`
   paths bind through the module index directly. A local def in this
   module shadows an import. Only a free name with neither falls to the
   existing name-match. Same for `Resolve<TypeF>`.
4. One `resolved_import` row per `use` leaf per file under `--resolve`,
   kind in {local, indirect (pub use hop), star (glob hop), namespace
   (`use a::b;` where `b` is a module), default never}. hops = re-export
   depth.

## Tests, fail-first, one commit per step
`tests/57_rust_module_plane.rs` + fixtures under
`tests/fixtures/rust_findings/module_plane/` (a two-crate workspace with
`Cargo.toml`s): `use crate::a::f`, `use super::f`, `pub use` one hop, two
hops, glob, glob ambiguity, rename, `#[path]` module, inline `mod` nesting,
local shadows import, cross-crate `use other_crate::f`, external `std::`.
COUNT tests: edges == the fixture's written bindings; wall(400)/wall(200)
files < 2.5 on a generated re-export chain.

## Receipt
Rerun `plans/extract-crawl-2026-08-29/rust.crawl.py` over
`/Users/chrishafley/projects/rust-analyzer` (941 src files, 75 program
roots) with YOUR binary: `import_resolve` edge count; reachable from
program roots 477 -> n; union 12,221 (63.2%) -> n; `ambiguous_cross_crate`
27,267 -> n, `single_def_cross_crate` 9,656 -> n. Section 13 in
rust.REPORT.md. Gate `cargo test --features cli --no-fail-fast` (background,
log, poll), SUM. Push, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-module-plane sprefa-coordinator "rust module plane: PR #N, import_resolve <n>, program-root reachable 477-><n>, union 12,221-><n>, gate <p>/<f>"`.

## Laws (inline)
No em dashes. No `eprintln!`. Comments state constraints only. Descriptive
names. Build the module index once per corpus, never per site. Every
`extract` call under `timeout 10`. No `cargo fmt` outside files you own.
Commit per step; never `--no-verify`.
