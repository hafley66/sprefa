# 6. Graph cores

**The question:** [chapter 5](05-evaluation.md) computes the full `reaches` relation by
fixpoint, which on a cyclic graph is `Θ(V²)` pairs. The engine avoids ever building
that table. The trick is structural: collapse each cycle to one super-node, leaving a
DAG, and answer reachability on the DAG instead. This chapter is the graph algorithms
that make that work, Tarjan's SCC at the center, plus the seeded forward/reverse walks
the engine actually runs, and what breaks when the graph is too big for a single DFS.

Running example unchanged: `main → run → parse → lex`, `lex → run`, `run → log`,
`helper` dead. Its cycle is `{run, parse, lex}`.

## 6.1 Tarjan's SCC: one DFS, one invariant

A **strongly connected component** (SCC) is a maximal set of nodes all mutually
reachable. Tarjan finds every SCC in a single depth-first search by tracking two
numbers per node:[^cite-tarjan]

| Field      | Meaning                                                          |
| ---------- | --------------------------------------------------------------- |
| `index[v]` | the order `v` was first visited (a timestamp)                   |
| `low[v]`   | the smallest `index` reachable from `v` while staying on the stack |

The one invariant that names an SCC:

```
   low[v] == index[v]   ⇒   v is the root of an SCC
                            pop the DFS stack down to v; those nodes are one component
```

`v` is an SCC root when it cannot climb back to anything visited earlier (its `low`
never dropped below its own `index`). Everything pushed after `v` and still on the
stack is reachable from `v` and reaches `v`, so it is one component. One DFS touches
each node and edge once: **O(V + E)**.

```ts
// Tarjan sketch. `low[v]` drops to the earliest on-stack node v can reach; when it
// never dropped below index[v], v is an SCC root and the stack unwinds to it.
let idx = 0; const index = new Map(), low = new Map(), onStack = new Set(), stack = [];
const comp = new Map(); let nComp = 0;
function dfs(v: string, adj: Map<string, string[]>) {
  index.set(v, idx); low.set(v, idx); idx++; stack.push(v); onStack.add(v);
  for (const w of adj.get(v) ?? []) {
    if (!index.has(w)) { dfs(w, adj); low.set(v, Math.min(low.get(v), low.get(w))); }
    else if (onStack.has(w)) { low.set(v, Math.min(low.get(v), index.get(w))); }
  }
  if (low.get(v) === index.get(v)) {              // v is an SCC root
    let w; do { w = stack.pop(); onStack.delete(w); comp.set(w, nComp); } while (w !== v);
    nComp++;                                       // popped group is one component
  }
}
```

## 6.2 Condensation: a DAG of components

Map each node to its SCC id, keep an edge between two components when an original edge
crossed them, and you have the **condensation**: every SCC is one super-node, and the
result is acyclic by construction (any cycle would have merged into a single SCC).

```
   original:  main → [run ⇄ parse ⇄ lex] → log        helper (isolated)
   condensed: [main] → [run,parse,lex] → [log]         [helper]
```

On a DAG, reachability is a topological-order walk with no thrashing: visit components
in reverse topological order and union each successor's reach-set into the current
one. The engine counts the full closure this way without ever materializing the
`Θ(V²)` pair table, because the count of an SCC's internal pairs is just `size²` and
cross-component reach is read off the DAG.

## 6.3 Seeded forward / reverse reachability

A *point query* pins one end and walks, instead of computing all pairs:

- `reaches_from(seed)`: BFS *out* along condensed out-edges, then expand each reached
  component to its member nodes. Answers "what does `run` reach?"
- `reached_by(seed)`: BFS *in* along the condensed *reversed* edges. Answers "who
  reaches `log`?"

Reversing a DAG's edges leaves its SCCs unchanged (a cycle reversed is still a cycle
through the same nodes), so the reverse walk is the identical algorithm over the
reversed adjacency. The seeded walk touches only what the seed connects to, which is
why it beats a recursive view that builds the whole closure then filters.

