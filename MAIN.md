# sprefa — MAIN

> A four-layer pattern-flow language with React-shaped queues, prolog-shaped
> facts, and tree-shaped data. Parses bytes, runs cursor streams through
> ops, accumulates facts into per-tick generations, fires effects post-seal.

## Mental model in five lines

1. Everything is a tree (json/jq/lines/graphs all project to trees with different delimiters).
2. Everything is a cursor (bytes + named captures + byte_range, flowing through ops).
3. Everything is two queues (render = current generation accumulates; flush = post-seal anti-join + effects fire).
4. Everything is layered (DSLs → pipes → facts → generations; cross-layer composition statically rejected).
5. Everything is dogfoodable (next move: write this diagram in sprf and have it stay synced).

## The goofy-ass system diagram

```
                      ╭─────────────────────────────────╮
                      │  sprf source on disk / buffer   │
                      ╰────────────────┬────────────────╯
                                       │ bytes
                                       ▼
            ┌──────────────────────────────────────────────┐
            │             tree-sitter-sprefa               │ ◄── grammar.js
            │     (host grammar + injected sub-grammars)   │     parser.c
            │                                              │
            │   pipe   fork   op_invocation   paren_slot   │
            │   atom   str    term_ref        brace_block  │
            └────────────────────┬─────────────────────────┘
                                 │ CST
                                 ▼
            ┌──────────────────────────────────────────────┐
            │              sprefa_parse                    │
            │     parse.rs : CST + injected-tree parse     │
            │     ast.rs   : sprf surface AST              │
            └────────────────────┬─────────────────────────┘
                                 │ AST (Pipe / Fork / OpInvocation)
                                 ▼
            ┌──────────────────────────────────────────────┐
            │              pipeline::lower                 │
            │                                              │
            │  Two-pass walk:                              │
            │   1. collect rule defs / known names         │
            │   2. lower each pipe → Pipeline { Op | Seq | │
            │      Fork(arms) }                            │
            │                                              │
            │  Static checks:                              │
            │   • binding graph (Read/Unbound modes)       │
            │   • tag stratification (writers > readers)   │  ◄── target
            │   • event subscribe-before-publish           │  ◄── target
            │   • DSL domain lattice (tree>json>text)      │  ◄── target
            └────────────────────┬─────────────────────────┘
                                 │ Pipeline (Arc<dyn Op> chain)
                                 ▼
   ╔════════════════════════════════════════════════════════╗
   ║                  pipeline::runner                      ║
   ║                                                        ║
   ║   ╭────────────╮         ╭────────────╮                ║
   ║   │ seed source│───►─────│ pipe_flat_ │───►── batches  ║
   ║   ╰────────────╯         │   map()    │      of cursors║
   ║      Stream<Cursor>       ╰────────────╯               ║
   ║                                                        ║
   ║   each Op: pipe(ctx, batch) → BoxFuture<batch'>        ║
   ║            pipe_flat_map(ctx, upstream) overrides for  ║
   ║            streaming subjects                          ║
   ║                                                        ║
   ║              ┌─────────── Cursor ────────────┐         ║
   ║              │  content     Arc<[u8]>        │         ║
   ║              │  byte_range  Range<usize>     │         ║
   ║              │  captures    HashMap<...>     │ ◄── target shape  ║
   ║              │                  REPO   ─┐    │  (post-collapse,  ║
   ║              │                  REV    ─┼─►  │   sprefa-4m7.4.12)║
   ║              │                  FS     ─┘    │         ║
   ║              │  last_bound  Option<Arc<str>> │         ║
   ║              │  slots       HashMap<TypeId>  │         ║
   ║              └───────────────────────────────┘         ║
   ╚═══════════════╤═══════════════════════════════╤════════╝
                   │ pure ops yield Effect descs   │ writes / reads
                   ▼                               ▼
       ┌─────────────────────────┐    ┌────────────────────────────┐
       │     effect_runtime      │    │      relation_store        │
       │                         │    │                            │
       │   RtCtx                 │    │   bags: HashMap<name, Bag> │
       │   ├── stores            │    │                            │
       │   ├── domains           │    │   Bag {                    │
       │   └── batchers          │    │     rows: Vec<Row>,        │
       │                         │    │     keyed: ...waiters,     │
       │   PureEffect:           │    │     broadcast: ...waiters  │
       │     blake3 cache        │    │     open_writers: counter  │  ◄── target RAII
       │     register_pure       │    │     gen: u64               │  ◄── target generations
       │                         │    │   }                        │
       │   Effects:              │    │                            │
       │     ReadBytes           │    │   SubjectRegistry<Wake>    │
       │     FsListFiles         │    │   (parking + waker keys)   │
       │     WriteRange          │    │                            │
       │     WriteFile           │    │  fact (= tag)  ◄── rename  │
       │     Print               │    │                            │
       │     Sh                  │    │                            │
       └─────────────┬───────────┘    └─────────────┬──────────────┘
                     │                              │
                     ▼                              ▼
            ┌────────────────────────────────────────────────┐
            │                end-of-tick                     │
            │                                                │
            │  Render queue — facts accumulating via         │
            │  tag/fact writes (RAII open_writers > 0)       │
            │                                                │
            │            ▼  last writer frame drops          │
            │                                                │
            │  Flush queue — generation seals:               │
            │   • anti-join readers wake, evaluate           │
            │   • tag_diff fires per (added, removed)        │
            │   • lsp[severity] diags emit                   │
            │   • write splices land (per-WriteTarget agg)   │
            │   • staged sh approvals resolve                │
            ╰────────────────────────┬───────────────────────╯
                                     │
                                     ▼
                       ┌────────────────────────────┐
                       │       server crate         │
                       │                            │
                       │  bin/sprefa-run  (CLI)     │
                       │  bin/sprefa-lsp  (LSP)     │  ──► VS Code client
                       │  bin/sprefa-server (daemon)│
                       │                            │
                       │  backend.rs: tower-lsp     │
                       │  session.rs: doc lifecycle │
                       │  run.rs:     per-seed loop │
                       │  config.rs:  sprefa.toml   │
                       └────────────┬───────────────┘
                                    │
                                    ▼
                ┌────────────────────────────────────┐
                │       file watcher / OS events     │
                │                                    │
                │   FS change → invalidate fs cache  │
                │              → spawn next tick     │
                │                                    │
                │   The daemon is the tick scheduler.│
                │   Cross-tick is NOT a language     │
                │   primitive — it's daemon policy.  │
                └────────────────────────────────────┘
```

