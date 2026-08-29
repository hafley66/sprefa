# Brief: the module plane, ts first (lane `fix-extract-ts-module-plane`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law).
User decision (2026-08-29): module resolution is the LANGUAGE'S OWN
algorithm, run once per file set as its own plane, and every resolve arm
binds imported names through it. Name-matching across files is the fallback
for names with no import binding, never the first leg.

## First action
```
git merge --ff-only baff35143
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-ts-module-plane sprefa-coordinator "<one line>"`.

## What exists (read before writing)
- `oxc_resolver` 11.24 is linked and `src/lang/ts_resolve.rs` already
  resolves a specifier string to a file (ESM/CJS + tsconfig paths +
  package.json exports); `src/deps.rs` uses it for `--deps`.
- `oxc_semantic` 0.135 is linked (`ts_rename.rs`); its `ModuleRecord`
  carries `import_entries`, `local_export_entries`,
  `indirect_export_entries` (`export { x } from`), `star_export_entries`
  (`export * from`), `export_default`.
- The ts arm emits `specifier` rows (imports/exports as written,
  `tests/39_ts_specifiers.rs`) and `Resolve<CallF>` binds by NAME through
  `call_name_match` (`src/lang/ts.rs`, read it) with the receiver rule from
  PR #547. `IndexBag` has OnceLock slots (`PathIndex`, `KindIndex`,
  `src/types.rs`) built once in `resolve_project` (`src/project.rs`).
- Measured on ~/projects/TypeScript-5.9 `src/**` (`ts5.REPORT.md`): 11,768
  sites ambiguous by name; `export * from` barrels
  (`src/compiler/_namespaces/ts.ts`) are why; 2,399 sites where a
  module-private def shadows an exported one.

## Build: ECMAScript ResolveExport, as the spec writes it
1. New IndexBag slot `ModuleIndex`: for every ts/js input, its
   `ModuleRecord` (from `oxc_semantic` on the parse the arm already does,
   or re-parse once in `resolve_project`; measure both, keep the cheaper),
   plus each specifier string resolved to a corpus file via
   `ts_resolve.rs` (cache per (dir, specifier)).
2. `resolve_export(file, name)` per ECMA-262 16.2.1.6.3: local export ->
   the def in this file; indirect export -> recurse into the target
   module with the imported name; star exports -> try each, ambiguous if
   two differ; cycle-safe (visited set); `export default` and
   `export =` / `import x = require` (TS) handled; namespace imports
   (`import * as ns`) bind `ns.f` to `resolve_export(target, "f")`.
3. `Resolve<CallF>` for ts: a site whose callee (or member receiver) is
   an import binding in THIS file binds through `resolve_export` to ONE
   def (span in the target file) and emits the edge with
   `kind: import_resolve` (add the `CallEdgeKind` variant and the two
   `_` arms in `tests/golden_parity.rs`, the way PR #547 did for
   `value_ref`). A local name declared in this file binds locally first
   (module-private shadows exported). Only a free name with neither
   binding falls to `call_name_match`. Same for `Resolve<TypeF>`
   (`extends` / type references through imports).
4. Record shape, general across languages (write it into
   `src/schema.rs` and `--schema`): `resolved_import`
   `{src_path, name, local, target_path, target_name, kind:
   local|indirect|star|namespace|default, hops}`, one row per import
   binding per file under `--resolve`; go and rust arms get the same
   record later (state that in the schema text, do not build them).

## Tests, fail-first, one commit per step
`tests/54_ts_module_plane.rs` + fixtures under `tests/fixtures/ts5_findings/module_plane/`:
barrel (`index.ts` with `export * from './a'` and `'./b'`), rename on
re-export (`export { a as b } from`), two-hop barrel, star-export ambiguity
(same name from two stars -> no edge, an `unresolved` row with reason
`ambiguous` via the drops channel PR #548 added), namespace import
member call, default import, module-private shadowing an import, cycle.
COUNT tests: edges == the fixture's written bindings; wall(400)/wall(200)
files < 2.5 on a generated barrel corpus.

## Receipt
Rerun `plans/extract-crawl-2026-08-29/ts5.crawl.py` (and
`ts5.crawl.module.py`) over ~/projects/TypeScript-5.9/src with your binary:
resolved_edge, `import_resolve` edge count, ambiguous-by-name sites
11,768 -> n, A_strict 4,837 -> n, in a Fixes table appended to ts5.REPORT.md.
Gate `cargo test --features cli --no-fail-fast`, SUM. Push,
`gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-ts-module-plane sprefa-coordinator "ts module plane: PR #N, import_resolve <n>, ambiguous 11,768-><n>, A_strict 4,837-><n>, gate <p>/<f>"`.

## Files you own
`src/lang/ts.rs`, `src/lang/ts_resolve.rs`, `src/project.rs`, `src/types.rs`
(additive: the slot, the variant, the record), `src/schema.rs`, `src/wire.rs`
(the new record's serialization only), `tests/golden_parity.rs` (the `_` arms
only), the new test + fixtures, goldens by their documented procedures with
hunk counts. Forbidden: every other language arm, `v6/prolog/**`, `CLAUDE.md`.
No whole-crate fmt. No subagents. No em dashes.
