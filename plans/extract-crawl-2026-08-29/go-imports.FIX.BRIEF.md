# Brief: go cross-package call resolution (lane `fix-extract-go-imports`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law).
Findings come from `go.REPORT.md` in your tree; read its kinks table first.

## First action
```
git merge --ff-only c60e5c4cc
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-go-imports sprefa-coordinator "<one line>"`.
(If the build cannot find `../../../hafley-rs`, say so in the hail; the
coordinator adds the symlink.)

## Method
Every fix: failing test FIRST (red output pasted in the commit body), fix,
green, one commit per fix. Fixtures under `tests/fixtures/<lang>_findings/`
already exist for most rows; reuse them. Never weaken a golden or parity
test; regenerate `tests/fixtures/kind_vocab/wire_golden.jsonl` only by the
procedure `tests/6_kind_vocab.rs` documents and state the hunk count. Run
the gate as `cargo test --features cli --no-fail-fast` and report the SUM
over all binaries. No whole-crate `cargo fmt`. No subagents.

## Files you own
`v6/sprefa-extract/src/lang/go.rs`, new `tests/51_go_package_resolve.rs`,
`tests/fixtures/go_findings/**`, `tests/fixtures/go/*.v5.jsonl` (only if
the parity matrix demands a regenerated row; explain in the commit body).
Forbidden: every other file.

## The gap (measured on ~/projects/typescript-go, 5,097 files)
159,740 sites, 46,055 resolved, entrypoint reachability 1.1%. Class 1 of
the unresolved: pkg-qualified calls `ast.IsStringLiteral(...)` where the
callee lives in another package of the same module. The Go resolve arm
matches bare names only; it never reads the file's `import` block, so a
selector call `pkg.Func` never joins to `Func` in the package the import
names. Read how the rust arm resolves `module::path` (`src/lang/rust.rs`,
`call_name_match` and its `callee_path` handling) and how the ts arm uses
`specifier` rows and `ts_resolve.rs`; the go arm already emits import
`specifier` rows (`tests/25_go_specifiers.rs`).

## Build
1. Site shape: a selector call `x.F(...)` where `x` is an imported package
   name (per the file's specifiers, honoring aliases and the default
   last-path-segment name) mints its site with `callee = "F"` and
   `callee_path = <import path>` (the string in the import spec). A
   selector whose receiver is NOT an import name stays as today.
2. Resolve: in `Resolve<CallF>` for go, a site with a `callee_path` joins
   only to defs in files whose package directory matches the import path's
   last segment under the module root (`go.mod` `module` line + relative
   dir; read `go.mod` from the nearest ancestor of the file, cache per
   run). A same-package bare call keeps the current behavior. Emit the
   edge with `kind: name_resolve` as today.
3. Method-on-known-receiver is OUT of scope (say so in the report);
   interface dispatch is OUT of scope; builtins get a named `unresolved`
   reason only if the arm already emits `unresolved` rows (check; if not,
   leave a report row).
COUNT test: a synthetic module in a tempdir with `go.mod`, 3 packages, 40
cross-package calls; assert the resolved_edge count includes all 40 and
that `--resolve` wall over 400 generated files keeps ratio(400/200) < 2.5
(`tests/46_resolve_scaling.rs` shape). Corpus receipt: rerun the crawl
script `plans/extract-crawl-2026-08-29/go.crawl.py` over typescript-go and
put resolved_edge and reachability before/after in the Fixes table (before:
46,055 and 201/18,849).

## Deliverables
Commits as above; append a Fixes table (kink / before / after / test) to
the report named at the top; push; `gh pr create --base main`; hail
`boop beep --no-wait --as fix-extract-go-imports sprefa-coordinator "fix-extract-go-imports: PR #N, <fixes>, gate <p>/<f>"`.
