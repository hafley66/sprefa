# `reify` — TS→IR→sprf macro op (design for review)

New op. Does NOT touch `ast`/`ast-grep` (the U1 metavar branch is
superseded, unmerged). Mirrors `~/projects/ext/sem`'s TS→IR shape,
emits generated sprf into an idempotent in-file region, expanded+run
as a true macro.

## The type problem, solved

```
 sprf value model:  ValueKind = { Atom, Pipe }      (no new kind)

 a TYPE = a PIPE that validates/coerces the cursor.
   t.i64   builtin pipe: parse cursor.value as i64; emit|drop
   t.u64   "
   t.A     the generated rule(:A,…) used as structural validator
           (its columns ARE the fields)
   t       a namespace value: DotTable of builtin type-pipes

 reflection:  type-pipe.dots = { name, fields, kind }
              → A.fields / A._kind resolve (DotTable already landed)
 comptime:    resolve_dot-at-lower on literal args ALREADY EXISTS
              → x?: t.i64 resolves the type-pipe at lower; the rule
                decl schema records the type-pipe per column.

 "types not being cursor string" — dissolves: a type never IS a
 string, it ACTS on one. Lattice (bytes⊑string⊑tokens⊑tree) is
 orthogonal value-level metadata.
```

## sem reuse boundary

```
 REUSE (sem-core is a lib):
   SemanticParserPlugin::extract_entities(content,path)->Vec<SemanticEntity>
   per-lang plugins (code/, json, yaml, toml, vue, …)
   structural_hash (Unison-style)  → idempotent-regen key
   SQLite entity cache             → only re-extract on change
   EntityRef { from,to, Calls|TypeRef|Imports }  → def/ref/import edges free

 GAP (must add, thin):
   SemanticEntity is COARSE: raw `content` + hashes, NO field/type list.
   struct A { x:i64; y:u64 }  →  needs a per-lang FIELD re-parse of the
   entity's node: Vec<(field_name, type_text)>. A small layer ON the
   sem entity, not a new graph.
```

## Op design (planning protocol)

### Layer 1 — signatures

```rust
// surface: placed after a rule, or chained. lang atom + selector.
//   reify(:rs)`kind: struct_item`         (declares + emits region)
struct ReifyDef;                       // OperatorDef

struct TyRef;                          // Prim(Arc<str>) | Named(Arc<str>) | App(Arc<str>, Vec<TyRef>)
struct TyField { name: Arc<str>, ty: TyRef }
struct TyEntity { name: Arc<str>, kind: Arc<str>, fields: Vec<TyField>,
                  edges: Vec<(Arc<str>, RefKind)> }

fn extract_ty(lang, files) -> Vec<TyEntity>;     // sem plugin + field re-parse
fn emit_sprf(&TyEntity) -> String;               // → `rule(:A, x?: t.i64, …);`
fn region_hash(&[TyEntity]) -> Hash;             // structural, idempotency key
fn rewrite_region(file, site, hash, body) -> Edited;  // managed markers
```

### Layer 2 — pseudo bodies

```
 ReifyDef::lower(ctx, dsl):
   record (invoke_site_byte, lang, selector) on ctx        // site = where region goes

 run pass (file-rewriting):
   ents = extract_ty(lang, target_files)        // sem cache keyed by structural_hash
   body = ents.map(emit_sprf).join("\n")
   h    = region_hash(ents)
   rewrite_region(file, site, h, body):
     find  «⟦reify:rs:<oldh>⟧ … ⟦/reify⟧» right after site
     if oldh == h: no-op (idempotent)
     else: replace region (or insert if absent), markers carry h
   reparse file → the region IS real sprf → splice + execute (macro)
```

### Layer 3 — lifetimes

```
 sem entity cache : SQLite, file→entities keyed by structural_hash;
                    re-extract ONLY on hash drift.
 generated region : DERIVED, never hand-edited. The IR is truth.
                    a manual edit changes nothing — next run the hash
                    guard regenerates over it.
 type-pipe        : builtin t.* = process-static pipes; t.A = the
                    live rule A (lifetime = the program).
```

### Layer 4 — storage / sequence / uniqueness

```
 one region per reify invoke-site, keyed (selector + site id).
 uniqueness  : same IR ⇒ byte-identical region ⇒ no-op.
 regen trigger: structural_hash drift ONLY (cheap, sem owns the hash).
 ordering    : region must lower BEFORE its generated rules are read;
               site is immediately AFTER the reify statement so the
               file’s own statement order gives it.
```

## Decisions still open (for you)

```
 1 op name:        reify | tsgen | astgen | downcvt
 2 regen trigger:  every run  vs  structural-hash-cached  (rec: cached)
 3 region commit:  checked-in generated sprf (diffable, marker) vs
                   gitignored/regenerated  (rec: committed + marker)
 4 type-pipe v0:   validate-only (drop on fail) | coerce (parse+rewrite
                   cursor value) | annotate-only (schema meta, no runtime)
 5 field re-parse: own tiny per-lang fns vs widen sem SemanticEntity
                   (sem is vendored ext/, prefer our thin layer)
```

## Not in this op (kept separate)

- `ast`/ast-grep stays untouched; U1 metavar branch abandoned.
- callable-Value (for `ast.rs`-style namespacing) — separate plan.
- tier-3 scope/stitch, SCIP key — later units, unaffected.
