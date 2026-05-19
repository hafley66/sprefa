# Type-IR in value space

Status: DRAFT 2026-05-19. Forks #1/#2/#3 from chat_log/20260518.0 picked by
me under value-space + callable-Value + cons-D-TY constraints. User may
veto any pick; rationale shown inline (search `RATIONALE`). Worktree
`/Users/chrishafley/projects/sprefa-types`, branch `feat/type-ir-value-space`,
base main 9bd1fad6.

## Inherited constraints (locked, NOT refightable)

| | source | constraint |
|---|---|---|
| value-space | memory `project_types_in_value_space` (2026-05-19) | a type IS a Value. `DotTable.ty: Arc<str>` was the wrong direction. types live in VALUE space, dot/pass/reflect on them at comptime |
| no ts_import | same | bulk tree-sitter node-types importer banned; ask before reviving `ts_import.rs` |
| callable-Value precedent | memory `project_callable_value` (GREEN unmerged) | `Callable(CallableRef{kind,name})` is the shape for "a name-handle that resolves through registry"; `resolve_dot` on `Callable(Rule)` projects against declared cols (callable-value H4) |
| cons-D-TY mount | memory `project_cons_calling_unification` step 5 | `Cons{key, value, ty: Option<Value>}` — a cell value carries a type Value |
| no resolution graph | chat_log/20260518.0 §"Open Questions" | use/ref/call resolution stays OUT; type-IR is nominal + structural + implements edges only |

## D-1 — types ARE rules; no new ValueKind variant **(RATIONALE PICK)**

Memory `project_types_in_value_space` says "types ARE Pipe Validators; types
= predicates = constraints." A predicate is a callable. The dots/types/nesting
C-spine plan (`/Users/chrishafley/.claude/plans/rustling-questing-falcon.md`)
already says "type = a rule whose columns are its fields." Callable-Value
(c79c47f8) already gives every rule a Value-handle: `Callable(CallableRef{
kind: CallableKind::Rule, name: "Foo".into() })`. Its `resolve_dot` arm
(callable-value H4) already projects that handle against `declared_cols("Foo")`.

So: a type Foo IS the rule `rule(:Foo, F1?, F2?, ...)`. Its value-space
addressable handle IS `Callable(Rule "Foo")`. Dotting it (field access) IS
the existing callable-Value H4 arm. Apply it (membership / predicate
check) IS `apply(&Callable(Rule "Foo"), args)` from callable-Value's `apply`
fn. **Zero new ValueKind variant.** Zero new resolve_dot path. Reuse
everything.

RATIONALE for picking this over a fresh `ValueKind::Type(TypeRef)`:

- callable-Value already shipped (unmerged but green). Adding `Type` would
  duplicate the name-handle + registry-lookup machinery
- a generic type `Vec(T?)` is naturally a rule with a type-param column;
  `Vec<i64>` = `Callable(Rule "Vec")` applied with `t=:i64`; symmetry with
  function application is free
- variants (sum types) desugar to cons-step-6 `{ }`-as-merge fan-out:
  `Foo = Bar | Baz` ⇒ `Foo { Bar; Baz }` lowers to a merge op over two
  callable-rule handles
- self-dogfood: sprefa's own type-rules (the autodoc north star) are
  already-rules-already; no impedance mismatch

Veto path: if user wants `Type` as a distinct ValueKind for tagging /
serialization / future divergence, the impl cost is mechanical — same
fanout pattern callable-Value paid (78 ValueKind:: sites per H6).

## D-2 — tref = structured rows **(FORK #2 PICK: a)**

Reject the flat-string form. A type-reference `Vec<HashMap<K,V>>` lowers to
a tree of tref rows:

```
tref(0, head: Callable(Rule "Vec"),     args: [1])
tref(1, head: Callable(Rule "HashMap"), args: [2, 3])
tref(2, head: Callable(Rule "K"),       args: [])
tref(3, head: Callable(Rule "V"),       args: [])
```

`head` is a Callable Value (D-1). `args` is a list of REF_ID (Index keys
in cons terms). Self-keyed by REF_ID enables "every type mentioning Vec" =
`tref(_, head: =:Vec, _)`. Marries the value-space anchor (head is a
Value) with relational queryability (rows you can JOIN and antijoin).