## Crate-by-crate

| crate | role | key types |
|---|---|---|
| `tree-sitter-sprefa` | host grammar; emits `parser.c` checked in per locked phase | grammar.js |
| `sprefa_parse` | bytes → CST + injected sub-trees + sprf-AST | `Pipe`, `Fork`, `OpInvocation`, `ParenSlot` |
| `sprefa_macros` | proc macros for op_ctor / language registration | derive helpers |
| `effect_runtime` | redux-saga shaped dispatch + caching + batching | `RtCtx`, `PureEffect`, `Batcher`, `RtCtxBuilder`, `SubjectRegistry`, `CancellationToken` |
| `pipeline` | core engine — ops, cursors, runner, relation store, lower | `Cursor`, `Op`, `Pipeline`, `RelationStore`, `Capture`, `Value::Term`, all op impls |
| `spine` | analysis layer — DAG, extraction, jq paths, init cursors | `Analysis`, `DAG`, `JqPath`, `Extractor` |
| `sprefa` | bridge crate, lifted from v2; reader/writer surfaces | `Reader`, `Writer`, `Mutations` |
| `server` | LSP backend (tower-lsp) + sprefa-run CLI + sprefa-lsp / sprefa-server bins | `Backend`, `Session`, `RunEvent` |

The pipeline crate is the big one. It contains:

