---
name: project_cross_file_entity_graph
description: "real goal = cross-file entity/import graph (stack-graphs/SCIP style), NOT bulk tree-sitter type dump; demand-driven only; U1 GREEN on branch (not merged)"
metadata: 
  node_type: memory
  type: project
  originSessionId: 2be5eb40-4f91-46fe-84ea-a3848747273b
---

Correction 2026-05-18 that reframes [[project_dots_types_tsimport]].

**Not wanted:** importing all ~369 tree-sitter node kinds as declared
types. Violates the DSL pattern-matching ethos and "don't save more
data than I need." Eager `ensure_lang_imported` (declare every kind) is
the wrong shape. Keep the type-import capability, but **demand-driven
only**: a node kind becomes a type only when a sprf statement names it.
~369 → ~3.

**Actual north star:** a blunt algorithm to track entities across
files. A tree-sitter / stack-graphs / SCIP-style graph of stanzas
describing module systems, import/export, and module-resolution
algorithms, emulated so **type flows cross files** when they don't hit
macros. `syn` is an acceptable alternative source for Rust. The
tree-sitter type import is at most a small input to this, not the goal.

**Why:** user wants cross-file type flow / name binding, not a grammar
schema dump. Pattern-matching ethos: match what you need.

`tag` and `fact` ops are **deprecated**. Use `rule(...)` as the
sink/declare. See [[feedback_no_tag_fact_use_rule]]. Type work itself
must be value-space ([[project_types_in_value_space]]); the entity
graph is ORTHOGONAL and must not block on it.

## U1 SHIPPED (GREEN, branch `feat/rs-entity-graph`, NOT merged)

2026-05-19, worktree `/Users/chrishafley/projects/sprefa-entity`,
branch off main `9bd1fad6`. Commits: f4d7e2bd RED, e3bfa410+f4264eb2
plan doc (`v4/docs/v4-u1-entity-graph-plan.md`), **18c44b1a** the
impl. `rs_entity_extract_target` 4/4, full v4 gate **500/0/1**, no
regressions. AWAITS USER REVIEW (core-op change), not merged/pushed.

The gate that blocked everything: plain `ast(:rs)` op
(`AstNmComponent::render_batch`, v2_ops.rs) collected only
`nm.range()` and dropped the NodeMatch env, so NO pattern metavar
reached a cursor term. FIXED: `scan_ast_metavars` at lower →
one `bound: Arc<[Arc<str>]>` field (C1, no instance bloat) → at
render, per-match metavars extracted owned INSIDE the find_all
closure (Node borrows grep), set as terms AND `set_at` with the
metavar node's OWN coord (C3 producer half done; `resolve_dot`
`NAME.lo/.hi/.fs` arm still TODO). Mirrors AstYamlComponent's
existing loop; AstYaml untouched. This is the separable additive
half of parked Task #4 — nothing to do with the vetoed node-types
importer.

Spine = def/ref/import EDGE rows (not bulk type import), Rust-first,
demand-driven. Authoring laws learned:
- defs need `ast_yaml(:rs)`kind: function_item\nhas:{field:name,
  pattern: $NAME}`` — plain `ast(:rs)`fn $NAME`` does NOT match
  through `pub`/`async`/etc. modifiers. refs/imports = `ast`
  patterns are fine.
- shared-term join across two reads = BIND `N?` on the first, REF
  `N` (no `?`) on the second. Binding `N?` twice = cross product.
- transitive closure over fs-fed sources = the known
  recursion-over-fs refresh limit ([[project_recursion_surface_gaps]]),
  deliberately deferred; one-hop name-equality is the tier-2
  contract. `dep_cycle` uses the shipped intra-row `?`-then-ref
  ([[reference_qmark_then_ref_intra_row]]).

## REIFY MACRO SHIPPED on branch (3 stones GREEN, gate 503/0/1)

2026-05-19, same branch `feat/rs-entity-graph` (NOT merged). User
redirected: do NOT mutate ast/ast-grep (U1 metavar `18c44b1a` is now
DEAD, superseded). New mechanism = a separate `reify` macro op.
Commits: `8d217be1` RED, `b2c83b3b` stone1 crate, `952d7832`
stones2+3. `reify_struct_smoke_target` 3/3, emit 2/2, crate 2/2.

- Stone 1 `sprefa-extract` crate (`v4/crates/sprefa-extract`,
  path-dep): tree-sitter ONLY (no syn, no compilers, kept SEPARATE
  per user — "we do lots with it like sem"). `LangExtract` trait +
  `TyEntity{name,kind,fields:[TyField{name,TyRef::Prim|Named}]}`.
  Knows langs, not sprf.
- Stone 2 `emit_sprf` (v4 `src/reify.rs`): TyEntity → a VALID
  structural decl `rule(:Point, x?, y?);  # reify types: x: t.i64,
  y: t.i64`. KEY DEFERRAL: sprf rule-decl has NO `col?: type`
  grammar (`x?: t.i64` misparses as kwarg → unknown-op `t.i64`).
  Types kept as a LOSSLESS trailing comment = the seam where typed
  cols slot in once value-space-types lands. reify must NOT block on
  it.
- Stone 3 pre-pass (`app.rs` run(), between read_to_string and
  host_parse): finds first `reify(` stmt, runs sprefa-extract over
  root, rewrites a `#<reify gen h=HASH>`..`#</reify gen>` managed
  region right after the stmt, then the UNMODIFIED
  parse→walk→run pipeline executes it (true macro). Idempotent by
  body hash (byte-current ⇒ no rewrite). `ReifyDef` op inert at
  lower. No-reify programs early-return untouched (zero regress).

WHY `t.i64` doesn't work yet (asked, answered): 3 stacked unbuilt
pieces — (1) `ValueKind={Atom,Pipe}` no callable/type variant;
callables in a side registry, not value space → can't dot a
non-value; (2) `resolve_dot` (ctx.rs:272) only does instance-dot +
type-column-of-declared-rule, no namespace/callable-projection arm;
(3) decl has no typed-col slot (kwarg clash). Ordered fix:
callable-Value → [[project_types_in_value_space]] → resolve_dot 3rd
arm → decl typed-col grammar. Deferred by design, not a bug; the
reify comment is the seam.

SCIP question (asked): the IR is NOT SCIP stanzas. SCIP = an
occurrence/ref index keyed by a symbol STRING; it has no structured
type/field model (same gap as sem-core — exactly why TyEntity
exists). Adopt only SCIP's symbol-descriptor STRING as the
cross-repo identity key at U2, synthesized from CST, no indexers.
TyEntity stays the structured-type IR; edges become SCIP-occurrence
shaped; the descriptor string is the U2 join key.

NEXT: resolve_dot where-bytes arm (NAME.lo/.hi/.fs), U2 SCIP
descriptor key, U4 tier-3 scope/stitch; widen reify beyond Rust
struct (enum/trait/imports → the def/ref/import edge spine).
`ast.rs` namespacing + typed cols blocked on callable-Value
(separate plan). ts-import stays parked.
