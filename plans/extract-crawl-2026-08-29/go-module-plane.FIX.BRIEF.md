# Brief: the go module plane (lane `fix-extract-go-module-plane`)

Read `plans/extract-corpus-2026-08-28/COMMON.md`. The ts (#549) and rust
(#552) module planes landed; both emit `resolved_import` rows and
`import_resolve` call edges through one `IndexBag` slot each
(`ts_modules`, `rust_modules`, `src/types.rs`), chained in
`import_facts` (`src/project.rs:1064-1085`). Go has NO slot: the bench
report (`plans/extract-bench-2026-08-29/ORACLES.REPORT.md` defect 2) measured
0 `resolved_import` rows for go over 5,097 files. Go's package plane
(PR #546, `go_module_of` / `go_package_dir` in `src/lang/go.rs`) already
resolves an import path to a directory; this lane makes it a plane.

## First action
```
git merge --ff-only 1de4d763b2a9fe74bb810137bcaf29f5cf7cf04a
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-go-module-plane sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.

## Ownership
Yours: new `src/lang/go_modules.rs`, `src/lang/go.rs` (only the import
lookup inside `Resolve<CallF>` / `Resolve<TypeF>`), ONE additive
`go_modules: OnceLock<GoModuleIndex>` slot in `src/types.rs` `IndexBag`
(own commit), the `go` arm in `import_facts` and the slot build in
`resolve_project` (`src/project.rs`), `tests/62_go_module_plane.rs`,
`tests/fixtures/go_modules/**`, `src/schema.rs` (edit the sentence that
says go emits no row). Forbidden: `src/lang/ts*.rs`, `src/lang/rust*.rs`,
everything else.

## Build, per the Go spec (Import declarations, Package clause)
1. `GoModuleIndex`: for every `.go` input, its package path (nearest
   `go.mod` module path + directory), its package name, and each import
   spec resolved to a corpus directory (reuse `go_module_of` /
   `go_package_dir`; a path outside every corpus module is external).
   Exported names per package = every top-level func/type/var/const whose
   first letter is upper-case, across all files of the directory (build
   once, keyed by directory).
2. `resolve_import(file, qualifier, name)`: the import whose local name
   (explicit alias, else the package clause name of the target directory,
   NOT the last path segment: `import "gopkg.in/yaml.v3"` binds `yaml`)
   equals `qualifier`, then `name` in that package's exported set. Dot
   imports (`import . "pkg"`) put the package's exports in file scope; blank
   imports bind nothing. A local declaration shadows an import.
3. `resolved_import` rows: one per import spec per file, kind `local`
   when the target package is in the corpus (`target_name` = package name),
   `namespace` for dot imports; external imports emit an `unresolved` row
   reason `external` through `call_drops`. Call edges bound this way carry
   `kind: import_resolve` (the variant exists).
4. Interface dispatch and receiver types (PR #554) stay as they are; only the
   package-qualified leg moves onto the plane.

## Tests, fail-first, commit per step
`tests/62_go_module_plane.rs`, fixtures under `tests/fixtures/go_modules/`
(a two-module workspace with `go.mod` files): alias import, package name
differing from the path's last segment, dot import, blank import, external
import -> `unresolved external`, local shadow, unexported name -> no edge.
COUNT: edges == the fixture's written bindings; the exported-set build is one
pass per directory (wall(400 files)/wall(200 files) < 2.5).

## Receipt
`plans/extract-bench-2026-08-29/bench.py` (on origin/main after the bench PR
lands, else on branch `bench/extract-oracles`) against
`go.oracle.module.tsv` (2,152 rows) and `go.oracle.call.vta.tsv`: module
recall/precision, call recall 5.6% -> n. Rerun
`plans/extract-crawl-2026-08-29/go.crawl.py`: reachable 4,832 -> n. Append
"Fixes 3" to go.REPORT.md. Gate in background, SUM. Push, PR, hail
`boop beep --no-wait --as fix-extract-go-module-plane sprefa-coordinator "go module plane: PR #N, resolved_import <n>, module recall <r>, call recall 5.6%-><r>, reachable 4,832-><n>, gate <p>/<f>"`.

## Laws
No em dashes. No `eprintln!`. Comments state constraints only, no dates.
Descriptive names. Every `extract` call under `timeout 10`. No `cargo fmt`
outside files you own. Never `--no-verify`.