RATIONALE: cons-plan already gives us ConsList; tref's args slot IS just
a ConsList of REF_ID. Storage-wise this is a single table with a
ConsList column. Once cons step 4 lands (cons/merge ops), tref rows can
be written in sprf surface as `tref(0, :Vec, (1))`.

## D-3 — drop-list to meta **(FORK #3 PICK: revised)**

Confirm-with-revision the chat_log/20260518.0 drop-list:

| feature | meta? | reason |
|---|---|---|
| visibility (`pub`, `private`) | yes | language-idiosyncratic; query rarely needs it cross-language |
| lifetimes (Rust `'a`) | yes | Rust-only; not part of nominal/structural identity |
| decorators / attributes | yes | open-set, language-specific |
| conditional / mapped TS types | yes | computed types; lose them as structured, keep raw text |
| macro-gen types | yes | post-expansion artifact; meta captures origin macro |
| **generic bounds (trait constraints)** | **NO** | promote to first-class. A `T: Iterator` bound IS a Callable(Rule "Iterator") reference. Same machinery as tref. Lose query power if demoted ("every type bounded by Iterator") |

Net schema (DRAFT — see D-1 for the value-space anchor):

```
ty(PATH:str, KIND:atom, NAME:str)            -- one row per type-rule decl
fld(OWNER:str, NAME:str, TY_REF:Int, ORD:Int)  -- TY_REF -> tref.REF_ID
tref(REF_ID:Int, HEAD:Callable, ARGS:ConsList) -- structured ref tree
bound(TY_PARAM:str, PRED:Callable)            -- generic bound = pred-ref
meta(PATH:str, KEY:str, VAL:str)              -- drop-list
```

## Type signatures (layer 1)

```rust
// v4/src/compile/lower/type_ir.rs  (new module)

/// A foreign type extracted into the IR. Not a Value — this is the
/// extractor's intermediate. The Value-space anchor is Callable(Rule "Foo")
/// emitted by the macro (D-1).
pub struct TypeDef {
    pub path: Arc<str>,                  // ScopePath::join (IR-C)
    pub kind: TypeKind,                  // IR-B discriminant
    pub fields: Vec<FieldDef>,           // record/variant payload
    pub bounds: Vec<BoundDef>,           // generic bounds (D-3 promote)
    pub meta:   Vec<(Arc<str>, Arc<str>)>, // drop-list
    pub structural_hash: blake3::Hash,   // for antijoin drift detection
}

pub enum TypeKind { Record, Variant, Func, Alias, Prim }

pub struct FieldDef {
    pub name: Arc<str>,
    pub ord:  u32,
    pub ty_ref: TyRef,                   // tree, not string (chat_log/20260518.0)
}

pub enum TyRef {
    Nominal { head_path: Arc<str>, args: Vec<TyRef> },
    Prim(Arc<str>),
    Tuple(Vec<TyRef>),
    Unknown(Arc<str>),                   // raw token tree, accepted lossy
}

pub struct BoundDef {
    pub ty_param: Arc<str>,
    pub pred_path: Arc<str>,             // resolves to Callable(Rule pred_path)
}

/// Per-language extractor. Mirrors sem-core's LanguageConfig pattern.
/// ~20 declarative lines per language per chat_log/20260518.0 §80/20 algo.
pub trait TypeMap: Send + Sync {
    fn lang_id(&self) -> &'static str;
    fn record_nodes(&self)  -> &'static [&'static str];
    fn variant_nodes(&self) -> &'static [&'static str];
    fn func_nodes(&self)    -> &'static [&'static str];
    fn ref_node(&self)      -> &'static str;
    fn member_field(&self)  -> &'static str;
    fn extract(&self, cst: &tree_sitter::Tree, src: &str) -> Vec<TypeDef>;
}
```

## Body (pseudo, layer 2)

