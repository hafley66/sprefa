# How the runs partitioned new types

Same 6 runs (3 Opus, 3 Haiku) + the initial stratified pass (S). This doc ignores the
duplication findings and looks only at **what new types each run minted and where it drew the
boundary** — the part that actually commits you to an architecture.

The recurring question at each hotspot: does the new abstraction become a **noun** (a struct/trait
that *owns* the facets) or a **verb** (a generic function parameterized by closures), and is the key
**typed** (enum/field) or **stringly** (HashMap key, macro arm)?

---

## 1. The builtin-rel groups (engine.rs fan-out)

Five facets are currently kept in lockstep across separate sites: `names`, `decls`, `used(prog)`,
`refresh()`, `reserved_msg`. Every run wanted to unify them; they disagreed on the **shape of the
unifying type**:

| Run | Proposed type | Partition shape |
|-----|---------------|-----------------|
| Opus A | `trait LazyIndexer { rels; decls; used; refresh; reserved_label }` + `fn indexers() -> &[&dyn LazyIndexer]` | **open vtable** — one impl per group, polymorphic |
| Opus B | `static BUILTIN_GROUPS: &[BuiltinGroup{ names, decls, used, refresh, reserved_msg }]` | **closed data table** — struct of data + fn pointers |
| Opus C | `BuiltinGroup { decls, names, label, refresh }` registry slice | **closed data table** (same as B, independently) |
| Haiku B | `all_rel_decls() -> HashMap<String, Vec<RelDecl>>` + `rels_used_for(prog, kind: &str)` | **stringly map** — facets stay separate, joined by a string key |
| Haiku A | "macro or factory function" | **no type** — codegen over the duplication |
| S | `trait Indexer { needs_refresh; refresh }` + `TickPlan` | open vtable + a plan struct |

Axis: Opus mints a **named aggregate that owns all five facets** (whether as a trait or a struct of
fn-pointers); Haiku keeps the facets as **parallel collections unified by a `&str` key**, or doesn't
introduce a type at all. The trait-vs-data-table split *within* Opus is the real design fork:
`LazyIndexer` (A) is extensible by downstream impls; `BuiltinGroup` (B/C) is a fixed table you append
a row to. B/C is the lighter answer and two independent Opus runs picked it.

---

## 2. The 4 corpus refreshers (type/call/dataflow/doc)

Strongest **type convergence** in the whole study. The resolver state — `by_name`, `sym_at`, `scip` —
got extracted as a struct with those exact three fields by four independent runs:

| Run | Resolver type | Driver |
|-----|--------------|--------|
| Opus A | `Resolver { by_name, sym_at, scip }` + `fn resolve(repo,file,name)` | `SourceIndexer` impl of `LazyIndexer` |
| Opus B | `struct NameResolver` (by_name/sym_at/scip) | `extract_corpus`-style |
| Opus C | `NameResolver { by_name, sym_at, scip }` + `resolve`/`qualify` | `extract_corpus(langs, exts, f)` |
| Haiku B | — (no resolver struct) | `generic_extraction_refresh<F,T>(name, extractor, output_rels, db)` |
| Haiku C | — (row-builder fns instead) | `refresh_builtin_indexer<F, T>(sql, extract_fn, consolidate_fn)` |
| S | `Resolver{by_name,sym_at,scip}` (implicit) | `trait CorpusIndexer { type Facts; … }` |

Axis again: **Opus extracts the noun** (`NameResolver`, a stateful struct that owns the indices and
exposes `resolve`/`qualify`). **Haiku extracts the verb** (`generic_extraction_refresh<F,T>` /
`refresh_builtin_indexer<F,T>` — a higher-order function taking closures, no resident state type). Same
target, opposite partition: Opus localizes the *state*, Haiku localizes the *control flow*.

---

## 3. Per-tick state and arg-threading → context structs

Independently, almost every run converged on the idea that the multi-arg threading and the scattered
mutable `Engine`/walker fields want to become **named bundles**. The bundles they minted:

| Concern | Proposed bundle | Run(s) |
|---------|-----------------|--------|
| Tick plan / classified program | `TickPlan{ mode: Full\|Paths }` / `ProgramPlan` / `Plan::build` | S, Opus A, Opus B, Opus C |
| Per-tick scratch vs config | `TickStats` + `TickScratch` (split out of `Engine`) | Opus B |
| Dataflow walker context (the 5–9 threaded args) | `FlowCtx` / `TsFlowContext` | Opus C, Haiku B |
| Data-walk context | `WalkCx{ fmt, content }` | Opus C |
| Gen state (7 threaded args) | `GenSink` (struct) vs `GenTargetVisitor` (trait) | Opus C / S |
| Git ref event (5-positional tuple) | `RefAdvance` struct | Opus A |
| Daemon session (prog+program_files+eng under 1 lock) | `Mutex<Session>` | Opus B |
| LSP line lookup | `LineIndex` (built once/file) | S, Opus B, Opus C |

The convergence here is the *direction*, not the name: long parameter lists and god-structs get
sliced into small role-typed bundles. Opus did almost all of these; Haiku produced only
`TsFlowContext` and `RefreshPlan{rel_kind, predicate, refresh_fn}`.

---

## 4. The three dataflow walkers

Everyone wanted one driver + a per-language seam, but partitioned the seam differently:

| Run | New types | Where the boundary sits |
|-----|-----------|------------------------|
| Opus A / S | `trait FlowNode { kind; children; text }` + `lift_flow<N: FlowNode>` | **adapter over the AST node** — one generic walker, three node adapters |
| Opus C | `enum FlowOp` classifier + `drive()` interpreter + `trait FlowLang { classify; span }` | **classify-then-interpret** — language reduces its AST to `FlowOp`, one interpreter consumes it |
| Haiku B | `trait ExprWalker { handle_identifier; handle_literal; handle_call; handle_member_access }` | **visitor with one method per node category** — language supplies the handlers |

