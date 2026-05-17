# v4 cross-language type/binding graph — prior art + algorithm research

Source: claude-session-2026-05-16. Intent captured in `human-goals.md`
(`v4-cross-language-type-graph-research-intent`). This is the findings doc.

Scope: partial / best-effort type+binding graph across Rust, Go, Kotlin from
tree-sitter CST alone. No full type inference. 500+ polyglot repos, low RAM.
**Hard rule for this engine: no running the program, no running the compilers.**

---

## TL;DR answers to the three questions

1. **Does anything already do this on tree-sitter?** Yes — **GitHub stack-graphs
   + tree-sitter-stack-graphs** is exactly "roughly resolve the binding graph
   from CST, no build system." It is the closest prior art. Caveat: the
   `github/stack-graphs` repo was **archived 2025-09-09** and ships no official
   Rust/Go/Kotlin rules. Port the algorithm, do not depend on upstream.
2. **Algorithm family for cross-language resolution/crawl?** **Scope graphs**
   (Néron/Visser, "A Theory of Name Resolution", ESOP 2015) and their
   engineering specialization **stack graphs**. Resolution = enumerate
   label-regex-constrained paths in a graph and rank them for shadowing. Partial
   resolution falls out for free: a missing file is missing edges, fewer paths,
   never a crash.
3. **How far without a compiler / without RAM?** Tier it. Tier-2 (tree-sitter
   symbol extraction + name equality) gets the import/module/symbol graph and
   ~70-80% of intra-repo "go to def" with near-zero RAM. Tier-3 (scope/stack
   graph) gets correct lexical scoping, shadowing, qualified paths, cross-repo
   linking, still no compiler. Generics / trait resolution / macro-expanded
   names are the part you give up — that is the "cursed" tier-4 that needs the
   compiler, and we do not pay for it.

---

## The compiler question (this is the important one)

**SCIP is a file format, not an indexer.** Two separable things:

- **The precise SCIP *indexers*** (`scip-typescript`, `scip-java`,
  `scip-python`, `scip-clang`, `rust-analyzer --scip`, `scip-go`/gopls): these
  **do run the compiler/typechecker and need the build graph.** We do **not**
  use these. They violate the no-compiler constraint and the RAM budget.
- **The SCIP *symbol-string descriptor scheme***: a pure naming convention for
  globally-unique symbol identity. Grammar:

  ```
  <symbol>     ::= <scheme> <package> (<descriptor>)+  |  'local ' <local-id>
  <package>    ::= <manager> <package-name> <version>
  <descriptor> ::= namespace | type | term | method | type-parameter
                 | parameter | meta | macro
  ```

  Nothing about emitting that string requires a compiler. We synthesize the
  descriptor string ourselves from the tree-sitter CST (module path + nesting +
  identifier + kind). It becomes the **cross-repo join key**: two independently
  produced per-file fact sets link by string equality, no global graph build, no
  ID rewriting. That is the *only* thing we adopt from SCIP.

So: **we run zero compilers.** We borrow SCIP's identity convention so that a
ref produced from repo A's CST and a def produced from repo B's CST collide on a
string when they are the same symbol, the same way v0 used normalized string
equality for cross-repo links — same posture, structured key.

Same logic kills Kythe/Glean/rust-analyzer/gopls/Kotlin-AA for the engine
core: all compiler-coupled, server-class RAM. Optional opt-in tier-4 per repo
only if a real index already exists; never required.

---

## What stack graphs actually is (the model to port)

Per-file, from tree-sitter CST, emit a graph:

- **def** = a *pop* node, **ref** = a *push* node.
- Edges encode language scoping (lexical parent, member access, import/module).
- Each edge carries an integer **precedence** for shadowing.

Resolution is a pushdown automaton with two stacks:

- **symbol stack** = the name path being resolved (`a.b.c`).
- **scope stack** = which scopes still to look in.

You may only traverse a pop node if its symbol matches the symbol-stack top.
Accepting paths of that PDA = valid bindings.

**Scale property (this is why it fits 500 repos / low RAM):** *partial path
stitching*. Each file is analyzed in isolation and produces partial paths.
Complete paths are formed by concatenating partial paths one file at a time.
Construction and resolution are **file-incremental** (linear reanalysis on
change, not quadratic). Persistence is **SQLite** holding per-file subgraphs +
partial paths — designed to spill to disk, hold a working set in RAM. This maps
directly onto our existing store/DD per-file fact model and the v0
parse-once-spill-to-sqlite discipline.

