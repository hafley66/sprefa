# petgraph swap for scc.rs

Drop-in migration map from the hand-rolled `v5/src/scc.rs` to `petgraph`.
Source read: `~/projects/ext/petgraph` (workspace, crate `crates/petgraph` v0.8.3,
edition 2024, rust-version 1.91). All refs below are `crates/petgraph/src/...`.

## Crate version / features

| Item | Value |
| --- | --- |
| crate | `petgraph` `0.8.3` (`crates/petgraph/Cargo.toml`) |
| default features | `["std", "graphmap", "stable_graph", "matrix_graph"]` |
| needed for this swap | `graphmap` (for `DiGraphMap`) — already in default |
| algos used | `tarjan_scc`, `condensation`, `toposort`, `has_path_connecting`, `dijkstra` — all in `petgraph::algo`, no extra feature gate |
| NOT needed | `rayon`, `serde-1`, `unstable` |

So `petgraph = "0.8"` with default features is sufficient. Nothing here is feature-gated beyond `graphmap` (default).

Note: this checked-out copy is edition-2024 / rust 1.91. The published 0.8.x on crates.io has a lower MSRV; if sprefa's toolchain is older, pin to the published `0.8` rather than this workspace path.

## 1. `tarjan_scc` — `algo/scc/tarjan_scc.rs:269`

```rust
pub fn tarjan_scc<G>(g: G) -> Vec<Vec<G::NodeId>>
where
    G: IntoNodeIdentifiers + IntoNeighbors + NodeIndexable,
```

- Returns one `Vec<G::NodeId>` per SCC. For `&DiGraphMap<u32,()>`, `NodeId = u32`, so you get `Vec<Vec<u32>>` of the actual node ids (no index/id indirection).
- Order of SCCs: **postorder = reverse topological sort** (doc lines 50-51, 170-172). The first SCC in the output is a sink in the condensed DAG; the last is a source. sprefa's own `tarjan` assigns `comp` ids in the same finishing order (sinks get low ids), so the ordering convention matches if you enumerate the returned vec in order.
- Order of node ids *within* an SCC is arbitrary.
- Recursive impl (single pass, O(V+E)). The reusable form is `TarjanScc<N>` (`:16`) with `.run(g, |scc| ...)` (`:56`) if you want to avoid the per-call `Vec` allocation; `node_component_index(g, v)` (`:138`) gives the comp index of a node after a run.

To recover sprefa's `(comp: Vec<u32>, ncomp)` shape from `Vec<Vec<u32>>`: iterate the result, assign each SCC its enumeration index as the component id, scatter into `comp[node] = i`. `ncomp = result.len()`.

## 2. `condensation` — `algo/mod.rs:481`

```rust
pub fn condensation<N, E, Ty, Ix>(
    g: Graph<N, E, Ty, Ix>,
    make_acyclic: bool,
) -> Graph<Vec<N>, E, Ty, Ix>
where Ty: EdgeType, Ix: IndexType,
```

- Takes an owned **`Graph`** (the array-backed `graph_impl` type), **not** `GraphMap`. Returns a new `Graph` whose node weight is `Vec<N>` (the members of that SCC) and whose edges carry the original `E` weights.
- `make_acyclic` (lines 386-387, 509-514): `true` ⇒ self-loops and parallel edges are dropped (`update_edge`, skip `source==target`), giving a true DAG; `false` ⇒ keeps every original edge (`add_edge`), so multi-edges and intra-SCC self-loops survive.
- Node weights: merged — each condensed node weight is `Vec<N>` of the collapsed originals (`:495`, `:504`). Edge weights: kept per-edge (`:511`/`:514`); with `make_acyclic=true`, `update_edge` keeps the last weight written for a collapsed pair.
- Internally calls `kosaraju_scc(&g)` (`:489`), NOT `tarjan_scc`. So condensed node order follows kosaraju output order, which is **not** guaranteed identical to `tarjan_scc` order. Do not assume condensed `NodeIndex(i)` lines up with `tarjan_scc()[i]`.

Friction for sprefa: `condensation` needs a `Graph` and produces a `Graph<Vec<N>,...>` object — heavier than sprefa's `Cond` (plain `Vec<Vec<u32>>` adjacency). It is the wrong tool if you want to keep `count_pairs`/`reaches_from` operating on bare `Vec<Vec<u32>>`. Prefer building the condensed adjacency yourself from `tarjan_scc` output (cheap: map each edge's endpoints through `comp[]`, dedupe), which is exactly what sprefa's `build_condensed` already does.

## 3. toposort + reachability — `algo/mod.rs`

