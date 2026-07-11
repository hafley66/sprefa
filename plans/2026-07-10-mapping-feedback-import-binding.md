# Mapping-session feedback: import bindings, template parts, shallow def-use (2026-07-10)

Source: Chris, live feedback from a large migration / static-analysis mapping session
on an unshareable codebase. The prior session hit snags he had predicted. Recorded
verbatim-in-spirit; overlap notes against existing rels added so implementers don't
duplicate.

## The asks

1. **`import_binding(file, local_name, source_module, imported_name, kind)`**
   builtin, parsed off the import AST, so aliased symbols resolve WITHOUT scip.
   - Overlap note: `module_binding(file, local, source, dst)` shipped in v0.7.0
     covers aliased-import locals. Gap vs the ask: no `imported_name` column
     (the canonical exported name as written at the import site) and no `kind`
     (named | default | namespace | side-effect | re-export). Extend or twin it.

2. **Acceptance criterion for (1)**: that ONE relation resolves which library any
   local name binds to in a two-line join. If the join needs more than two lines,
   the shape is wrong.

3. **`template_parts(file, line, node, idx, kind, text)`** source op: splits a
   template literal into its static and interpolated pieces, ordered by `idx`,
   `kind` = static | expr. (Template-built import paths / URLs / keys become
   joinable.)

4. **Shallow single-function def-use**: `binds` + `flows_to` so
   `const p = X; f(p)` and `f(cond ? A : B)` resolve. This def-use pass is the
   ONLY thing that recovers value-computed call args.
   - Overlap note: the intra-proc `df_node`/`df_edge` lift exists (std/flow.dl
     unions it into `flow_edge`). The ask is the SHALLOW, cheap, always-on shape:
     a two-rel surface a mapping query can join without loading the whole
     dataflow family. Decide: thin views over df_* vs a dedicated light pass.

5. **`const_string_member(file, object, member, value)`**: any string-valued
   const object (lookup tables, route maps, key registries) discovered
   generically instead of hardcoding one object name per query.

6. **First-class `unresolved(reason)` marker**: a map must distinguish
   "no edge exists" from "edge exists but target computed at runtime".
   Reasons enumerable (dynamic-import, computed-member, template-target,
   spread-args, ...). Unions into the same graph the resolved edges live in.

7. **Reusable std lib emitting ONE unified edge relation** carrying `kind` plus
   optional metadata fields — the mapping consumer joins one rel, not five.
   (std/flow.dl's flow_edge union is the precedent; this generalizes it to the
   module/import/call/template layers.)

## Chris's own triage (keep this framing)

- Structural idioms captured by pattern matching need NO engine change and are
  buildable today as SG rules.
- Import binding, template parts, const string member, and the unresolved
  marker are all cheap AST-local built-ins.
- Interprocedural dataflow is the one frontier feature and deserves its own
  scoped track — do not let it ride along.

## Sequencing sketch (not committed to)

Cheap AST-local batch first (1, 3, 5, 6 — each is a TypeLang/extract-family
addition in the existing builtin-rel checklist shape), then the std unified-edge
lib (7) over what exists, then shallow def-use (4) as its own small arc, with
the two-line-join criterion (2) as the review gate for the whole batch.