**Cross-repo:** the "virtual file set" = current repo + dependency repos pinned
to the manifest commit; stitching crosses repo boundaries with no special case.
This is the same shape as the v4 ghcache cross-rev reachability vision.

**What it resolves:** defs/refs, imports, module/lexical scoping, shadowing,
ambiguity (= multiple maximal paths), unresolved (= no path).
**What it punts:** dataflow / parameter value tracking, and generic
type-parameter resolution (Java-style inheritance through generics) — explicitly
unsolved upstream. That punt is acceptable and is exactly the line we draw.

Upstream status: production at GitHub since Nov 2021 (Python, every commit,
millions of repos). Dual MIT/Apache-2.0. **Repo archived 2025-09-09.** Official
TSG rules exist only for Python/Java/TS/JS. Third-party `tree-sitter-kotlin-sg`
exists on docs.rs. **No Rust, no Go upstream.** We write our own rules via
`tree-sitter/tree-sitter-graph` (the general node/edge-emitting DSL that stack
graphs is one consumer of).

Refs:
- https://arxiv.org/abs/2211.01224 (Stack graphs: Name resolution at scale)
- https://github.com/github/stack-graphs
- https://github.com/tree-sitter/tree-sitter-graph
- https://web.cecs.pdx.edu/~apt/esop15.pdf (A Theory of Name Resolution)
- https://arxiv.org/abs/2210.06121 ("Knowing When to Ask", Statix — delay
  queries that depend on not-yet-present scope info; relevant to streaming /
  incremental partial resolution)

---

## Rust tree-sitter-graph rule sketch

A `.tsg` file is a list of **stanzas**. Each stanza matches a tree-sitter query
pattern and emits scope-graph nodes/edges. Resolution is a stack walk: a
**reference pushes** a symbol, a **definition pops** it, and a pop is only
traversable when it matches the symbol-stack top.

```
  tree-sitter CST
        │  (one stanza per binding construct)
        ▼
   rust.tsg  ──emit──►  per-file scope graph
                               │
                               ▼
                        SQLite (spill, working set in RAM)
                               │  partial-path stitch (per file)
                               ▼
                     symbol / scope-stack PDA  ──►  resolved bindings
```

Edge + node legend (kept deliberately small):

```
  ──P──►   parent / lexical edge   "also look in the enclosing scope"
  ──M──►   member / "." edge       "look inside this module or type"
  push x   reference node          pushes x onto the symbol stack
  pop  x   definition node         pops x; traversable only if stack top = x
```

The stanzas (`enclosing_*` are inherited scoped vars threaded down by the
`source_file` stanza; `ROOT_NODE` is the cross-file/cross-repo global):

