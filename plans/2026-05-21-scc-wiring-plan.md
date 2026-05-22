# SCC-condensed `reaches` wired into the v5 engine

Date: 2026-05-21. Branch: `feat/v5-dl-engine`. Scope: callgraph example only, additive.

## Goal

Replace the recursive `INSERT` fixpoint for a transitive-closure rule with an
SCC-condensed form that never materializes the Θ(V²) pair table. Kernel warm
tick on `reaches` fell from ~197s (E1) to 1.4ms standalone (E4); fold that path
into the engine so `reaches` is one engine path, not a separate proof binary.

## Decisions

- **No persistent symbol interning.** SCC interns `String→u32` transiently per
  rebuild (O(edges), inside the 1.4ms). Persisting it adds an N+1 on every source
  insert, an orphan-GC burden, and autoincrement ids that are *not* content-stable
  (breaks cross-repo). Deferred behind E7 (corpus-scale RSS measurement).
- **Reuse the git blob OID for the file grain only** (already the change-detection
  key). It does not id sub-file nodes; not used as a node id.
- **Explicit closure form, no auto-detection.** A rule `reaches(a,b) <- closure(calls).`
  marks `reaches` as the transitive closure of edge relation `calls`.
- **`reaches` is a SQL VIEW** over the condensation, so existing `lower_query` /
  `lower_rule` (anti-joins) consume it unchanged. E6 measures the scan cost.

## Type signatures

```rust
// src/scc.rs  (pure graph, no SQL)
pub fn tarjan(adj: &[Vec<u32>]) -> (Vec<u32>, usize);
pub struct Cond { pub comp, pub ncomp, pub size, pub cyclic, pub cadj, pub members }
pub fn build_condensed(adj: &[Vec<u32>]) -> Cond;
pub fn reaches_from(c: &Cond, start: u32) -> Vec<u32>;   // seeded BFS (LSP point path)
pub fn count_pairs(c: &Cond) -> u128;                    // stats / tests

// src/ast.rs
enum BodyItem { ...; Closure { rel: String } }
impl Rule { pub fn closure_edge(&self) -> Option<&str>; } // Some(edge) iff body == [Closure]

// src/engine.rs
fn closure_map(rules: &[&Rule]) -> HashMap<String,String>;     // head -> edge
fn declare_closure(&mut self, d: &RelDecl, edge: &str) -> Result<()>;
fn load_edges(&self, edge, c0, c1) -> Result<(Vec<Vec<u32>>, Vec<String>)>;
fn rebuild_closures(&self, edges: &[&str]) -> Result<()>;
fn any_closure_empty(&self, edges: &[&str]) -> Result<bool>;
```

## Storage layout (per edge relation R, keyed by edge not head)

```
scc_node_R(name TEXT PRIMARY KEY, comp INTEGER, cyclic INTEGER)   -- node -> component (+ self-reach flag)
scc_edge_R(comp_src INTEGER, comp_dst INTEGER, PRIMARY KEY(comp_src, comp_dst))  -- condensed DAG, deduped
VIEW rel_<head>(c0, c1)                                           -- recursive CTE + cyclic self-reach
```

`cyclic` denormalized onto the node so the view needs no `scc_meta`. `size`/`members`
live only in the in-memory `Cond`. 2 tables + 1 view per edge relation; that N is
inherent (one condensation per graph), the schema is fixed.

## Read/write sequence (tick, derived step)

```
gate = source_changed || any_derived_empty(derived_rels) || any_closure_empty(edges)
if gate:
    rebuild_derived   : DELETE derived tables; fixpoint over non-closure derived rules   # builds rel_calls
    rebuild_closures  : for each edge: load_edges(rel_calls) -> adj
                        build_condensed
                        BEGIN; DELETE scc_node_R/scc_edge_R; bulk INSERT; COMMIT
queries run after (rel_reaches view resolves over fresh scc tables)
```

Closure rules are excluded from `derived_rules`/`derived_rels` (not lowered via
`lower_rule`). Same path mirrored in `tick_paths` for `--watch`.

## Uniqueness / invariants

- `scc_node.name` PK = node identity within one edge graph.
- `scc_edge` PK `(comp_src, comp_dst)` makes condensed dedup free.
- VIEW = cross-component reach (recursive CTE) UNION same-cyclic-component pairs.
- Replacement is wholesale per tick (single-repo). Cross-repo IncSCC deferred.
- **Stratification gap (out of scope):** a derived *rule* consuming `reaches`
  won't see fresh closure in the same tick (SCC rebuilds after the fixpoint).
  Queries on `reaches` are fine (run last). Callgraph example consumes only `calls`.

## Experiments / tests

| id | question | bar |
|---|---|---|
| wiring | engine `reaches` == old recursive `reaches`, end to end | full row-set agreement |
| E6 | does the view re-incur Θ(V²) for full-scan / anti-join consumers? point query seeded? | point µs; decide guard if scan blows up |
| E5 (later) | AST-resolved sparser `calls` shrinks megablob SCC | ncomp↑, max-comp↓ |
| E7 (later) | persistent content-id interning lowers corpus RSS | RSS drop > overhead |

Engine-level tests: parity, incremental split, incremental merge, cycle/self-loop,
sink/source, degenerate (empty/single/disconnected), point-vs-scan parity, anti-join.

## Decision gate at E6

If SQLite can't seed the recursive CTE for point queries, fall back to (a) a
tick-time materialized `rel_reaches` refreshed only when a rule consumes it
unfiltered, or (b) Rust-parameterized `reaches_from` for point queries with the
view reserved for anti-join composition.