```ts
// Forward seeded reachability over the condensation: BFS out-edges from the seed's
// component, then expand reached components to member nodes. `reaches_from` mirror is
// the same loop over the REVERSED condensed edges.
function reachesFrom(cond: Cond, seed: string): string[] {
  const c0 = cond.comp.get(seed)!;
  const seen = new Set<number>(); const q = [...cond.cadj[c0]];   // out-edges of seed's comp
  while (q.length) {
    const c = q.shift()!;
    if (seen.has(c)) continue; seen.add(c);
    for (const s of cond.cadj[c]) if (!seen.has(s)) q.push(s);
  }
  if (cond.cyclic[c0]) seen.add(c0);                // self-reach via the cycle
  return [...seen].flatMap(c => cond.members[c]);   // expand comps to member nodes
}
```

## 6.4 When DFS breaks at scale

Tarjan is `O(V + E)` and unbeatable on a graph that fits in RAM as adjacency lists. It
breaks down on three axes:

| Axis            | Why DFS struggles                                                       |
| --------------- | ----------------------------------------------------------------------- |
| RAM             | DFS needs the whole graph (and a stack of depth up to `V`) resident; there is no I/O-efficient external-memory DFS the way there is for sorting or BFS |
| parallelism     | DFS is inherently sequential: the visit order is a strict chain, hard to split across cores |
| incrementality  | one edge change can in principle restructure SCCs; plain Tarjan recomputes from scratch |

The literature routes around each:

- **2-hop / pruned landmark labeling (PLL).** Precompute for each node a small label
  set so any reachability/distance query is answered by intersecting two labels, no
  walk at query time. Pruned Landmark Labeling makes the labels small in
  practice.[^cite-pll] This trades a heavier build for `O(label)` queries, the move
  when queries vastly outnumber edits.
- **Parallel SCC.** Replace DFS with *trim* (peel off trivial in/out-degree-0 nodes),
  then **forward-backward**: pick a pivot, intersect its forward and backward reachable
  sets to carve out one SCC, recurse on the three remaining regions in parallel.
  Coloring-based variants propagate the max-id reachable to label components without a
  global DFS.[^cite-fwbw][^cite-color]

The engine sits at the "fits in RAM, rebuild per tick" point, so it runs plain Tarjan.
Knowing the alternatives is knowing which axis you would be forced onto first.

**Intuition:** members of a cycle reach the same things, so collapse each cycle to one
node, reachability on the resulting DAG is a seeded walk, and the heavy machinery (2-hop
labels, parallel SCC) only matters once one DFS no longer fits.

## In your engine

All of this is `../../src/scc.rs`, pure graph code with no SQL or engine types:

- `tarjan`: iterative (explicit work-stack, not recursion) Tarjan with the
  `low[v] == index[v]` root test of §6.1; returns `(comp, ncomp)`.
- `build_condensed`: §6.2's builder for `Cond { comp, size, cyclic, cadj, cadj_rev, members }`,
  the condensed DAG plus its reverse, deduped.
- `reaches_from` / `reached_by`: §6.3's seeded forward/reverse BFS over `cadj` /
  `cadj_rev`, expanding reached components to member nodes.
- `count_pairs`: §6.2's closure count over the condensation (`size²` for cyclic
  components, reverse-topo union for cross-component), never building the pair table.

The parent book's §7.2–7.3 wires these into `run_query` for src/dst-pinned closure
queries.

## Exercises

1. **Tarjan trace.** Run Tarjan on the running example from `main`. Give `index` and
   `low` for each node and the moment each SCC pops. Confirm the four SCCs are
   `{main}`, `{run, parse, lex}`, `{log}`, `{helper}`.

2. **Why reverse is free.** Argue that reversing every edge leaves the SCC partition
   unchanged but flips the condensed DAG. What does that buy `reached_by`?

3. **Closure count without the table.** On the condensation
   `[main] → [run,parse,lex] → [log]`, `[helper]` isolated, compute the total number of
   `reaches` pairs using `size²` for the cyclic component plus cross-component reach.
   Check it against the parent book's chapter 7 exercise 4.