```tsg
;; file = a module scope hanging under the global ROOT
(source_file) @root {
  node @root.defs                       ; names defined at file/module top
  node @root.lexical                    ; lexical scope for those items
  edge @root.lexical -> @root.defs
  edge ROOT_NODE     -> @root.defs      ; reachable by crate/mod path
}

;; fn f() {}  →  pop "f" into the enclosing defs, fresh body scope
(function_item name: (identifier) @name body: (block) @body) @fn {
  node def
  attr (def) type = "pop_symbol", symbol = (source-text @name),
             is_definition, source_node = @name
  edge @fn.enclosing_defs -> def                 ; visible to siblings
  node @body.lexical
  edge @body.lexical -> @fn.enclosing_lexical    ; body sees outer names
}

;; mod m { .. }  →  nested module; m.<x> resolves into m's defs
(mod_item name: (identifier) @name body: (declaration_list) @body) @mod {
  node mdef
  attr (mdef) type = "pop_symbol", symbol = (source-text @name),
              is_definition, source_node = @name
  edge @mod.enclosing_defs -> mdef
  node @body.defs
  edge mdef -> @body.defs                        ; ──M──► member edge
}

;; use a::b;  →  reference chain pushed toward ROOT, name re-bound locally
(use_declaration argument: (scoped_identifier) @path) @use {
  node ref_last                                  ; rightmost segment
  attr (ref_last) type = "push_symbol", symbol = "<last seg>",
                  is_reference, source_node = @path
  node ref_first                                 ; leftmost segment
  attr (ref_first) type = "push_symbol", symbol = "<first seg>"
  edge ref_last  -> ref_first                    ; stack reads first..last
  edge ref_first -> ROOT_NODE                    ; resolve from crate root
  edge @use.enclosing_defs -> ref_last           ; bound name now in scope
}

;; struct S { field: T }  →  type def + member set so S.field can walk on
(struct_item name: (type_identifier) @name
             body: (field_declaration_list) @fields) @s {
  node sdef
  attr (sdef) type = "pop_symbol", symbol = (source-text @name),
              is_definition, source_node = @name
  edge @s.enclosing_defs -> sdef
  node @fields.members
  edge sdef -> @fields.members                   ; ──M──►
}
(field_declaration name: (field_identifier) @fname type: (_) @ty) @f {
  node fdef
  attr (fdef) type = "pop_symbol", symbol = (source-text @fname),
              is_definition
  edge @f.enclosing_members -> fdef
  node tref                                       ; field's type is a ref,
  attr (tref) type = "push_symbol", symbol = (source-text @ty)  ; so a.b.c
  edge fdef -> tref                               ; keeps walking
  edge tref -> @f.enclosing_lexical
}

;; impl S { fn m() {} }  →  methods land on S's member set
(impl_item type: (type_identifier) @tname
           body: (declaration_list) @body) @impl {
  node tref
  attr (tref) type = "push_symbol", symbol = (source-text @tname)
  edge tref -> @impl.enclosing_lexical            ; find the type S
  node @body.members
  edge @body.members -> tref                      ; pops here attach to S
}
```

Worked example. Source `src/lib.rs`:

```rust
mod m { pub fn f() {} }
use m::f;
fn g() { f(); }
```

Resulting graph (who flows into who):

```
                       ROOT
                        ▲ P
              ┌─────────┴──────────┐
              │  file defs (lib.rs)│◄───────────────┐
              └──┬────────┬────────┘                │ P
            pop m│        │pop f  (from `use m::f`)  │
                 ▼        ▼                          │
          ┌──────────┐  ┌─────────────────┐         │
          │  mod m   │  │ use m::f         │         │
          │  defs    │  │ push f → push m  │─────────┘  → ROOT
          │  pop f   │  └─────────────────┘
          └────┬─────┘
             M │  (m.<x> walks into mod m's defs)
               ▼
          fn f  (definition)
```

Resolution trace for the call `f()` inside `g`:

```
  ref f ──push f──► g body ──P──► file defs
     │                              │
     │   pop f  (the name `use m::f` bound here)
     ▼                              ▼
  use-node: push f, push m ──► ROOT ──► mod m defs
                                          │  pop m, then pop f
                                          ▼
                                       fn f   ✓ bound
```

Add a `tref → ROOT_NODE` fallback edge on unresolved type references so a
missing/unruled dependency degrades to "no path" (tier-2 behavior) instead of
failing the file. Generics, trait method resolution, and macro-expanded names
are deliberately not modeled here — that is the tier-4 line.

---

## The tiered strategy (Sourcegraph model, adapted)

Degrade outward, mix tiers within one answer:

| Tier | Mechanism | Needs | RAM | Gets you | Punts |
|---|---|---|---|---|---|
| 1 | grep / normalized-string equality (v0) | nothing | ~0 | candidate links | everything imprecise |
| 2 | tree-sitter symbol+import extraction, name equality (scip-ctags shape) | tree-sitter grammar | tiny | symbol list, import/module graph, ~70-80% intra-repo defs | scoping, shadowing, cross-repo identity |
| 3 | scope/stack graph from CST, partial-path stitching, SQLite spill | tree-sitter-graph rules per lang | working-set only | correct lexical scoping, shadowing, qualified paths, cross-repo via SCIP-string key | generics, traits, macro-expanded names, dataflow |
| 4 | real compiler index (rust-analyzer/gopls/Kotlin-AA → SCIP) | compiler + build graph | GB/process | precise generics/traits | nothing — but unaffordable at 500 repos |

We build **tier 2 and tier 3**. Tier 1 already exists from v0. Tier 4 is
opt-in per repo only, never required, never on the hot path. A file that fails
to parse or has no rules drops to the tier below — fewer edges, never a failed
index. Graceful degradation is structural, not error handling.