`toposort` — `:208`
```rust
pub fn toposort<G>(
    g: G,
    space: Option<&mut DfsSpace<G::NodeId, G::Map>>,
) -> Result<Vec<G::NodeId>, Cycle<G::NodeId>>
where G: IntoNeighborsDirected + IntoNodeIdentifiers + Visitable,
```
Returns nodes in topological order, or `Err(Cycle(node))` if not a DAG. Self-loops count as cycles. `space` is an optional reusable `DfsSpace` workspace.

`has_path_connecting` — `:366`
```rust
pub fn has_path_connecting<G>(
    g: G, from: G::NodeId, to: G::NodeId,
    space: Option<&mut DfsSpace<G::NodeId, G::Map>>,
) -> bool
where G: IntoNeighbors + Visitable,
```
DFS from `from`, returns whether `to` is reachable; `from==to` ⇒ `true` (lines 346-347). This is a single-pair boolean, O(V+E) per call. Wrong shape for sprefa's `reaches_from` (which wants the full reachable *set* in one pass).

`dijkstra` — `algo/dijkstra.rs:92`
```rust
pub fn dijkstra<G, F, K>(graph: G, start: G::NodeId, goal: Option<G::NodeId>, edge_cost: F)
    -> HashMap<G::NodeId, K>
where G: IntoEdges + Visitable, G::NodeId: Eq + Hash, F: FnMut(G::EdgeRef) -> K, K: Measure + Copy,
```
With `goal = None` and `edge_cost = |_| 1`, the returned `HashMap` keys ARE the set of nodes reachable from `start` (it visits the whole reachable subgraph). So `dijkstra(&g, start, None, |_| 1usize).into_keys()` is the closest stock equivalent to `reaches_from`'s set semantics — at the cost of a `HashMap` and a priority queue you do not need.

Best stock fit for the *reachable-set* shape is the visitor layer, not `algo::`:
- forward set from `start`: `petgraph::visit::Bfs::new(&g, start)` (or `Dfs`), drain `.next(&g)` into a `Vec`.
- reverse set into `target`: same `Bfs`/`Dfs` over `petgraph::visit::Reversed(&g)`, which flips edge direction without building a reversed copy. This replaces sprefa's hand-maintained `cadj_rev`.

`Reversed` is already used inside `toposort` (`:253`), so it is on the default path.

## 4. Graph type to feed them — `Graph` vs `DiGraphMap<u32,()>`

sprefa keys nodes by raw `u32`. Two candidates:

| | `Graph<(),(),Directed,u32>` | `DiGraphMap<u32,()>` |
| --- | --- | --- |
| node id type | `NodeIndex<u32>` (newtype, dense 0..n) | `u32` (your own ids, any value) |
| `to_index` | identity on the dense index (`graph_impl/mod.rs:2582`) | IndexMap hash lookup (`graphmap.rs:1248`) |
| ids stable across removals | no (swap-remove shifts) | yes (id is the key) |
| build from `&[Vec<u32>]` | must `add_node` n times, then map u32→NodeIndex on every edge | `DiGraphMap::from_edges(...)` or `add_edge(a,b,())` directly with your u32s |
| extra mapping table | yes (u32 ↔ NodeIndex) | none |
| tarjan_scc returns | `Vec<Vec<NodeIndex>>` (un-map to u32) | `Vec<Vec<u32>>` (already your ids) |
| `condensation` works on it | yes | no (condensation requires `Graph`) |

**Lowest friction: `DiGraphMap<u32,()>`.** Node ids stay your u32s end to end, no side table, and it implements every trait the four algos need: `Visitable` (`graphmap.rs:1166`), `IntoNodeIdentifiers` (`:1210`), `NodeIndexable` (`:1238`, `node_bound = node_count`), `IntoNeighbors` (`:1270`), `IntoNeighborsDirected` (`:1283`). All algo calls take `&DiGraphMap`.

