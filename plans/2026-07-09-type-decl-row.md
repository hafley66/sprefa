# type_decl_row — types derived from rels (Phase 5)

Branch `feat/type-decl-row` off `feat/type-shapes-prototype` (99d6877). Reserved builtin
sink `type_decl_row(shape, pos, col, ty)` that DERIVED rules head; the engine consumes it
across a one-tick phase delay to materialize `rel name: shape.` decls whose shape was
computed, not written by hand.

## 1. Type signatures

```rust
// src/engine/mod.rs — reserved sink registration (mirror GRAPH_RELS / DEMAND_RELS)
const TYPE_DECL_RELS: [&str; 1] = ["type_decl_row"];
fn type_decl_rel_decls() -> Vec<RelDecl>;          // one decl: (shape,pos,col,ty), group "types"
pub fn type_decl_row_used(prog: &Program) -> bool; // headed by any rule?

// persisted-shape store (meta table _shapes), read at declare / written at end of tick
fn load_persisted_shapes(&self) -> Result<HashMap<String, Vec<Col>>>;   // shape -> cols
fn persist_type_decl_shapes(&mut self, prog: &Program) -> Result<()>;    // digest-guarded

// declare-time resolution of a derived `rel name: shape.` (engine has _shapes; frontend does not)
//   fills self.rels[name] from _shapes, or records a shape-pending / shape-shadowed / unknown-ty diag
fn resolve_derived_shapes(&mut self, prog: &Program) -> Result<()>;

// engine structural diag buffer (like extraction_drops but ALSO surfaced at --check via diags())
shape_diags: Vec<DiagRow>,      // Engine field, cleared at tick start

// src/typecheck.rs — frontend expansion learns to DEFER an unresolved ref
pub fn expand_shapes(items: &mut Vec<Item>, dl_path: &str, diags: &mut Vec<TypeDiag>);
//   unchanged signature; behavior: an unresolved shape_ref stays Some(name) with cols empty
//   (deferred) WHEN the program heads type_decl_row; else the existing unknown-shape error.
//   Item::Shape decls are RETAINED (no longer dropped) so the engine sees syntax shape names.
```

## 2. Pseudo-code

```
// typecheck::expand_shapes
heads_type_decl = items.any(rule head rel == "type_decl_row")
for rel with shape_ref = Some(name):
    if syntax_shapes has name: rel.cols = cols; rel.shape_ref = None
    else if heads_type_decl:   leave shape_ref = Some(name), cols = []   // DEFERRED
    else:                      unknown-shape error (existing)
// keep Item::Shape items (drop the retain() call)

// Engine::tick_report, top: self.shape_diags.clear()
// declare_all (start of tick): after existing per-decl guards + declare loop,
//   call resolve_derived_shapes(prog):
persisted = load_persisted_shapes()          // from _shapes
syntax_shape_names = { s.name for Item::Shape in prog }
for rel with shape_ref = Some(name) still unresolved:
    if persisted has name:
        cols = persisted[name]
        declare(RelDecl{ name: rel.name, cols, ..})   // reuses PK-drift migration + _reldigest delete
    else:
        shape_diags.push(shape-pending: "`{name}` derives from type_decl_row; available next tick")
for name in persisted.keys():
    if syntax_shape_names has name:
        shape_diags.push(shape-shadowed: "syntax `type {name}` shadows the derived shape; derived rows ignored")

// persist_type_decl_shapes, END of tick (after post-stratum rebuild, before @next staging):
if !type_decl_row_used(prog): return
d = rel_content_digest("type_decl_row")
if load_rel_digest("shape:type_decl_row") == Some(d): return    // recompute guard
rows = SELECT shape,pos,col,ty FROM rel_type_decl_row ORDER BY shape,pos
validate each ty: Type::parse(ty).is_some() || builtin_enum_variants(ty) || ty in prog brands
    else shape_diags.push(shape-unknown-ty ...) and SKIP that shape (stays pending)
_shapes := full replace (DELETE then batched multi-VALUES insert; no per-row write)
save_rel_digest("shape:type_decl_row", d)

// Engine::diags() append self.shape_diags (respecting the `only` path filter) so --check + LSP see them
```