---

## Dotted-path projection + cycle detection (the backbone the human wants)

Goal: print the type/data model like a folder review with dots
(`a.b.c.d`), every route through the model, mark where the circles are.
Applies identically to a resolved struct→field→struct type graph and a SQL
table→FK→table graph.

The resolved graph is directed and **will** have cycles (self-FK, mutual FK,
recursive structs, `Box<Self>`). Naive DFS dotted-path enumeration over a cyclic
graph is **infinite**. Required pipeline:

1. **Tarjan SCC**, O(V+E), one DFS, O(1) extra per node (index/lowlink). First
   pass at scale: tells you exactly which nodes/edges are in a cycle. Any SCC
   with >1 node, or a self-loop, is a circle to mark in the printout.
2. **Cut**: designate one back-edge per SCC (or a depth cap) → graph becomes a
   DAG for projection purposes. The cut edges are precisely the "here is the
   circle" annotations in the folder-with-dots view.
3. **DFS path enumeration** over the resulting DAG → the dotted paths. DFS keeps
   an on-stack set; hitting an on-stack node = a back-edge = stop + record the
   cycle marker instead of recursing.
4. **Johnson's algorithm** (O((V+E)(C+1)) time, **O(V+E) space**) only when you
   must *report the actual circuits* (e.g. "FK loop: orders→customers→orders"),
   not just detect them. Output-sensitive: cheap unless the graph is genuinely
   very cyclic. Optionally bounded-length variant (arXiv:2105.10094) to cap
   blowup at 500-repo scale.

Refs:
- Johnson 1975: https://www.cs.tufts.edu/comp/150GA/homeworks/hw1/Johnson%2075.PDF
- Bounded simple cycles: https://arxiv.org/pdf/2105.10094

### How it maps onto sprf rules / store

- The scope/stack graph is per-file facts → fits the existing per-file
  fact-store / DD collection model and SQLite spill (DD memory-tiering skill).