Caveat on `NodeIndexable` for `DiGraphMap`: `to_index` is positional in the insertion-ordered IndexMap and `node_bound = node_count` (`:1244`), so node ids need NOT be contiguous or 0-based. `tarjan_scc` only uses `to_index` internally for its scratch array and returns real `NodeId`s, so this is safe. But if you mix in code that assumes `node id == index` (sprefa's current `comp` array is indexed by node-as-usize), you must go through `to_index`/`from_index` or rebuild a `Vec<u32>` keyed by your own id.

Build from sprefa's `adj: &[Vec<u32>]`:
```rust
use petgraph::prelude::DiGraphMap;
let mut g: DiGraphMap<u32, ()> = DiGraphMap::new();
for (u, succ) in adj.iter().enumerate() {
    let u = u as u32;
    g.add_node(u);                  // keep isolated nodes
    for &w in succ { g.add_edge(u, w, ()); }
}
```
`add_node` is idempotent (`graphmap.rs:291`, `entry().or_default()`); `add_edge` auto-creates endpoints. The explicit `add_node(u)` keeps zero-out-degree nodes that only appear as a source index. Since sprefa's `adj` is index-keyed (node `i` is row `i`), the resulting `DiGraphMap` uses `0..n` as ids and `to_index` is effectively identity in practice — but do not rely on that if the input ever becomes sparse.

If you later want `condensation`, you must instead build a `Graph<(),(),Directed,u32>` (`Graph::with_capacity`, `add_node`, `add_edge` over a u32→NodeIndex map). Given sprefa already wants bare adjacency out the other side, that round-trip is not worth it.

## Mapping table: scc.rs → petgraph

| scc.rs (`v5/src/scc.rs`) | petgraph replacement | notes |
| --- | --- | --- |
| `tarjan(adj) -> (Vec<u32> comp, usize ncomp)` | `algo::tarjan_scc(&g) -> Vec<Vec<u32>>` (`scc/tarjan_scc.rs:269`) | Rebuild `comp`/`ncomp` from the result: for each SCC index `i`, set `comp[node]=i`; `ncomp = sccs.len()`. SCC order is reverse-topo, matching sprefa's id convention. |
| `build_condensed(adj) -> Cond` | no single call; keep a thin builder | Use `tarjan_scc` for membership, then map each edge through `comp[]` + dedupe to get `cadj` (same loop sprefa has at `scc.rs:86-94`). `algo::condensation` is the wrong shape (wants/returns `Graph`, uses kosaraju order). |
| `reaches_from(c, start) -> Vec<u32>` | `visit::Bfs::new(&g, start)` drained to `Vec`, OR `algo::dijkstra(&g, start, None, \|_\| 1).into_keys()` | Direct over the original `g` skips the condensation entirely for a single query. To keep the SCC-collapsed speedup for many queries, BFS the condensed `cadj` (sprefa's current approach) — petgraph has no built-in "reach over condensation" helper. `has_path_connecting` (`algo/mod.rs:366`) only answers the boolean single-pair case. |
| `reached_by(c, target) -> Vec<u32>` | `visit::Bfs::new(visit::Reversed(&g), target)` drained to `Vec` | `Reversed` flips edges without a reversed copy, replacing the hand-built `cadj_rev`. Same single-vs-condensed tradeoff as above. |
| `count_pairs(c) -> u128` | **no petgraph equivalent — stays hand-rolled** | This is `\|transitive-closure pairs\|` weighted by SCC size (`scc.rs:103-141`): per-SCC bitset reach over the condensed DAG plus `size^2` for cyclic SCCs. petgraph 0.8.3 has no transitive-closure / reachable-pair-count primitive (`algo/tred.rs` is transitive *reduction*, not closure, and does not count). Keep `count_pairs` as is, fed by the locally-built condensed adjacency. |

### What `Cond` should wrap

Keep `Cond` as sprefa's own struct (`comp`, `ncomp`, `size`, `cyclic`, `cadj`, `cadj_rev`, `members`). Do not replace it with petgraph's condensed `Graph<Vec<N>,...>`:
- `count_pairs` needs the bare `cadj: Vec<Vec<u32>>` and per-comp `size`/`cyclic`; petgraph's condensed `Graph` gives `Vec<N>` weights but not `cyclic` and forces you back through `NodeIndex` to read adjacency.
- The only petgraph piece worth adopting inside `build_condensed` is swapping the hand-rolled iterative `tarjan` for `algo::tarjan_scc` to produce `comp`/`members`. Everything downstream (`size`, `cyclic`, `cadj`, `cadj_rev`) stays sprefa-built from `comp` + `adj`.
- `cadj_rev` could be dropped if `reached_by` switches to `Bfs` over `Reversed(&g)` on the original graph, but if you keep the condensed-DAG fast path for `reached_by`, keep `cadj_rev`.

### Net recommendation shape

Minimal swap: inside `build_condensed`, replace the `tarjan(adj)` call with a `DiGraphMap<u32,()>` build + `algo::tarjan_scc` + a comp/ncomp reconstruction; leave `count_pairs`, `reaches_from`, `reached_by`, and the `Cond` struct untouched. That removes the one hand-written graph algorithm (Tarjan) that petgraph covers exactly, and leaves the closure-counting logic petgraph cannot replace.