- `_0_cursor.rs` — Cursor primitive (target collapse: addressing→captures, Vec→HashMap, see card sprefa-4m7.4.12)
- `_1_op.rs` — Op trait (pipe, pipe_flat_map, try_raw_regex, materialize_with)
- `_2_pipeline.rs` — Pipeline { Op | Seq | Fork } + runner
- `lower.rs` — AST → Pipeline two-pass walk
- `binding_graph.rs` — Read/Unbound TermMode static analysis
- `value.rs` — Value enum, Seg template, materialize_template
- `pattern_op.rs` — pattern op trait (re/glob/ast/json/comment all implement)
- `relation_store.rs` — fact bag + waker registry (rename target: tag → fact)
- `effects.rs` — Read/Write/List/Print/Sh effect kinds + batchers
- `registry.rs` — string-shape → Box<dyn Op> factory
- `ops/` — one folder per op: re/, glob/, ast_grep.rs, json.rs, comment.rs, repo/, rev/, fs/, tag.rs, rule.rs, sh.rs, render.rs, write_cursor.rs, write_file.rs, lsp.rs, print.rs, read.rs, str_op.rs, void.rs, cursor_ref.rs, relation/
- `readers/` — file source layer (DiskFileSource, GitFileSource, BufferOverlay)
- `fs_watcher.rs` — debounced OS watcher
- `disk_cache.rs` — L2 SQLite OpCache
- `cache_key.rs` — blake3 fingerprinting

## The four layers

```
   Layer 4   GENERATIONS / DIFF / TIME-TRAVEL
             │  tag@gen(:r, N, ...)   tag_diff(:r, N, M, ...)
             │  daemon owns tick scheduling; language owns intra-tick
   ───────────────────────────────────────────────────────────────
   Layer 3   FACTS / RULES / RELATIONS                  ◄── stratified
             │  tag(:r, ...)  rule(:foo, ${A?}, ...) { body }
             │  !tag?(:r, ...)  RAII auto-seal on writer drop
   ───────────────────────────────────────────────────────────────
   Layer 2   PIPE OPS                                   ◄── stdin/stdout
             │  source > pattern > effect
             │  > sequence  ; fork  {} brace-block sub-pipe
   ───────────────────────────────────────────────────────────────
   Layer 1   DSLs                                       ◄── domain lattice
             │  re   glob   ast[lang]   json   comment   marker
             │  carveout ${...} re-entry; tree>json>text
```

Cross-layer composition statically rejected at lower-time:
- `pattern/domain-mismatch` — text DSL embedding tree DSL.
- `pattern/category-mismatch` — pattern carveout containing rule call or relation read.
- `tag/negation-cycle` — anti-join's transitive producers depend on its frame.
- `rule/recursive` — self or mutual rule recursion (v0).

## Tick lifecycle (the React analog)

```
   tick spawn (CLI invocation, LSP request, watcher fire)
       │
       ▼
   ┌───────────────────────────────────────────────────────┐
   │  RENDER PHASE (open generation N)                     │
   │                                                       │
   │   seed_upstream emits root cursor(s)                  │
   │   ↓                                                   │
   │   pipeline drains: ops fan-out, captures bind,        │
   │   tag writes accumulate, rule frames hold writer-     │
   │   shares on their potential-write fact bags           │
   │                                                       │
   │   side-effecting ops (sh, write_*, lsp[…]) STAGE      │
   │   their effects (don't fire yet — gated on seal)      │
   └────────────────────────┬──────────────────────────────┘
                            │  last rule frame drops
                            ▼
   ┌───────────────────────────────────────────────────────┐
   │  FLUSH PHASE (gen N seals → gen N+1 ready)            │
   │                                                       │
   │   • Tag bags with open_writers→0 fire seal-waiters    │
   │   • Anti-join (!tag?) readers evaluate; emit cursors  │
   │   • tag_diff fires per (gen N-1 → N) row delta        │
   │   • Diagnostics (lsp[…]) flush to LSP client          │
   │   • Write splices aggregate per (repo,rev,fs);        │
   │     right-to-left splice; cache invalidate            │
   │   • Staged sh approvals resolve / yield to LSP click  │
   │   • Generation N's row set becomes immutable history  │
   └────────────────────────┬──────────────────────────────┘
                            │
                            ▼
                    (await next tick spawn)
```