- Nodes/edges are just more cursor rows; resolution paths are a recursive rule
  (the antijoin/retraction machinery already handles "edge gone → path retracts
  → dotted-path projection updates", which is the cross-rev staleness watcher).
- Dotted-path projection = a recursive rule over the edge relation; cycle
  cut = a `where` against the on-path SCC set. This is the same
  "type graph projected as a tree, query it with CSS/dotted paths" idea already
  in the codex-session note — the type graph is just one more projection rules
  target and the LSP/map renders.

---

## Recommendation (synthesis)

1. Port the **scope-graph + stack-graph** core (Néron/Visser model, stack-graph
   path-stitching specialization) into the v4 store. Do not depend on the
   archived `github/stack-graphs` crate.
2. Author tree-sitter-graph rules for **Rust, Go, Kotlin** ourselves (none
   upstream). Start with imports/modules/top-level defs (tier-2 graph), then add
   lexical scope + shadowing (tier-3).
3. Adopt the **SCIP descriptor string** as the cross-repo identity key.
   Synthesize it from CST. **Run no compilers.**
4. Tier-2 tree-sitter symbol+name-equality as the always-available fallback.
5. **Tarjan first** before any dotted-path projection; **Johnson** only to
   report circuits. Cut edges become the "circle here" markers in the
   folder-with-dots printout, which is the backbone for the later visual UI.
6. Explicitly out of scope (the cursed tier-4): generics/trait resolution,
   macro-expanded names, dataflow. Accept the same imprecision v0 accepted on
   string links; mark those edges low-confidence rather than resolving them.

---

## Prior-art verdict: has anyone built the polyglot version

Survey 2026-05-16. Short answer: **nobody but GitHub, and they archived it.**

| Finding | Detail |
|---|---|
| Only polyglot tree-sitter + stack-graph engine ever shipped | `github/stack-graphs`. Archived 2025-09-09, no maintained fork (~166 forks, 0 continuations). |
| Languages it ever got | JS, TS, Python, Java (+ out-of-tree Ruby at old 0.6). No Rust/Go/Kotlin/C/C++ rule file ever existed. |
| A 2026 team hit the same wall | Crader / sheeptechnologies RFC-001 evaluated stack-graphs, cited the archive + per-language rule-maintenance burden, walked away to heuristic imports + embeddings. |
| Closest living engine to copy | `metaborg/rust-scopegraphs` — the TU Delft group's own Rust scope-graph engine. MIT, active, "not production ready." Bare engine; you supply the tree-sitter front-end + per-language rules. |
| Frozen reference | Archived `github/stack-graphs` crates still build (checked out at `ext/stack-graphs`). Vendor for the algorithm, don't depend. |
| How real polyglot nav works in prod | Sourcegraph SCIP, Meta Glean+Glass: per-language compiler indexers linked by a common symbol-string format — the thing the no-compiler rule forbids. |
| Tree-sitter-only tools that exist | aider repomap / emerge / dep-tree / scip-syntax — extract-and-rank or import-edge, no scoped binding resolution, single-file at the resolution level. |

`rust-scopegraphs` / `stack-graphs` are **backends** (engine = graph store +
path search). The **frontend** is the per-language tree-sitter rules — that is
the work nobody did for systems languages, and the moat if we do.

aider repomap, for the record: tree-sitter `tags.scm` def/ref tags + NetworkX
personalized PageRank. It is **ranking, not resolution** — symbol-name string
match + graph centrality, no scope/shadowing/aliases. Popular because it is the
cheapest thing good enough to feed an LLM context window. Adopt its `tags.scm`
extraction as the tier-2 feeder and its PageRank as a "show hot nodes first"
signal for the visual map; it does not replace tier-3 stitching for the
correctness the dotted-path projection and the cross-rev staleness watcher need.

---

## Size of github/stack-graphs and the smallest version we need

Measured at `ext/stack-graphs`:

| crate / file | LOC | essential to us? |
|---|---|---|
| `tree-sitter-stack-graphs/src` | 8,858 | no — its own `.tsg` DSL interpreter + CLI; we emit via tree-sitter-graph |
| `stack-graphs/src` total | 11,779 | mostly product, not algorithm |
| ↳ `partial.rs` | 2,634 | no — incremental partial-path machinery; we get incremental free from `RUNTIME_DIRTY` + retraction |
| ↳ `graph.rs` | 1,682 | reference only — node/edge model is ~200 lines minimal |
| ↳ `c.rs` | 1,606 | no — C FFI, irrelevant in Rust |
| ↳ `stitching.rs` | 1,417 | no — DB-backed stitch; replaced by one recursive sprf rule |
| ↳ `arena.rs` | 1,139 | no — custom allocator for speed; use Vec/HashMap |
| ↳ `storage.rs` | 871 | no — its own SQLite; we have a store |
| ↳ `cycles.rs` | 406 | yes — cycle cutoff in path search, ~80 lines minimal |
| ↳ `paths.rs` | 96 | yes — the path struct |
| ↳ assert/viz/debug/stats | ~800 | no — test + tooling |

The algorithm is small. The product is big because of GitHub-scale
incrementality, its own arena+SQLite, C FFI, and a DSL interpreter — none of
which are ours. Smallest correct resolver, single symbol stack, ~100–150 lines:

```
resolve(ref_node) -> defs:
    stack = []                          # the symbol stack
    dfs(node, stack):
        push(s) node:  stack.push(s)
        pop(s)  node:  if stack.top != s { dead end } else stack.pop()
        scope / root:  walk through unchanged
        def node and stack empty:  emit node
    memo/visited keyed by (node, stack)  # cycles terminate
```

The single stack covers imports, exports, modules, lexical scope, fields,
methods — exactly tier-2/tier-3. The *second* stack (the "scope stack" in
`partial.rs`) only buys generic/parameterized indirection — skipped for v1, it
is the cursed tier-4 anyway. The genuinely hard, unavoidable work is writing
correct per-language tree-sitter rules, not the resolver.

---

## Coupling into the existing store (no separate graph engine)

The scope graph is more fact rows in the substrate we already have. Resolution
is **not** plain transitive closure — it is a stack walk (CFL-reachability).
We avoid running that walk live by precomputing per-file *partial paths* at
parse time (pure, rayon side, where tree-sitter already runs); each partial
path carries its required symbol-stack prefix as a column, which collapses the
stack discipline into a join key. The runtime then only joins + retracts.

```
  parse / extract (pure, rayon)        runtime graph (facts + retraction)     store
  ────────────────────────────         ──────────────────────────────────    ─────
  tree-sitter CST                       RUNTIME_NODE / RUNTIME_EDGE rows       per-file
  rust.tsg stanzas ─emit─► node/edge    recursive stitch rule (join, not PDA)  subgraph
  partial-path gen ─► partial_path      replace_supports ► VisibleDelta        + paths
       (only code that "knows" the stack discipline)        retract
```

Mapping onto current v4 (`runtime_graph.rs`, `store.rs`):

| scope-graph concept | existing substrate |
|---|---|
| scope / def / ref node | `RUNTIME_NODE` row, new `NodeKind` markers `ScopeNode` / `DefNode` / `RefNode` next to `Owner`/`Source`/`Row` (`runtime_graph.rs:54-89`) |
| P / M / push / pop edge | `RUNTIME_EDGE` row, `kind_id` = edge label (`runtime_graph.rs:20,30-33`) |
| per-file subgraph spill | SQLite-backed `FactStore` (`store.rs`) |
| binding retracts on file change | `replace_supports` → `VisibleDelta { retracted }` antijoin (`runtime_graph.rs:908-953`) |
| dirty re-resolve on edit | `RUNTIME_DIRTY` worklist (`runtime_graph.rs:21,262`) |
| dotted-path / cycle (Tarjan/Johnson) | same algorithm family as the rule-cycle pass in `compile/binding_graph.rs`; share code |

Sketch signatures:

```rust
// runtime_graph.rs — three more zero-size kind markers
pub struct ScopeNode;  impl NodeKind for ScopeNode { const TAG:&str="scope"; type Extra=(); }
pub struct DefNode;    impl NodeKind for DefNode   { const TAG:&str="def";   type Extra=(); }
pub struct RefNode;    impl NodeKind for RefNode   { const TAG:&str="ref";   type Extra=(); }

// parse side (pure): one partial path = a stitchable fragment
struct PartialPath {
    file: StringId,
    from: NodeId, to: NodeId,
    symbol_pre:  Vec<StringId>,   // symbol-stack required entering this fragment
    symbol_post: Vec<StringId>,   // symbol-stack left after it
    origin: Origin,               // see next section
}

// runtime side: the stitch is one recursive rule, finite (file-incremental),
// not arbitrary fixpoint:
//   resolved_binding(REF?, DEF?) :-
//     partial_path(REF?, MID?, pre=[], post=S?) ,
//     partial_path(MID?, DEF?, pre=S?, post=[])
```

The one capability this requires that is not yet in place: recursive sprf rule
support (bounded, file-incremental — easier than general Datalog recursion).
Tracked separately.

---

## Programmable amendment stack (L0–L4)

The auto-resolved graph is the first layer of a rule stack, not the final
answer. Each layer is rules over the layer below; output is always facts.

```
 L0  frontend (pure, rayon)   tree-sitter + per-lang rules ─► node/edge + partial_path facts
 L1  deterministic stitch     recursive system rule         ─► resolved_binding   origin=auto
 L2  amendments (user rules)  union L1 + cross-repo pattern  ─► resolved_binding   origin=user
                              pointers  repo($X)>rev($Y)>...
 L3  statistical (user rule)  where score > 0.8              ─► resolved_binding   origin=guess
 L4  user AST relations       ast(...) ─► node/edge facts, first-class beside L0
```

| vision feature | substrate that carries it | note |
|---|---|---|
| union auto facts, rule-calls-rule with amendments | rules-are-the-only-tables + `INSERT OR IGNORE` union | already in v4 |
| cross-repo pattern pointers (v0 `repo($X)>rev($Y)`) | ghcache + content-addressed IDs + SCIP-string join key | already planned |
| manual edits survive re-resolution | the `origin` column (below) | **must build** |
| statistical threshold | numeric `where score > t` predicate | forces the numeric-predicate feature flagged in human-goals |
| incremental on watch | `RUNTIME_DIRTY` + antijoin | already in v4 |
| stitch is recursive | bounded recursive rule support | the one missing capability |
| det + statistical edges in one graph the path query walks | confidence as an edge attribute; path rule threads a score-accumulation column (min/product) | path rule grows a score column |

### The `origin` column (replaces the word I was overusing)

Every fact row gets one small label column, `origin`, with values like
`auto` (machine-resolved by L1), `user` (you typed it in an L2 rule),
`guess` (statistical match from L3), `userast` (from an L4 AST rule).

When a file changes, recompute deletes-and-rebuilds **only the rows labelled
`auto`**. Rows you added by hand keep their label and survive untouched.
Without the label, recompute cannot tell machine edges from hand edges and
wipes both — manual tuning vanishes on the next file save. That is the entire
purpose of the column: each layer's recompute only ever touches its own rows.

Retraction is therefore scoped by `origin`: `replace_supports` for the L1
system rule filters to `origin = auto`; L2–L4 rows are retracted only by their
own owning rule being retracted, never by an upstream file edit.

---

## What we precisely reuse from ext/stack-graphs (dual MIT/Apache)

| file | LOC | reuse |
|---|---|---|
| `cycles.rs` | 406 | **lift directly** — cycle cutoff in path search, pure, no FFI/SQLite deps |
| `paths.rs` | 96 | **lift directly** — resolved-path struct + accumulation |
| `partial.rs` | 2,634 | **spec only** — the concat rule `A.symbol_post == B.symbol_pre`; this is what the stitch rule implements. Drop the arena impl |
| `graph.rs` | 1,682 | **spec only** — push/pop/scope/root semantics + precedence ordering for shadowing (~200 real lines) |
| `assert.rs` | 304 | **lift the test format** — `// ^ defined:` fixture DSL; lets us port their test corpus to validate our reimpl |
| `languages/*.tsg` (JS/TS/Py/Java) | — | **reference patterns** — no Rust/Go, but import/module/class shapes transfer |
| `c.rs` `arena.rs` `storage.rs` `stitching.rs` | 6,033 | **drop** — FFI, custom allocator, its own SQLite, DB-coupled stitch. Read `stitching.rs` once as fixpoint control-flow spec, then discard |
| `tree-sitter-stack-graphs/` (whole crate) | 8,858 | **drop** — `.tsg` DSL interpreter + CLI; we emit via our own walker |

~800 LOC liftable-or-spec out of ~20k. `LICENSE-MIT` + `LICENSE-APACHE` present
→ vendor with attribution.

## Substrate fit: one abstraction, already built

`v3/crates/effect_runtime/src/v2`. Two layers, no third graph abstraction
needed.

`FactStore<R>` (`fact_store.rs:66`, impls `MemFactStore` / `SqliteFactStore`):
`declare` · `insert`/`insert_batch` · `read_where(table,col,val)` ·
`rows_of`/`iter_table` (streaming) · `delete_matching(preds)` ·
`table_version` (cache token) · `commit(gen,bus)`. SQLite spill = the
`SqliteFactStore` backing, nothing extra.

`FactRuntimeGraph` (`runtime_graph.rs`) already a graph + bounded fixpoint:

| need | existing API | line |
|---|---|---|
| scope/def/ref node, P/M/push/pop edge | `insert_node` / `insert_edge` / `edges_where` / `delete_edge` | `:510,548,563,590` |
| the stitch fixpoint itself | `sweep_to_quiescence(max_iters, TraversalOrder::DepthFirst, handler)` | `:718` |
| incremental on file edit | `mark_dirty` / `dirty_owners` / `clear_dirty` | `:642,665,688` |
| retraction → delta, auto rows only | `replace_supports` → `VisibleDelta{retracted}` | `:870,295` |
| resumable mid-stitch | `record_continuation` / `dispatch_wake` | `:809,848` |

The stitch is: insert `partial_path` facts → `mark_dirty` →
`sweep_to_quiescence(DepthFirst)` with a handler that concatenates matching
partial paths → `replace_supports` to retract on change. The recursion driver
exists; the symbol stack rides as data on the rows (`symbol_pre`/`symbol_post`
columns), not as engine state, so the DFS sweep never needs to know about
stacks. The "missing capability" is only the language-level recursive-rule
*surface* — the runtime fixpoint is built.

One schema constraint the substrate imposes: `read_where` is single-column
equality, and the stitch self-join is `symbol_post == symbol_pre` over a
serialized stack. Store a **stack hash as an indexed column** so the join is an
equality lookup, not a table scan. That is the only requirement; no second
graph layer.