Column build from a persisted `(col, ty)`: `Type::parse(ty)` → base type, brand None;
else (validated brand name) → `Col{ ty: Type::Text, brand: Some(ty) }` (enum brands store TEXT;
int detection preserved via Type::parse). Limitation noted: a `<: int` brand shape column lands
TEXT, cosmetic only (rules over the derived rel typecheck at load when cols were empty).

## 3. Instance lifetimes

- `_shapes` (SQLite meta table): db-persistent, survives restart, migration-safe (CREATE IF NOT
  EXISTS in ensure_meta).
- `shape:type_decl_row` digest row in `_reldigest`: db-persistent; the recompute guard.
- `self.shape_diags: Vec<DiagRow>`: per-tick, cleared at tick start, read via `diags()` after tick.
- `self.rels[name]` for a resolved derived shape rel: rebuilt each tick by declare (idempotent);
  the underlying `rel_<name>` table persists + migrates on column drift.

## 4. Storage layout, reads, writes, uniqueness

- `_shapes(shape TEXT, pos INTEGER, col TEXT, ty TEXT, PRIMARY KEY(shape,pos))`.
- WRITE: end of tick, digest-guarded full replace of all derived shapes present in
  `rel_type_decl_row`. One batched insert (chunked multi-VALUES) in a transaction.
- READ: start of tick, `resolve_derived_shapes` loads the whole table into a map.
- Uniqueness: (shape,pos) PK; a shape's column order = type_decl_row.pos ascending.
- Phase delay: writer runs AFTER the derived fixpoint of tick N; reader runs at declare of
  tick N+1. Same-tick is a phase circularity (schema needed at declare, rows exist post-derive) —
  the @next carry precedent; the daemon reactive loop makes the delay invisible.

## 5. Diag codes (all non-error, so the PostToolUse error-gate stays green)

- `shape-pending` (info): a `rel name: shape.` whose shape has no syntax decl and no persisted
  rows yet — derives from type_decl_row, available next tick.
- `shape-shadowed` (warn): a syntax `type name(...)` and derived type_decl_row rows share a name;
  syntax wins, derived rows ignored.
- `shape-unknown-ty` (warn): a derived shape row names a ty that is neither a base type, an
  ambient builtin enum brand, nor a program brand; that shape stays pending.

## 6. Example (examples/type-from-json.dl)

- jsonp over a committed sample payload → intermediate rel `payload_col(pos,col,ty)`
  (int when every observed value parses as int, else text). Routed through its own rel FIRST
  (term-extract cannot co-head the derived sink — the repo mixed-kind law), then
  `type_decl_row("payload", pos, col, ty) <- payload_col(pos,col,ty).`
- mapped-type: `type_decl_row(concat("partial_", rel), pos, col, ty) <- rel_col(rel,pos,col,ty,_).`
  (concat + body-bind from Phase 4, committed at base head 99d6877).
- `rel payload_rel: payload.` becomes a live checked rel one tick later.

## 7. Tests

- Unit (typecheck): deferred shape_ref survives when type_decl_row headed; unknown-shape kept
  when not headed.
- Unit (engine): resolution order (syntax beats persisted → shadow diag), unknown-ty diag,
  digest no-op (same rows → no re-persist).
- e2e tests/it/type_decl_row.rs (in-process Engine, two explicit ticks like temporal_carry):
  pending on tick 1, usable rel on tick 2; shape row change migrates + re-derives; user
  `rel type_decl_row(...)` bails; json example end to end; mapped-type produces partial_* shapes.

## 8. Regens / deferrals

- README builtin-rels + docs/reference/*: regenerated block drift from the new decl — note in
  report, do not hand-edit the generated zones.
- Deferred: `<: int` brand base type in a derived shape (lands TEXT); orphan `_shapes` GC when a
  shape name stops being produced (harmless, overwritten on next full replace... actually full
  replace prunes it — no GC needed).
