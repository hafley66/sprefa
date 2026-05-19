# Host-language LSP trait architecture (sprefa v4)

Status: DESIGN ONLY 2026-05-19 — not coded, not RED. Companion to
`plans/2026-05-19-cons-calling-unification.md` (cons-plan, parallel
track). Mirrors `OperatorDef` / `Component` (a static spec trait + a
runtime instance trait) as the two layers; every host concept that
wants hover / completion / goto-def / semantic-tokens / definition
participates by implementing one of the two traits. No
LSP-named branches sprinkled across `parse.rs` / `walk.rs` /
`lower/ctx.rs`. Where today's code has such a branch
(`lsp_locate_dsl`, `walk_pipe_for_dsl`, `walk_pipe_for_op` in
`v4/src/app.rs:2466-2500`), the plan moves it onto a trait method.

## Analogy to `OperatorDef` / `Component`

`OperatorDef` (`v4/src/compile/lower/op_def.rs:165`) is a static,
sprf-blind spec of an op's slots + lower behavior, dispatched through
`Registry` (`v4/src/compile/lower/registry.rs:12`); `Component`
(`v3/crates/effect_runtime/src/v2/component.rs:49`) is the instance
that runs per pipe at render time. **`HostLspDef`** is the static
spec for one host concept (rule decl, term ref, dot projection, atom,
…) — what kind it is, what semantic-token type it lowers to, what
hover/completion text it produces from a small ID-shaped descriptor;
**`HostLspNode`** is the per-document instance the walker stamps with
exact byte ranges + descriptor at lower time. The lifetime split
mirrors the op pair: `HostLspDef` impls are `Arc`-shared
`'static` singletons in a `LspRegistry`; `HostLspNode` values are
constructed per ingest, live as long as the `DocState`, and are
re-built each `lsp_open`/`lsp_change`. Routing
`(uri, byte) → &dyn HostLspDef + HostLspNode` is the LSP analog of
`Registry::lower_at` routing `(name, args) → Pipe<Cursor>`.

## Layer 1 — type signatures (Rust, pseudo)

```rust
// v4/src/cst/lsp/host.rs  (new module under existing v4/src/cst/lsp/)

use std::ops::Range;
use std::sync::Arc;
use effect_runtime::v2::ByteRange;
use lsp_types::{
    CompletionItem, Hover, Range as LspRange, SemanticTokenModifier,
    SemanticTokenType,
};
use crate::cst::diag::Diag;
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};

// ── Layer A — static def (mirror of OperatorDef) ─────────────────────

/// Stable kind tag for one host concept. One impl per concept;
/// registered into `LspRegistry` at startup. `'static`, send/sync.
pub trait HostLspDef: Send + Sync + 'static {
    /// Stable identifier; routes from `HostLspNode.kind_id` back to
    /// this def. e.g. "rule_decl", "term_ref", "dot_proj", "atom_lit",
    /// "kwarg_cell", "typed_col_cell", "callable_value", "reify_region",
    /// "type_anno_pipe", "intra_row_self_eq", "scope_open", "cons_cell",
    /// "merge_block". Mirrors `OperatorDef::name`.
    fn kind_id(&self) -> &'static str;

    /// Semantic-token classification — the only place a kind decides
    /// what color it gets. Returns `None` to skip emission.
    /// Mirror of `OperatorDef::paren_args` (static spec).
    fn semantic_token(&self) -> Option<(SemanticTokenType, &'static [SemanticTokenModifier])> {
        None
    }

    /// Hover text for a node of this kind. The walker stuffs the
    /// concept-specific descriptor into `node.payload` (opaque
    /// `Arc<dyn Any + Send + Sync>`); this method downcasts it. Mirror
    /// of `OperatorDef::lower` (the bytes-to-Pipe transform) — same
    /// shape, different output.
    fn hover(&self, ctx: &LspQueryCtx<'_>, node: &HostLspNode) -> Option<Hover> {
        let _ = (ctx, node);
        None
    }

    /// Completion list at `node`'s position. `prefix_byte` is the
    /// caret offset within the node; the def decides what to surface.
    fn completions(&self, ctx: &LspQueryCtx<'_>, node: &HostLspNode, prefix_byte: usize)
        -> Vec<CompletionItem>
    {
        let _ = (ctx, node, prefix_byte);
        Vec::new()
    }

    /// Goto-definition. Resolves to a host byte range. `None` = no
    /// target. Sites that resolve into a DSL body return the BODY-LOCAL
    /// range pre-shifted by `LspQueryCtx::shift_to_host`.
    fn goto_def(&self, ctx: &LspQueryCtx<'_>, node: &HostLspNode) -> Option<LspRange> {
        let _ = (ctx, node);
        None
    }

    /// All host byte ranges that reference the same identity as this
    /// node. Default empty. e.g. "every read of TERM in this rule's
    /// scope".
    fn references(&self, ctx: &LspQueryCtx<'_>, node: &HostLspNode) -> Vec<LspRange> {
        let _ = (ctx, node);
        Vec::new()
    }

    /// Optional diag enrichment hook — diags created elsewhere (parse,
    /// walk, runtime) can be re-coded / re-spanned by the concept that
    /// owns the offending range. Default = passthrough.
    fn enrich_diag(&self, ctx: &LspQueryCtx<'_>, node: &HostLspNode, d: Diag) -> Diag {
        let _ = (ctx, node);
        d
    }
}

// ── Layer B — runtime node (mirror of Component) ─────────────────────