Three genuinely different partitions of the same code: node-adapter (the AST stays, you wrap it),
op-classifier (the AST is reduced to a neutral op enum first), visitor (the walk is fixed, behavior is
injected per node kind). Opus C's classify-then-interpret is the only one that fully decouples the
walk from the language; the FlowNode adapter is the cheapest.

---

## 5. parse_file extraction arms

| Run | New type | Partition |
|-----|----------|-----------|
| S | `trait Extractor { extract; interns }` per BodyItem variant | **polymorphic** — one impl per op |
| Opus A | `Hit` struct + `apply_hits`; arms become free `fn extract_*() -> Vec<Bind>` | **data + combinator** |
| Opus C | `fan_out<H>(binds, hits, bind_hit)` + free fns | **data + combinator** |
| Haiku B | `struct MatchExtractor { matches, text }` + `run_regex_matches`/`run_ast_walk`/`run_sg_walk` | **value type + per-kind methods** |

Notable: the stratified pass (S) reached for a **trait** (`Extractor`); both Opus runs declined the
vtable and instead made each arm a **plain function returning `Vec<Hit>/Vec<Bind>`** unified by one
`fan_out`/`apply_hits` combinator. The Opus answer is the lighter partition — no dynamic dispatch,
the variation lives in free functions, the shared cross-product lives in one generic combinator.

---

## 6. Where Opus refused to introduce a type (and Haiku did)

The inverse is as telling. For **RPC dispatch** (`daemon.rs::handle_request`):

- Haiku B: `struct RpcDispatcher { handlers: HashMap<String, fn(Msg)->Result<Value>> }` and a separate
  `struct RequestHandlers { on_initialize, on_hover, … }` + `message_loop`.
- Opus B / Opus C: **no new type** — "one `handle_<method>` free fn per arm", keep the match, isolate
  the lock scope per arm.
- S: `struct RequestHandler<'a>(&Daemon)` with a method per RPC.

Haiku reaches for a `HashMap<String, fn>` dispatcher (stringly indirection); Opus keeps it as a flat
set of functions behind the existing match. Same instinct as §1/§2 — Haiku defaults to a
string-keyed map, Opus defaults to either a typed aggregate or no new type at all.

---

## 7. Typed-key vs stringly-key, across all hotspots

| Hotspot | Opus partition | Haiku partition |
|---------|----------------|-----------------|
| builtin groups | `enum`/struct table | `HashMap<String,…>` |
| edge kinds | `enum EdgeKind` (A, C, S) | left as `&'static str` |
| dispatch (rpc, source-op, item) | free fns / `const SOURCE_OPS` slice | `HashMap<String, fn>` |
| corpus resolver | `NameResolver` struct | generic fn + closures |
| enum parse/sql arms | `&[(&str, Variant, &str)]` data table | macro / map |

Consistent: **Opus encodes the key in the type system** (enum variant, struct field, slice of tuples);
**Haiku encodes the key as a runtime string** (HashMap, macro arm). Both remove the duplication; only
the Opus partition makes a wrong key a compile error.

---

## 8. The new types worth minting (cross-confirmed)

Ranked by how many independent runs proposed the **same boundary**:

1. **`NameResolver { by_name, sym_at, scip }`** with `resolve`/`qualify` — 4 runs (Opus A/B/C + S). The
   most-agreed new type in the study.
2. **`BuiltinGroup { names, decls, used, refresh, reserved_msg }`** as a registry slice — 2 Opus runs
   independently + a trait variant from a 3rd. Drives the 7–8-site fan-out from one table.
3. **`enum EdgeKind`** (field/variant/impl/generic/param/returns/uses/same-package) — 3 runs. Replaces
   the `&'static str` edge kind everywhere.
4. **`LineIndex`** (per-file, built once) — 3 runs. Kills the O(filelen) LSP position scans.
5. **A per-tick plan bundle** (`TickPlan`/`ProgramPlan`) carrying `mode: Full|Paths` — 4 runs. Lets
   `tick`/`tick_paths` share one body.
6. **`FlowCtx`/`WalkCx`** context structs to kill the 5–9-arg threading — 2–3 runs.
7. **A flow seam** — pick one: `trait FlowNode` (cheap) or `enum FlowOp` + `drive()` (full decouple).

Lower-confidence (single-run) type ideas that are still worth noting because they name a real seam:
`TickStats`+`TickScratch` split of `Engine` (Opus B), `Mutex<Session>` for the daemon's 3-mutex race
(Opus B), `RefAdvance` for the 5-tuple (Opus A), `GenSink` vs `GenTargetVisitor` for gen state.

---

## 9. Summary

- **Where the new boundary goes** is far more consistent than **what shape it takes**. All six runs
  cut at the same seams (resolver state, builtin-group facets, edge kinds, per-tick plan, walker
  context); they disagree on trait vs struct-table vs map vs generic-fn vs macro.
- **Opus partitions toward typed nouns**: a struct/enum that owns the facets, keys encoded in the type
  system, context bundled into role types, and a willingness to introduce *no* type when free
  functions suffice.
- **Haiku partitions toward stringly verbs**: `HashMap<String, fn>` dispatchers, generic
  `fn<F,T>(closures)` over closures, macros/factories — the duplication goes away but the key stays a
  runtime string.
- The one new type every tier agrees on is **`NameResolver`**. After that, `BuiltinGroup` (as a data
  table, not a trait), `EdgeKind`, `LineIndex`, and a `TickPlan` mode-bundle are the safe mints.