The crucial invariant: render-phase ops never fire user-visible effects. Effects stage. Flush-phase resolves them after the generation seals. Same shape as React's commit phase running effects only after render is fully committed.

## Where this is heading

Near-term (cleanup baseline + write architecture):

- Cursor collapse (sprefa-4m7.4.12 — captures HashMap, addressing-as-captures, source-op stdin/stdout).
- Delete RulePredicateOp + rule? path. Rule = call only.
- Convert JoinOp / ProbeOp from snapshot to drain+subscribe.
- Memoize RuleCallOp by (rule_name, hash(args)).
- WriteTarget {repo, rev, fs} effect surface; per-target aggregation; cross-rev auto-worktree.
- Render aggregation reducer mode (per-(fs, SCOPE) collapse before write).

Medium-term (the headline):

- RAII auto-seal on rule frame drop.
- Anti-join `!tag?(...)` parking on per-generation seal.
- Lower-time stratification analyzer (writers > readers, no negation cycles).
- DSL domain-lattice check at lower-time.
- proto-drift demo end-to-end in VS Code.

Long-term (research surface):

- Tag rename to `fact` (prolog-inspired; separates from cursor-annotation mental model).
- Generations as first-class: `fact@gen(:r, N, ...)`, `fact_diff(:r, N, M, ...)`.
- Event primitive (Subject, no replay; tick-scoped; lower-time subscribe-before-publish).
- Programmable LSP code-actions (`choice("title", :a, :b)` op + suspension).
- Cross-language entity addressing via sem-MCP.
- Cross-language type-graph normalization (the hardest one).
- Manifest pinning + cross-repo trigger (root.main advances → pinned children re-fire).
- SQLite-backed persistent fact bags + strings/refs denormalization.

The collapse vision (eventual):

- TERM (`${X}` / `${X?}`) is sugar for a `next(:TERM, &.content)` op call.
- Pattern op (`re(...)` / `glob(...)`) in arg position is sugar for source > pattern > sink.
- Rule-as-relation: rule rows live in fact bags; calling a rule = inserting + reading a fact.
- Pipes-as-values: a pipe expression evaluates to a Pipeline value, passable as op arg.
- Cut-form (`!op(...)`) marks an effect-with-yield-point; not pure projection but mutation requiring flush coordination.

That's where the language wants to go: minimum sugar, maximum composition, four primitive layers stratified by static check.

## Dogfood goal

This MAIN.md is data. Every crate's role is a fact. Every layer's composition rule is a fact. Every diagnostic code is a fact. The sprf source that GENERATES this diagram from the codebase (`tree-sitter-sprefa` grammar, `pipeline::ops` directory, `effects.rs` effect kinds, etc) is a near-term writeable demo: a `sprefa_self.sprf` that walks v3/, extracts crate/op/effect/diag rows into facts, renders them via aggregation, and writes back into MAIN.md's anchored regions.

When that lands, MAIN.md stays synced with the code by definition. The diagram becomes a fact graph. The drift detection that watches for "this MAIN.md describes an op that no longer exists" is the same anti-join machinery used for proto-drift.

That's the dogfood landmark. Until then this doc is hand-maintained.

## Naming notes (in flight)

- **tag → fact** (proposal). Prolog convention; separates from "annotation" mental model. Touches relation_store, ops/tag.rs, ops/relation/, parse.md, every fixture using `tag(...)`.
- **last_bound** stays as advisory. Possibly retire when scan-pointer ops formalize a typed PointerSlot.
- **`_`** RESERVED. Not yet documented; do not assign meaning.
- **carveout** is the canonical term for `${...}` parser re-entry. Use consistently.