/// Per-document, per-concept-occurrence record. Built by the walker
/// at lower time and stored in `DocState.lsp_index`. ID-shaped, NOT
/// closure-shaped — analysis must remain data so the LSP code stays
/// free of execution closures (mirror of Component's `describe()` →
/// `&dyn Any` recommendation, v3/crates/effect_runtime/src/v2/component.rs:152).
#[derive(Clone)]
pub struct HostLspNode {
    /// Routes back to a `HostLspDef` in the `LspRegistry`. Equal to
    /// the def's `kind_id()`.
    pub kind_id: &'static str,
    /// Source byte range. The ONE source of truth for hit-testing.
    pub range: ByteRange,
    /// Opaque per-concept descriptor. Each `HostLspDef` impl knows
    /// what concrete type to downcast it to. Mirrors saga effect
    /// descriptors / react `_debugHookType`.
    pub payload: Arc<dyn std::any::Any + Send + Sync>,
    /// Scope chain at this node, deepest-first. e.g.
    /// ["Module", "rule:outer", "rule:inner"]. The walker writes this
    /// from `LowerCtx::scope_path` at the moment the node is stamped.
    /// Used by `references()` to bound the search.
    pub scope: Arc<[Arc<str>]>,
    /// Children (for nested-rule scopes, dotted heads `x.a.b.c`, cons
    /// cells nested under cons-of-cons). Optional; flat list works
    /// for the common case.
    pub children: Vec<HostLspNode>,
}

// ── Registry + resolver (mirror of `Registry` + `lower_at`) ──────────

/// Static map kind_id → HostLspDef. Built once at startup, shared
/// `Arc` clone, never mutated after `register_*` returns. Mirror of
/// `compile::lower::registry::Registry`.
pub struct LspRegistry {
    map: std::collections::HashMap<&'static str, Arc<dyn HostLspDef>>,
    // optional: subsume DslBodyLsp here so the DSL body trait is reached
    // through the same registry. See section 7.
    dsl_lsp: std::collections::HashMap<&'static str, Arc<dyn DslBodyLsp>>,
}

impl LspRegistry {
    pub fn register(&mut self, def: Arc<dyn HostLspDef>);
    pub fn register_dsl(&mut self, op_name: &'static str, lsp: Arc<dyn DslBodyLsp>);
    pub fn get(&self, kind_id: &str) -> Option<Arc<dyn HostLspDef>>;
    pub fn dsl_for(&self, op_name: &str) -> Option<Arc<dyn DslBodyLsp>>;
    pub fn kinds(&self) -> Vec<&'static str>;
}

/// Per-document index built each ingest. Maps a byte position to the
/// deepest containing HostLspNode. Stored on DocState.
pub struct LspIndex {
    /// Flat list of every node ever stamped in this doc, source-order.
    /// Storage is a Vec, queries use binary search on sorted-by-lo
    /// + linear within overlapping window. (Interval tree later if
    /// the linear pass shows up in profiles; section 4 covers.)
    nodes: Vec<HostLspNode>,
    /// For DSL body containment: every op call that owns a DSL body,
    /// mapped to op_name + body byte range. The byte-resolver descends
    /// into this for `dsl_lsp` dispatch. Direct port of today's
    /// `walk_pipe_for_dsl` (v4/src/app.rs:2466).
    dsl_bodies: Vec<DslBodyHit>,
}

#[derive(Clone)]
pub struct DslBodyHit {
    pub op_name: Arc<str>,
    pub host_off: u32,
    pub raw: Arc<str>,
}

/// Query context handed to `HostLspDef` methods. Carries the per-doc
/// state plus the registry + facts handle so a def can resolve names
/// the same way the walker would (`LowerCtx::get_rule`,
/// `FactStore::declared_cols`).
pub struct LspQueryCtx<'a> {
    pub doc_text: &'a str,
    pub uri: &'a str,
    pub index: &'a LspIndex,
    pub registry: &'a LspRegistry,
    pub facts: &'a dyn effect_runtime::v2::FactStore<crate::Cursor>,
    pub sprf_store: Option<&'a Arc<crate::store::SprfStore>>,
}

impl<'a> LspQueryCtx<'a> {
    pub fn shift_to_host(&self, host_off: u32, body_range: Range<usize>) -> LspRange;
    pub fn resolve_kind(&self, node: &HostLspNode) -> Option<Arc<dyn HostLspDef>>;
}

// ── Top-level resolution API (the only thing handlers call) ──────────