```
// the macro: surface = `use(:Rust, "src/lib.rs")` ⇒ lower-time CST splice
fn lower_use(ctx, lang_atom, src_path) -> Result<Pipe<Cursor>, LowerError> {
    let lang = ctx.registry.type_map(lang_atom)?;        // TypeMap lookup
    let src  = read_to_string(src_path)?;
    let cst  = parse_for(lang.lang_id(), &src)?;          // tree-sitter
    let defs = lang.extract(&cst, &src);                  // Vec<TypeDef>

    // for each TypeDef: emit a synthetic rule decl + populate rows
    let mut pipe = Pipe::new();
    for td in defs {
        let rule = synth_rule_from_typedef(&td)?;         // rule(:td.path, F?...)
        ctx.register_rule(rule.clone());
        pipe = pipe.step(emit_ty_row(&td));               // ty(...) row
        for f in &td.fields {
            let ref_id = pipe.intern_tref(&f.ty_ref);      // recursive intern
            pipe = pipe.step(emit_fld_row(&td, f, ref_id));
        }
        for b in &td.bounds {
            // BOUND = Callable(Rule b.pred_path) — D-1
            let pred = Value::callable(CallableKind::Rule, b.pred_path.clone());
            pipe = pipe.step(emit_bound_row(&td, b, pred));
        }
        for (k,v) in &td.meta { pipe = pipe.step(emit_meta_row(&td, k, v)); }
    }
    Ok(pipe)
}

fn intern_tref(&mut self, t: &TyRef) -> u32 {
    // depth-first walk; head = Callable(Rule head_path), args = recurse
    // returns REF_ID stable per (head_path, args) tuple (interned)
}
```

## Instance lifetimes (layer 3)

| Type | Lifetime |
|---|---|
| `TypeDef` | per-`use`-call; consumed by `lower_use`, dropped after rows emitted |
| `TypeMap` impl | `'static`, registered once at startup like `OperatorDef` |
| synthetic `Rule` (`Callable(Rule path)` target) | lives in `LowerCtx.rules` for compile duration; emitted-table rows persist via FactStore |
| `tref` rows | persist in FactStore; addressed by `REF_ID` |
| `structural_hash` | computed at extract; antijoin'd at re-extract for drift |

## Storage / reads / writes / uniqueness (layer 4)

- **Storage**: 5 new tables (ty, fld, tref, bound, meta), all inside the
  existing FactStore. No FactStore changes.
- **Reads**: macro emits rows + rules at lower; user queries via plain
  rule reads — `Foo.field` IS callable-Value H4 dot. "every type mentioning
  Vec" = `tref(_, head: Callable(Rule "Vec"), _)` join.
- **Writes**: only `lower_use`. Re-running `use` retracts old rows for the
  same `src_path` (existing recursion-surface retraction handles it).
- **Uniqueness**: `ty._id = blake3(path)`; `tref._id = blake3((head_path,
  args_ref_ids))` for interning; `fld._id = blake3((owner, name))`.

## Build order

| step | what | gate |
|---|---|---|
| 0 | confirm D-1: no new ValueKind variant — callable-Value's Callable(Rule) IS the type anchor. Requires feat/callable-value merged (memory task #16). | docs only |
| 1 | `type_ir.rs` skeleton: `TypeDef` / `TyRef` / `TypeMap` trait + tests over a hand-built `TypeDef` (no extractor yet) | unit tests on type_ir.rs |
| 2 | rustdoc-JSON adapter: `RustdocTypeMap` impl, fed from `cargo rustdoc --output-format json` over a fixture crate. Confirms the chat_log/20260518.0 self-dogfood pivot. | RED: extract + assert 5 rows for a 5-type fixture |
| 3 | `lower_use` op: lower-time CST splice that emits the 5 tables + synthesizes rule decls | RED: `use(:Rust, "fixture.rs")` then query `MyStruct.field` |
| 4 | tref intern + structured-row emission; verify "every type mentioning Vec" query | RED: tref reachability over a Vec-heavy fixture |
| 5 | bound rows + Callable-handle linkage (D-3 promote); verify "every type bounded by Iterator" | RED |
| 6 | meta drop-list catch-all; verify visibility/lifetimes round-trip via meta only | RED |
| 7 | structural_hash + antijoin drift: regenerated-vs-on-disk; flag a deliberately stale field | RED (the chat_log/20260518.0 north-star autodoc test) |
| 8 | integration with cons D-TY (cons step 5): a cell can carry `ty: Callable(Rule "Foo")` value; verify lower-time `resolve_dot` on a typed cell projects via the type-rule | RED, after cons step 4 merges |