4. **Which axis first.** Suppose the call graph grows to 50M edges and queries are rare
   but edits are constant. Which of §6.4's three axes bites first, and which technique
   (2-hop, parallel SCC, incremental) does *not* help here? (Forward-ref
   [chapter 7](07-incremental.md).)

## Answers

1. `index/low`: `main 0/0`, `run 1/1`, `parse 2/1`, `lex 3/1`, `log 4/4`,
   `helper 5/5`. The back-edge `lex → run` pulls `low` of `lex`, `parse`, `run` all down
   to `1`. `log` pops first (`4 == 4`), then `run` is the root (`1 == 1`) and pops
   `{run, parse, lex}`, then `main` (`0 == 0`), then `helper` (`5 == 5`). SCCs:
   `{log}`, `{run, parse, lex}`, `{main}`, `{helper}`.

2. A directed cycle `a→b→c→a` reversed is `a→c→b→a`, still a cycle through the same
   nodes, so "mutually reachable" is symmetric under reversal and the SCC partition is
   identical. Only the edges *between* components flip. That is what lets `reached_by`
   reuse the exact `reaches_from` loop over `cadj_rev`: same components, edges reversed.

3. Cyclic component `{run, parse, lex}` has `size = 3`, so `3² = 9` internal pairs
   (including self-reach). Cross-component reach: `[main]` reaches `{run,parse,lex}` (3)
   and `{log}` (1) = 4; `[run,parse,lex]` reaches `{log}` (1) × size 3 = 3; `[log]`
   reaches nothing; `[helper]` nothing. Plus `[main]→[main]` is not a self-reach (size-1,
   acyclic). Total = `9 (internal) + 1·4 (main out) + 3·1 (cycle→log) = 16` reaches
   pairs, matching chapter 7's hand count of who-reaches-whom.

4. Incrementality bites first: constant edits with plain Tarjan means recomputing the
   whole SCC structure every edit. 2-hop labeling does *not* help here, it makes the
   build *heavier* to make queries cheap, the wrong trade when edits dominate and
   queries are rare. Parallel SCC speeds the rebuild but still rebuilds. The actual fix
   is incremental SCC maintenance ([chapter 7](07-incremental.md)).

## Citations

- Tarjan, R. *Depth-First Search and Linear Graph Algorithms.* SIAM J. Comput. 1(2),
  1972. The original `index`/`low` SCC algorithm; §6.1 is this paper.[^cite-tarjan]
- Fleischer, L., Hendrickson, B., Pınar, A. *On Identifying Strongly Connected
  Components in Parallel.* IPDPS 2000. The forward-backward divide-and-conquer SCC that
  replaces sequential DFS.[^cite-fwbw]
- Hong, S., Rodia, N. C., Olukotun, K. *On Fast Parallel Detection of Strongly Connected
  Components (SCC) in Small-World Graphs.* SC 2013. Trim + coloring + forward-backward
  tuned for real graphs.[^cite-color]
- Akiba, T., Iwata, Y., Yoshida, Y. *Fast Exact Shortest-Path Distance Queries on Large
  Networks by Pruned Landmark Labeling.* SIGMOD 2013. The 2-hop label scheme of
  §6.4.[^cite-pll]

[^cite-tarjan]: Tarjan, *Depth-First Search and Linear Graph Algorithms*, SIAM J.
Comput. 1(2), 1972.
[^cite-fwbw]: Fleischer, Hendrickson, Pınar, *On Identifying Strongly Connected
Components in Parallel*, IPDPS 2000.
[^cite-color]: Hong, Rodia, Olukotun, *On Fast Parallel Detection of Strongly Connected
Components in Small-World Graphs*, SC 2013.
[^cite-pll]: Akiba, Iwata, Yoshida, *Fast Exact Shortest-Path Distance Queries on Large
Networks by Pruned Landmark Labeling*, SIGMOD 2013.