/// Resolve the deepest host concept at `byte` (or the DSL body that
/// contains it). Used by `lsp_hover`, `lsp_completion`, `lsp_definition`.
pub fn resolve_at(ctx: &LspQueryCtx<'_>, byte: usize) -> ResolveHit<'_>;

pub enum ResolveHit<'a> {
    Host { def: Arc<dyn HostLspDef>, node: &'a HostLspNode },
    Dsl  { lsp: Arc<dyn DslBodyLsp>,  hit: &'a DslBodyHit, body_byte: usize },
    None,
}

/// Emit semantic tokens for the whole doc. One linear pass over the
/// index; one Vec<SemanticToken> result; sprefa-lsp encodes into LSP
/// delta-form. Today's `semantic::legend_token_types` (in
/// `v4/crates/sprefa-lsp/src/semantic.rs`) keeps its legend role.
pub fn semantic_tokens(ctx: &LspQueryCtx<'_>) -> Vec<SemanticToken>;
```

## Layer 2 — pseudo-code bodies

```text
resolve_at(ctx, byte):
    # 1. DSL body wins when byte is inside one — mirrors today's
    #    `walk_pipe_for_dsl` (v4/src/app.rs:2466) and the existing
    #    `dsl_hover_with_doc` path (app.rs:2146).
    for hit in ctx.index.dsl_bodies sorted by (hi - lo) asc:
        if hit.host_off <= byte <= hit.host_off + hit.raw.len():
            lsp = ctx.registry.dsl_for(&hit.op_name)?
            return ResolveHit::Dsl { lsp, hit, body_byte: byte - hit.host_off }
    # 2. Otherwise: deepest host node containing `byte`.
    #    nodes are sorted by `range.lo` asc; binary-search for the first
    #    node whose `range.lo <= byte`, then linear-scan backward to
    #    find ALL nodes overlapping `byte`, pick min-extent.
    lo = bsearch_le(ctx.index.nodes, byte)
    best = nil
    for n in ctx.index.nodes[..=lo].iter().rev():
        if n.range.hi < byte: continue
        if best is nil or (n.range.hi - n.range.lo) < (best.hi - best.lo):
            best = n
        if n.range.lo == 0: break   # cheap cutoff
    if best:
        def = ctx.registry.get(best.kind_id)?
        return ResolveHit::Host { def, node: best }
    ResolveHit::None

semantic_tokens(ctx):
    out = []
    for n in ctx.index.nodes:
        def = ctx.registry.get(n.kind_id)?
        let (tt, mods) = def.semantic_token() else continue
        type_idx = ctx.legend.type_index(tt)?
        mod_bits = ctx.legend.modifier_bits(mods)
        out.push(SemanticToken{ byte_range: n.range.into(), token_type: type_idx, token_modifiers: mod_bits })
    # DSL bodies layer on their own tokens, shifted into host coords
    for hit in ctx.index.dsl_bodies:
        let lsp = ctx.registry.dsl_for(&hit.op_name) else continue
        for t in lsp.semantic_tokens(hit.raw.as_bytes()):
            out.push(SemanticToken{
                byte_range: (hit.host_off as usize + t.byte_range.start)
                            ..(hit.host_off as usize + t.byte_range.end),
                token_type: t.token_type,
                token_modifiers: t.token_modifiers,
            })
    out.sort_by_key(|t| t.byte_range.start)
    out

lsp_hover handler:
    match resolve_at(ctx, byte):
        Host { def, node } => def.hover(ctx, node)
        Dsl  { lsp, hit, body_byte } => lsp.hover(hit.raw.as_bytes(), body_byte)
        None => None

lsp_completion handler:
    match resolve_at(ctx, byte):
        Host { def, node } => def.completions(ctx, node, byte - node.range.lo as usize)
        Dsl  { lsp, hit, body_byte } => lsp.completions(hit.raw.as_bytes(), body_byte)
        None => []
```

## Layer 3 — instance lifetimes

| Type | Lifetime | Where it lives |
|---|---|---|
| `Arc<dyn HostLspDef>` | `'static` singletons. Constructed once at process start, registered into `LspRegistry`, never mutated. | `LspRegistry::default_host()` mirrors `compile::lower::default_registry()` (v4/src/compile/lower/mod.rs). |
| `LspRegistry` | Process lifetime. One per `SprfState`. Cheap `Arc`-clone for borrowing. | Sibling to `SprfState::registry: Arc<Registry>` (v4/src/app.rs:520). |
| `HostLspNode` | Per-document: built fresh by the walker each `lsp_open`/`lsp_change`. Cleared in `lsp_close`. Survives reads until the next ingest replaces it. | `DocState.lsp_index: LspIndex` (new field on v4/src/app.rs:492). |
| `LspIndex` | Same as `DocState`: one per opened doc; replaced atomically each ingest. | Inside `DocState`. |
| `LspQueryCtx<'a>` | Per-RPC. Borrows `DocState`, `SprfState.registry`, `SprfState.facts`, `SprfState.sprf_store`. | Stack-lived in each `lsp_hover`/`lsp_completion`/`lsp_definition` handler (v4/src/app.rs:1505-1583). |
| Concept-specific payloads (`Arc<dyn Any>`) | Tied to their parent `HostLspNode` — `Arc` lets the def downcast at query time without copies. | Inside `HostLspNode.payload`. |

Interaction with `lsp_open`/`lsp_change` re-ingest: today's
`SprfState::ingest` (v4/src/app.rs:710) rebuilds `program`,
`walk_diags`, `probes`, `runtime_hovers` from scratch each call. The
walker grows ONE new output (`lsp_index: LspIndex`) appended to
`DocState` (v4/src/app.rs:492). Build cost is the same linear walk it
already does. Tying lifetime to `DocState` means the lock-discipline
question is exactly D3 from the LSP debt memo (see section 4).

## Layer 4 — storage, sequence of reads/writes, uniqueness

**Storage layout (the per-position resolver index):**

```text
LspIndex {
    nodes:       Vec<HostLspNode>,      // sorted by range.lo, then range.hi desc
    dsl_bodies:  Vec<DslBodyHit>,       // sorted by host_off
}
```

The `Vec<HostLspNode>` form is the simplest correct shape:

- Build: O(N) appends during walker traversal, one sort at end of
  `ingest`.
- Query: binary-search to first node with `range.lo <= byte`, then
  linear scan back over overlapping nodes (small window in practice
  because nested concepts are bounded by depth).
- Today's `walk_pipe_for_op` / `walk_pipe_for_dsl`
  (v4/src/app.rs:2466-2500) already does an unindexed linear pass
  through the AST each query and accepts that cost; an indexed
  binary-search-plus-scan is strictly better.

**Promotion path:** if perf shows up (target = 500-repo
multi-doc sessions), promote `nodes` to an `IntervalMap` keyed on
byte ranges. Same trait surface; section 9 step 6 lands it as an
addon RED. Linear-with-search wins step 1.

**Sequence of writes:**

1. `parse.rs::host_parse` → `Vec<PipeAst>` (unchanged).
2. `walk.rs::walk_program` → grows two new sinks alongside `walk_diags`
   and `pipes`: a `Vec<HostLspNode>` and a `Vec<DslBodyHit>`. Walker
   stamps nodes from `walk_op` / `classify_slot` / `walk_pipe`. Section
   6 details which concept gets stamped where.
3. `ingest` finalizes by sorting both vecs and dropping them into the
   new `DocState.lsp_index`.

**Sequence of reads:**

- `lsp_hover` / `lsp_completion` / `lsp_definition` handlers (today
  v4/src/app.rs:1505-1583) all become: `resolve_at` → trait dispatch.
- `semantic_tokens` is a full-doc fold; runs only at the LSP request
  rate, not on every keystroke.

**Uniqueness conditions:**

- One `HostLspNode` per source occurrence of a concept. A rule decl
  at `rule(:Person, name?, age?)` yields one `rule_decl` node spanning
  the whole call + three `typed_col_cell` children. Nodes never
  overlap with the same `kind_id` at the same `range`.
- `dsl_bodies` is keyed by `host_off`, which is unique by construction
  (one op per CST position).
- `kind_id` uniqueness in `LspRegistry::map` is enforced by `register`
  (panic on duplicate, mirror of `compile::lower::registry::register`).

**Lock discipline (D3 interaction, lsp-debt memo line 33):**

Today `SprfState::docs: Mutex<HashMap<String, DocState>>`
(v4/src/app.rs:504) is held through the whole `ingest`
re-parse (app.rs:769) — this plan does NOT close that bug; it inherits
it. Mitigation: build `LspIndex` BEFORE taking the mutex, then move
it in via the existing `docs.lock().unwrap().insert(...)` write
(app.rs:769-781). Read handlers take the mutex briefly to clone the
needed `Arc`s out of `DocState` and drop the guard before running
trait dispatch — same shape as today but the trait routing has no
lock-held side effects so the unlock-during-dispatch fix actually
works. Calling out as a hole in section 10 (D3-resolution is parallel
work, not in this plan's scope).

## Layer 5 — concept mapping table

Every host-language concept that surfaces in source today (plus the
ones cons-plan adds). Columns:

- **Concept** — what the user typed.
- **kind_id** — `HostLspDef::kind_id()` for that concept.
- **Hover** — what `hover()` returns.
- **Completion** — what `completions()` surfaces at this site.
- **Goto-def** — what `goto_def()` resolves to.
- **Sem-tok** — `semantic_token()` classification.
- **Today (gap)** — what works in v4 right now.
- **Under the trait** — what the impl ships.

| Concept | kind_id | Hover | Completion | Goto-def | Sem-tok | Today | Under the trait |
|---|---|---|---|---|---|---|---|
| Rule decl `rule(:Name, A?, B?)` | `rule_decl` | "rule `Name`\\ncols: A, B\\nbody: { … }" with cursor-flow count via `DocState.probes` | rules in `ctx.get_rule` namespace + reserved names (`cons`, `merge`) | n/a (this IS the def) | `FUNCTION` w/ `declaration` mod | None — runtime cursor-flow only via `host_hover` at app.rs:2063 | New |
| Rule call `Name(...)` (apply) / `Name?(...)` (query) | `rule_call` | "rule `Name` / cols / body span" — same payload as decl, just resolved through `LowerCtx::get_rule` | rule names in scope | rule_decl's range | `FUNCTION` | None | New |
| Term decl `x?` | `term_decl` | "term `x` (bind)\\nscope: rule `Name`" + bound value sample if walker recorded one | terms-in-scope from `Rule.captured` + walk-collected binders | n/a | `PARAMETER` w/ `declaration` mod | None | New |
| Term ref `x` | `term_ref` | "term `x` (read)\\nbound at <range>" | terms-in-scope | term_decl's range | `PARAMETER` | None | New |
| Dot projection `Person.name`, `x.a.b.c` | `dot_proj` | "field `name` of type `Person`" using `LowerCtx::col_type` chain | columns of head's type | rule_decl of the column's table | `PROPERTY` | None — `resolve_dot` runs but its output never reaches LSP | New; per-segment children pin (head=`type_ref`, segs=`dot_proj`) |
| Atom literal `:Point` | `atom_lit` | "atom `:Point`" plus rule lookup if `Point` is a declared rule | atom-keyed positions only (e.g. `rule(:_`) | rule_decl if it names one | `ENUM_MEMBER` (atoms = symbolic) | None | New |
| Kwarg cell `x: 1` | `kwarg_cell` | "kwarg `x` of op `<name>`" pulled from `OperatorDef::paren_args` | known kwarg names from the enclosing op's def | the relevant `ArgSig`'s doc as inlay (deferred to inlay; not goto) | `PARAMETER` | None | New; routes through `OperatorDef::paren_args` |
| Typed-col cell `x?: t.i64` (post-cons-plan) | `typed_col_cell` | "col `x` : `i64`" reading from `LowerCtx::col_type` | type values from value-space type IR | rule_decl of the column's table | `PARAMETER` + `TYPE` modifier | None — today the `?` is swallowed by `split_keyword_arg` (walk.rs:557, cons-plan step 0) | New, lands with cons-plan step 5 |
| Callable-Value sites (`apply`, `&.apply(...)`) | `callable_value` | "callable value\\ncols: …\\ncaptures: …" | `apply` keyword args | rule_decl if the callable is a named closure | `FUNCTION` w/ `defaultLibrary` mod | None | New, lands with `feat/callable-value` merge |
| Reify op + managed region `# <reify gen … >` | `reify_region` | "reified region\\nrule: <name>\\nspan covers reify-generated text" | n/a (the region is read-only generated content) | the reify call site that emitted it | `MACRO` | None | New |
| Intra-row self-eq site `f?(N?, N)` | `intra_row_self_eq` | "self-eq: column N = column N" | n/a | first ref of N | (no extra) | RED test `intra_row_self_eq_target.rs` lowers correctly but LSP doesn't surface it | New |
| Nested-rule scope + closure capture | `scope_open` | "scope: outer.inner\\ncaptures: A=…" using `Rule.captured` (rule.rs:48) | n/a (used by `references()`) | enclosing rule's decl | (no extra) | None | New; one node per `{...}` block boundary the walker pushes via `enter_scope` |
| Type annotation `t.i64` as value-space pipe | `type_anno_pipe` | "type IR: `i64`\\nlattice: …" | type-IR namespace | type-def site (TBD per types-in-value-space plan) | `TYPE` | None | New, blocked on type-IR landing; payload-only stub allowed in step 1 |
| Cons cell (post-cons-plan) | `cons_cell` | "cons cell: key=<…> value=<…> (positional / keyed)" | n/a | n/a | `OPERATOR` for `:` separator | None | New, lands with cons-plan step 1 |
| Merge block `{...}` (post-cons-plan step 6) | `merge_block` | "merge block: <N> arms" | n/a | n/a | `KEYWORD` for `{`/`}` braces | None | New, lands with cons-plan step 6 |
| DSL body cell (sql/re/glob/json/markdown) | (routed to `DslBodyLsp`) | n/a (delegated) | n/a (delegated) | n/a (delegated) | n/a (delegated) | Works: `dsl_hover_with_doc` (app.rs:2146), `lsp_dsl_completion` (app.rs:2162), `lsp_dsl_definition` (app.rs:2184) | Wrapped via `LspRegistry.dsl_for` — see section 7 |

Count: **15 host concepts** mapped (rows above, excluding the DSL
delegation row). Each gets one `HostLspDef` impl.

## Section 6 — where each compile layer participates

Constraint: NO layer gets an "if LSP enabled" branch. Each layer
participates by IMPLEMENTING the trait on its own types OR by
EXPOSING a single small accessor that the trait machinery calls.

| Layer | Participation | Justification |
|---|---|---|
| **PARSER** (`v4/src/compile/parse.rs`, `v4/src/compile/ast.rs`) | None — the parser stays sprf-blind. `OpCall.span`, `SlotText.span`, `DslText.span` (ast.rs:31-70) are ALREADY the hit-test source of truth. The walker reads them. | Adding a trait at the CST layer would re-do what the existing tree-sitter `injection_grammar` mechanism already does for DSL bodies. CST spans + the walker-stamped index are sufficient. |
| **WALKER** (`v4/src/compile/walk.rs`) | Grows ONE new sink: `nodes_out: &mut Vec<HostLspNode>` threaded through `walk_program` → `walk_pipe` → `walk_op` → `classify_slot`. Each classification arm in `classify_slot` (walk.rs:628-813) stamps a node with the kind_id for the arm it took. **NO LSP-NAMED METHOD CALLS.** Stamping is shaped as `concepts::stamp_term_ref(nodes_out, slot.span, name)` — a function in a new module `compile/lower/concepts.rs`. Walker imports that module and calls one stamp fn per arm. | The classification IS the concept-tagging — `classify_slot` ALREADY decides "this is an atom / term-ref / dot-proj / inline pipe", it just throws the discrimination away after computing the `Value`. The walker keeps its existing shape and the kind tagging is the cheapest possible bolt-on (one function call per arm). |
| **LOWER (`compile/lower/ctx.rs`, `compile/lower/value.rs`)** | None directly. `LowerCtx::resolve_dot` (ctx.rs:272) already produces the data the `dot_proj` concept needs; the walker harvests its `Ok(Value)` chain into the node payload before discarding. `LowerCtx::col_type` (ctx.rs:167) and `LowerCtx::get_rule` (ctx.rs:247) are READ by `HostLspDef` impls from `LspQueryCtx`, not augmented. | The lower layer's only job is computing the `Pipe<Cursor>`. The LSP layer needs READ access to its outputs, not new methods. |
| **REGISTRY** (`compile/lower/registry.rs`) | Untouched. `OperatorDef::paren_args` (op_def.rs:171) is the kwarg-name source for the `kwarg_cell` concept's `completions()` — read through `LspQueryCtx.registry`. | The op layer ALREADY publishes its kwarg names as a static slice. No new method needed. |
| **RUNTIME (`lsp.rs`, `app.rs`)** | `LspDiagComponent`, `LspHoverComponent`, `ExpectZeroComponent`, `ExpectMatchComponent` already emit Diag events with byte spans (lsp.rs:179-196, 276-286, 625-637, 758-767). These pipe into `runtime_hovers` / `runtime_diags` already; the LSP layer reads `DocState.runtime_hovers` (app.rs:499) via `LspQueryCtx`. **One new concept payload** carries the runtime-hover index a node points into so a `rule_decl`'s `hover()` can surface cursor-flow info using today's `host_hover` (app.rs:2063) text. | Runtime hover machinery is already-built; the trait layer aggregates, doesn't replace. |
| **CST/LSP (`v4/src/cst/lsp/{mod,position,providers,shift,highlights}.rs`)** | Stays. `DslBodyLsp` (providers.rs:31) is the sub-DSL trait; `LspRegistry` references it via `register_dsl(...)` (see section 7). `position::byte_to_position` / `position_to_byte` are used by `LspQueryCtx::shift_to_host`. `highlights::Legend` is consumed by `semantic_tokens()`. | These are already shaped as a sub-system; reuse > rebuild. |

**Stamping discipline.** Every node-stamp is a one-line call from
the walker into `concepts::stamp_*`. The walker's diff is purely:
add a `nodes_out` arg, add 15-ish `stamp_xxx` call sites at the
existing classification arms. The trait implementations themselves
live entirely in `v4/src/cst/lsp/concepts/{rule_decl,term_ref,dot_proj,…}.rs`
new files. Total walker LoC delta: tens, not hundreds.

## Section 7 — subsume / coexist with `DslBodyLsp`?

**Decision: COMPOSE (path c in the prompt).** Keep `DslBodyLsp`
(`v4/src/cst/lsp/providers.rs:31`) as a peer trait reached through
`LspRegistry.dsl_for(op_name)`. Do NOT unify into one trait, and do
NOT wrap `DslBodyLsp` behind a `HostLspDef`.

**Concrete sketch:**

- `LspRegistry` has two parallel maps:
  `map: HashMap<&'static str, Arc<dyn HostLspDef>>` and
  `dsl_lsp: HashMap<&'static str, Arc<dyn DslBodyLsp>>`.
- `resolve_at` already checks `dsl_bodies` first (section 2 pseudo).
  When the byte lands inside a body, the result is
  `ResolveHit::Dsl { lsp, hit, body_byte }` and the handler
  delegates to `lsp.hover(...)` directly (mirror of today's
  `dsl_hover_with_doc` at app.rs:2146).
- `semantic_tokens` (section 2) merges host tokens with DSL tokens
  via the existing `shift_to_host` (shift.rs:13).

**Justification:**

1. **Different shapes.** `HostLspDef` is concept-keyed by `kind_id`;
   `DslBodyLsp` is op-keyed (sql, re, glob, json, markdown). One key
   is "what is this fragment of source", the other is "what grammar
   does this body use". Forcing them into one trait would re-introduce
   a kind tag inside `DslBodyLsp` AND require every DSL impl to
   re-implement the byte-range hit-test that `LspIndex` already does.
2. **DSL bodies have a borrowed parse engine.** `tree-sitter::Tree`,
   `regex::Regex`, the json parser. `DslBodyLsp` already caches these
   internally. A wrapper layer would either re-cache (waste) or leak
   the cache key into `HostLspDef.payload` (smell).
3. **Today's app.rs path is body-pure.** `dsl_hover_with_doc`,
   `lsp_dsl_completion`, `lsp_dsl_definition`, `sql_lsp_diagnostics`
   (app.rs:2146-2210) all take only `(body_bytes, body_byte)`. The
   trait surface matches. Reusing it costs nothing.

**Where (a) "host trait WRAPS DslBodyLsp at body's outer cell" was
considered and rejected:** the outer cell is the op call (e.g. the
whole `` sql`…` `` op). A `HostLspDef` for "sql op" would forward
every method to the inner `DslBodyLsp`. That is dead weight; the body
hit-test (`dsl_bodies`) routes to `DslBodyLsp` directly with the
same byte-precision, and the op-as-a-whole (the backticks, the op
name) has no LSP signal beyond what an `op_invocation` concept would
already give it.

## Section 8 — interaction with cons-plan

Walk through cons-plan steps 0-7 (per
`plans/2026-05-19-cons-calling-unification.md` lines 142-194). For
each: does it ADD a host surface? REFACTOR an existing one? FORCE a
landing order?

| Cons step | Host surface impact | Action for this LSP plan |
|---|---|---|
| **Step 0** — `?`-decl-mark survives lexing (walk.rs:557) | ADD `typed_col_cell` concept. Until step 0 lands, `x?: t.i64` is a single positional slot — no LSP surface to attach to. After step 0, the `?` is a recognized decl-marker on a `Cons` cell whose value is a sub-cons-list with reserved `decl`/`ty` cells. | Defer `typed_col_cell` HostLspDef until step 0 merges. Section 9 build order step 6 lands it. |
| **Step 1** — `Cons` type + `ValueKind::ConsList` (value.rs) | REFACTOR `kwarg_cell` → `cons_cell`. The existing `CallArg` IS already a proto-cons (value.rs:50); `CallArg → Cons` rename is mechanical (35 refs / 7 files per cons-plan line 154). | Rename `kwarg_cell` HostLspDef to `cons_cell` after step 1 merges. Trait shape unchanged. |
| **Step 2** — `D-Q1 binder` (registry.rs:318-377) | No new concept. The `lower/positional-after-kwarg` diag stays; LSP `enrich_diag` hook (HostLspDef method) can attach a fix-it later. | Diag-enrichment hook is already on the trait; no change. |
| **Step 3** — `D-R3 root relabel` to `Container::Merge` | ADD `merge_block` concept (root level + nested `{...}`). | Land HostLspDef for `merge_block` AFTER step 3 + step 6. |
| **Step 4** — `cons`/`merge` ops + `apply(ConsList)` | REFACTOR `callable_value` concept's payload to take `ConsList` not `Vec<Value>`. | Trait surface unchanged; just the concrete payload shape inside `Arc<dyn Any>`. Done in lockstep with step 4 merge. |
| **Step 5** — value-space `ty` cell, `set_col_type`/`resolve_dot` | REFACTOR `dot_proj` concept: payload's "type name" becomes a value-space `Value`, not `Arc<str>`. `type_anno_pipe` concept moves from stub-only to real. | Trait surface unchanged; payload shape widens. |
| **Step 6** — `{` IS the merge op | (already covered by step 3 → `merge_block`) | n/a |
| **Step 7** — `&` is the current cursor; `&.value`, `&.at`, `&.terms.X` | ADD `cursor_view` concept (or fold into `dot_proj` if `&` becomes just another Value that resolves through `resolve_dot`). cons-plan step 7 says "no new dot machinery" so the latter — `dot_proj` covers it. | No new concept; ensure `dot_proj` HostLspDef impl recognizes `&` as a special head and renders hover text appropriately. |

**Landing order constraints:**

- This LSP plan can ship its first 5 concepts (`rule_decl`,
  `rule_call`, `term_decl`, `term_ref`, `atom_lit`) WITHOUT any
  cons-plan step landed. They live in the current `CallArg` world.
- `kwarg_cell` HostLspDef can ship today too; it becomes `cons_cell`
  via a rename at cons-plan step 1 (trivial mechanical update,
  no semantic change).
- `typed_col_cell` BLOCKS on cons-plan step 0.
- `merge_block` BLOCKS on cons-plan steps 3 + 6.
- The cons-plan should NOT modify any LSP code. This plan absorbs
  the cons refactor purely via concept-payload-shape evolution. No
  cons-plan step needs an LSP-aware branch.

## Section 9 — build order + RED tests

Each step is one concept (`HostLspDef` impl) or one resolver layer.
Each has a RED test mirroring `lsp_hover_smoke.rs` (handler-level) or
`dots_nested_rules_target.rs` (walker-level). RED first, then GREEN,
then commit. Worktree-isolated per the user's parallel worktree
workflow.

| # | Step | RED test name | Test shape | Cons-plan dep |
|---|---|---|---|---|
| 1 | **Skeleton: `HostLspDef` trait + `HostLspNode` + `LspRegistry` + `LspIndex` + `resolve_at` + `RuleDeclDef` impl.** Walker stamps `rule_decl` nodes. Handler delegates. | `tests/host_lsp_rule_decl_hover.rs::rule_decl_hover_shows_name_and_cols` | open `rule(:Point, x?, y?);` then `lsp_hover` at byte 5 → expect `"rule \`Point\`\ncols: x, y"`. | none |
| 2 | **`TermDeclDef` + `TermRefDef`.** Stamp at the `?` arm and the bare-ident arm of `classify_slot` (walk.rs:685-703). | `tests/host_lsp_term_ref_hover.rs::term_ref_shows_bound_at` | `rule(:p, X?) { X };` hover X read → expect "term `X` (read)\nbound at \<range\>". | none |
| 3 | **`AtomLitDef`.** Stamp at `raw.strip_prefix(':')` arm (walk.rs:659). | `tests/host_lsp_atom_hover.rs::atom_resolves_to_rule_when_named` | `rule(:foo);` then `tag(:foo);` hover the `:foo` in `tag` → "atom `:foo` (rule)" with goto-def into the decl. | none |
| 4 | **`DotProjDef`.** Stamp the entire dotted head + per-segment children at walk.rs:710-749. Payload carries the resolved type chain from `LowerCtx::resolve_dot`. | `tests/host_lsp_dot_proj_hover.rs::dot_segment_shows_field_type` | `rule(:Person, name?, age?); tag(:p, Person.name);` hover `name` → "field `name` of type `Person`". | none |
| 5 | **`KwargCellDef` (renamed to `ConsCellDef` post-step 1).** Stamp from `split_keyword_arg` (walk.rs:551). Completions use `OperatorDef::paren_args`. | `tests/host_lsp_kwarg_completion.rs::kwarg_completion_lists_op_arg_names` | `lsp.hover[…` then `:` trigger char → expect kwarg names from `LSP_SPEC`. | rename at cons step 1 |
| 6 | **Resolver wires DSL passthrough.** `LspRegistry::register_dsl` for sql/re/glob/json/markdown. `resolve_at` checks `dsl_bodies` first. `lsp_hover`, `lsp_completion`, `lsp_definition` handlers route through the new resolver. **Remove** today's hand-rolled `walk_pipe_for_dsl` / `walk_pipe_for_op` (app.rs:2466-2500); the resolver replaces them. | `tests/lsp_hover_smoke.rs::lsp_hover_inside_sql_body_uses_dsl_provider` — keeps PASSING (regression gate) | (existing test; this step proves we didn't regress sub-DSL hover.) | none |
| 7 | **`SemanticTokenDef` + `semantic_tokens()` function.** Wire into sprefa-lsp's `semantic` module so the existing `legend_token_types` (`v4/crates/sprefa-lsp/src/semantic.rs`) consumes the merged token stream. | `tests/host_lsp_semantic_tokens.rs::rule_decl_emits_function_declaration_token` | parse `rule(:foo);`, ask semantic_tokens, expect one FUNCTION+DECLARATION token over the `foo` ident. | none |
| 8 | **`ScopeOpenDef` + `Rule.captured` surface.** Stamp at `LowerCtx::enter_scope` / `exit_scope` boundaries (ctx.rs:176-181). | `tests/host_lsp_scope_hover.rs::nested_rule_shows_outer_captures` | `rule(:outer, A?) { rule(:inner) { A } };` hover at `inner` opener → "scope: outer.inner\ncaptures: A". | none |
| 9 | **`CallableValueDef`.** Lands in lockstep with `feat/callable-value` merge. | `tests/host_lsp_callable_value_hover.rs` | TBD with callable-value plan. | callable-value merge |
| 10 | **`TypedColCellDef`.** | `tests/host_lsp_typed_col_hover.rs::typed_col_shows_value_space_type` | `rule(:p, x?: t.i64);` hover `t.i64` → "type IR: i64". | cons step 0 + 5 |
| 11 | **`MergeBlockDef`.** | `tests/host_lsp_merge_block_hover.rs::brace_block_shows_arm_count` | `{ a; b; c }` hover `{` → "merge block: 3 arms". | cons step 3 + 6 |
| 12 | **`ReifyRegionDef`.** | `tests/host_lsp_reify_region_hover.rs` | reify gen region shows source span. | reify lands |

Steps 1-8 are deliverable on `main` today. Steps 9-12 follow the
named upstream branches. Each step compiles green between numbered
commits.

## Section 10 — open design questions / known holes

| # | Hole | Severity | Notes |
|---|---|---|---|
| H1 | `DocState` `Mutex` held across re-ingest (D3 in lsp-debt memo) intersects this plan: the walker builds `LspIndex` inside the locked section. | HIGH | Not fixed here. Mitigation in section 4 (build before lock). Real fix is parking lot or rwlock — separate track. |
| H2 | `Arc<dyn Any + Send + Sync>` payloads lose static typing. Every `HostLspDef` impl downcasts; a type mismatch crashes at query time, not lower time. | MEDIUM | Mirror of `Component::describe()` (component.rs:152). Mitigation: per-concept payload structs in a sibling module `concepts/payload.rs`; one downcast helper per concept with `?`-return. Could go all the way to a sealed enum `HostLspPayload { RuleDecl(RuleDeclPayload), TermDecl(TermDeclPayload), … }` if downcast cost shows up. |
| H3 | The walker stamps once at lower time. If the lower fails (e.g. unknown op), the partial stamp set may leak. | MEDIUM | Mitigation: keep stamps in a local `Vec` until the pipe walk returns; on failure, drop. Symmetric with how `walk_diags` is collected. |
| H4 | Concept overlap. `Person.name` produces both a `dot_proj` and (children) `type_ref` + `dot_proj` segments. `resolve_at` returns "min-extent" but choice between same-extent concepts is undefined. | LOW | Document a stable kind_id ordering (define a `Concept` enum with a `Discriminant`-based ord) for tie-break. Pin once seen in practice. |
| H5 | DSL parsing of `${…}` sub-pipe carveouts (`InterpKind::SubPipe`, op_def.rs:99) already re-enters the walker recursively (walk.rs:264-313). Each recursion needs to stamp into the SAME `nodes_out` with shifted spans. | MEDIUM | Pass `nodes_out` through `walk_op`'s carveout call site; rebase the local stamps by the body offset (the same `body_offset` already computed at walk.rs:283). |
| H6 | `Cursor.at` / `term.at` ref the SprfStore (lsp.rs:164, 267). `RuleCallDef::hover` reading "captures" needs `Rule.captured` (rule.rs:48) which is only on the `Rule` value, not the AST. The walker has `LowerCtx::get_rule` but doesn't currently keep a node→Rule pointer past the call. | MEDIUM | Either stash a `Weak<Rule>` in the `rule_call` payload (the registry's `rules: Arc<Mutex<HashMap<…, Rule>>>` lives at ctx.rs:34 for the compile duration only), or eagerly snapshot `name + cols + captured` at stamp time. Choose eager-snapshot to keep the payload self-contained and free `LowerCtx` to drop. |
| H7 | `LspQueryCtx::facts` is `&dyn FactStore<Cursor>`, taken from `SprfState.facts` (app.rs:506). Today's hover code path (app.rs:1505-1555) holds the `docs` mutex while calling into SQL providers (`sql_lsp_ctx`). The new resolver should NOT make this worse; with the lock-drop-then-dispatch shape in section 4, facts can still be accessed since `SprfState.facts: Arc<dyn FactStore<Cursor>>` is independent of `DocState`. | LOW | Confirm by audit when wiring. |
| H8 | The `text` reflow scheme. LSP hover Position uses UTF-16 code units (position.rs). The trait returns LSP `Range` directly — every `HostLspDef::goto_def` must call `LspQueryCtx::shift_to_host` correctly. | LOW | Provide a one-liner `LspQueryCtx::range_for(node: &HostLspNode) -> LspRange` so the common case is trivial. |
| H9 | Semantic-tokens delta protocol. Today sprefa-lsp returns full tokens each time (`tower-lsp` default). The trait emits a sorted Vec; the LSP server still encodes to absolute then delta. | LOW | Encode lives in sprefa-lsp, not in this trait. Trait stays absolute-byte-range. |
| H10 | Concept-payload growth. As more concepts ship, `LspIndex.nodes` grows linearly with source size. For 500-doc workspaces this is fine; budget needs revisiting at scale. | LOW | Section 4 covers — `IntervalMap` is the promotion target. |

## Promote and feedback

Promote-and-feedback: this plan is ready for a multi-agent feedback
round in the cons-plan style (grammar / runtime / type-model lenses,
distilled into a `## Consolidated feedback` table). Suggested lenses
for this round: **LSP-protocol** (tower-lsp / lsp_types invariants),
**lower-walker** (real walker reach, span correctness, scope path),
**LSP-debt cross-check** (D3 lock discipline, D5 sentinel coupling).
Decide after reviewing.