Steps 1-7 are independent of cons-plan progress. Step 8 is the marry-up
point; runs after cons step 4 + 5.

## Worms — defused vs still-open

| worm | state |
|---|---|
| new ValueKind variant fanout (a la callable-Value H6) | defused: D-1 reuses Callable, zero new variant |
| resolve_dot duplication for Type | defused: D-1 reuses callable-Value H4 arm |
| generic application (Vec<i64>) needs new machinery | defused: reuse rule apply (callable-Value Rule arm) with t=:i64 positional |
| variant / sum types need a new construct | defused: cons step 6 merge-as-{} IS variant-as-sum |
| extractor maintenance burden per language | bounded: ~20 declarative lines per lang per chat_log/20260518.0 §80/20 |
| resolution / use-graph wall | accepted lossy per chat_log/20260518.0; meta captures raw token tree for Unknown refs |
| tref intern across compilation runs | OPEN: REF_ID stability across re-ingest. Either re-intern from scratch each `use` call (simple, loses cross-call query identity) or persistent intern table (complicates retraction). DRAFT pick = re-intern each call; OPEN for review |
| macro hygiene: `use(:Rust, "a.rs")` emits `rule(:Foo, ...)`; collision with user's existing `Foo` | OPEN: namespace the synthesized rule name by src_path? Or accept user owns the namespace? DRAFT pick = namespace by src_path, e.g. `:a_rs::Foo`; OPEN for review |
| meta drop-list catch-all is unbounded | bounded: schema is `(key, val)` string pairs; analyses can ignore unknown keys |

## Relation to other tracks

- **callable-Value** (feat/callable-value c79c47f8, unmerged): PRECONDITION for D-1. Step 0 cannot proceed without it merged.
- **cons-calling-unification** (BUILDABLE, no branch): cons step 5 D-TY consumes this work; step 8 here is the marry-up. Steps 1-7 here are independent.
- **cross-file entity graph** ([[project_cross_file_entity_graph]]): structurally similar (tref ≈ ref-edge, ty ≈ def-row). Different concern (types vs entity reachability); same row-shape style. Do NOT unify upfront; let usage drive convergence.
- **host-LSP plan** (under feedback round): orthogonal. Type values gain dot-into / hover via the same per-concept HostLspNode mechanism whenever that plan lands; this plan does not need it.

## Open questions for user (DRAFT picks above; veto path noted)

1. D-1: keep types-as-rules with Callable(Rule) anchor, or split off a new `ValueKind::Type(TypeRef)`? (pick: reuse)
2. D-2: structured tref rows (queryable) vs flat string (cheap)? (pick: structured)
3. D-3: promote generic bounds to first-class via Callable refs, or demote to meta? (pick: promote)
4. tref REF_ID stability across re-ingest: re-intern fresh each call vs persistent table?
5. macro hygiene: namespace synthesized rule names by src_path (`:a_rs::Foo`) vs flat (`:Foo`)?
6. first-language adapter: rustdoc-JSON (chat_log/20260518.0 pivot) vs JSON Schema + quicktype (polyglot, lossy)? (DRAFT pick = rustdoc-JSON for self-dogfood; rust-first matches the entity-graph crate's direction)

## Critical files (when impl starts)

- new: `v4/src/compile/lower/type_ir.rs` (TypeDef/TyRef/TypeMap)
- new: `v4/src/compile/lower/type_extract/rustdoc.rs` (first adapter)
- `v4/src/compile/lower/ops.rs` (~where `rule(:F, ...)` lowers): add `use` op definition
- `v4/src/compile/lower/registry.rs` (~where ops register): register `use` op + `type_map(lang_atom)` lookup
- `v4/src/compile/lower/value.rs` (NO changes — D-1 reuses Callable)
- `v4/src/compile/lower/ctx.rs` (~where `register_rule` lives): no signature change, just call from `lower_use`

## Verification

- per-step RED tests as listed (steps 2-8)
- autodoc dogfood (step 7) is the north-star: extract sprefa's own types,
  diff against on-disk source, flag a deliberately stale field. Same
  shape as the dots/types/nesting C-spine plan §Verification autodoc test.
- run on a small Rust fixture crate first (5 types), then a larger one
  (the v4 crate itself once stable).
