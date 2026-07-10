---
name: project_dots_types_tsimport
description: feat/dots-types-tsimport — chained dots + tree-sitter type auto-import; in worktree; one-model/comptime is the next direction
metadata: 
  node_type: memory
  type: project
  originSessionId: 2be5eb40-4f91-46fe-84ea-a3848747273b
---

Follow-on to [[project_dots_types_nesting]]. Base `f0422142` (== local
`main`, not stale). Worktree `/Users/chrishafley/projects/sprefa-dots-types`;
primary repo dir back on `main` with user's pre-existing working files intact.

**SPLIT 2026-05-18** after 2-agent review found a reproduced crash:
- `feat/dots-chained` — **MERGED to main 2026-05-18.** Hit the stale-base
  gotcha (main had advanced f0422142→3f4163f3 via the recursion/retraction
  merge); rebased onto current main, re-verified green 16/16 (NOT
  green-faked — re-ran after rebase), ff-merged. main now
  `36f15733` (chained dots) / `57492274` (rename) / `3f4163f3`. Branch
  deleted (== main). NOT pushed.
- `feat/ts-import-wip` = full stack incl b8d72264 (importer) + 28f7cfc9
  (demo). Worktree on this. PARKED, blocked on task #5. **Its base is now
  stale** (still f0422142; main is 36f15733) — rebase onto main before
  resuming #5/#4. The rename+chained-dots commits it carries are now
  redundant with main (will drop in the rebase).

**BLOCKER (task #5), reproduced via /tmp/mixlang.sprf:** ast(:rs)+ast(:cpp)
in one program panics `fact_store.rs:325` — flat table namespace, declare
asserts on colliding kind names (e.g. `assignment_expression` cols differ
rs vs cpp). Also: user `rule(:block,...)` collides; `synth().expect()` =
process crash on bad JSON; SQLite does CREATE TABLE ×369 per store. Both
reviewers: namespace imported tables by lang, lazy/demand declare, don't
hard-panic, land #4 before any comptime work (comptime before #4 = backwards).

Original commits (now on feat/ts-import-wip), all were green in isolation:
- `369d716f` rename `Provenance`→`CaptureSource` (banned identifier)
- `fabf4d14` `LowerCtx` gains per-(table,col) type map (`set_col_type`/
  `col_type`, RefCell); `resolve_dot` re-`.typed()`s its projection so
  `x.a.b.c` resolves past hop 1. Test `dots_chained_target.rs`.
- `b8d72264` `ts_import.rs`: `node-types.json`→decl-only sprf types, vendored
  `v4/assets/node-types/{rs,cpp}.json`, process-global parse cache,
  `ensure_lang_imported` hooked into AstDef + AstYamlDef lower. Single-named
  declared-kind fields linked via set_col_type → chaining. Test
  `ts_import_smoke.rs`.

Plus `28f7cfc9` runnable demo `v4/examples/dots-types-demo.sprf` (verified
via sprefa-run: 6 rust_fns rows; bare `ast(:rs)` triggers import; documented
one-liner shows loud lang/dot-miss).

Deferred / AGREED NEXT (task #4): when ast matches, copy the live tree-sitter
node's field bytes into the imported type's columns per row, so
`function_item.name` carries real data at runtime (today it compile-resolves
but is empty at runtime). This is the data-population half and pairs with the
one-model move (`ty`→`Value`, reflection). Also deferred: union/supertype
field types, langs without a vendored grammar (no-op).

**Next direction the user asked for (2026-05-18):** unify dot + types + values
into ONE model, Zig-comptime style. The gap: `DotTable.ty` is `Option<Arc<str>>`
(a name), not a `Value`. Make `ty` a `Value` + a meta-type `Type` whose cols are
{name,fields,kind} → types become dottable (reflection: `function_item.fields`,
`._kind`). comptime == `resolve_dot` at lower time on literal args (regime A,
already exists). Lattice rides orthogonally as value level. Proposed building
the reflection slice next, additive on this branch.

## Task #4 design — PARKED 2026-05-18 (approved-as-design, not coded)

Make matched ast nodes hand real field text to typed columns. Read side
(resolve_dot → Term::read) already works; only write side missing.

- `ts_import.rs`: add `pub fn fields_for(lang, kind) -> Option<Arc<[Arc<str>]>>`
  (named fields, declared order). ImportedType gains `fields: Arc<[Arc<str>]>`;
  cache value becomes `Grammar { types, by_kind: HashMap<kind, Arc<[..]>> }`.
- `v2_ops.rs AstNmComponent::render_batch`: replace `.map(|nm| nm.range())`
  with collect `(range, Vec<(col, text.into_owned(), child.range())>)` —
  extract owned text INSIDE the map closure before `grep` drops (Node<'r>
  borrows grep). For matched node: kind=nm.kind(); fields_for(lang,kind);
  per field `nm.field(f)` → text; always push `_kind`.
- hits builder: after LO/HI, `if child.get(col).is_none() { child.set(col,..);
  child.set_at(col,..,Coord{lo,hi}) }` — never clobber a user metavar.
- ast-grep-core 0.36.3 Node API confirmed: kind()/range()/text()/field(name).
- Risks: term-name collision (mitigated by get().is_none()); `body` field
  copies whole body (defer coord-only refine); per-field cost bounded (≤6,
  cache-hit only, inside par_render).
- key_terms stay [LO,HI]; field terms ride val-hash complement; rename → new
  _id (correct retraction).
- Tests: fields_for ordering; runtime test on `fn foo(){}` → term name=="foo",
  _kind=="function_item" (model on source_aware_ast_smoke.rs).
- Scope: ~1 fn in v2_ops.rs + 1 lookup in ts_import.rs + 2 tests, same branch.

**Why:** the user likes Zig comptime and wants the model collapsed, not three
parallel concepts.
**How to apply:** when resuming, the reflection slice is the agreed next unit;
ask before merging this branch to main.
