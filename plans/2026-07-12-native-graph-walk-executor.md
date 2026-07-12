# Native graph-walk executor for recursive reachability rules

## The insight (why this exists)

A recursive datalog rule that walks a graph transitively — reachability, blast
radius, dependency closure, "what does X reach / reach X" — lowers to a **SQL
semi-naive fixpoint**: one JOIN+INSERT per BFS round, looped to convergence.
Statement count is **O(D)** where D = graph depth (number of rounds). On the
real repo `port_reach` ran **220 statements**, most of the 1.85s was SQLite
round-trips + index churn, NOT the graph work.

The same query as an **in-memory BFS** is O(graph): load adjacency once, walk
with a queue + visited set. On `port_reach` the walk itself is **5ms**; the rest
is the SQLite read/write boundary. The engine already had ONE native closure
path (`src/scc.rs`, Tarjan/SCC condensation) for the PURE `head <- closure(edge)`
form; this arc adds a second for the "seeded walk with a stop rule and/or a
depth lattice" forms that scc.rs can't express.

## What landed (2026-07-12)

`src/walk.rs` — `multi_source_halt_bfs`: tagged, halt-gated forward BFS.
`src/engine/derive.rs::try_native_halt_bfs` — recognizer that detects the shape
in a recursive rel-component and dispatches to walk.rs instead of the SQL
fixpoint. Recognized shape (port_reach):

    head(tag, node) <- seed(tag, start), edge(start, node).        # base
    head(tag, node) <- head(tag, mid), !halt(mid, _), edge(mid, node).

Discipline baked in (the three controllable-cost constraints):
- **RSS**: a `sym` graph loads into integer space (`load_edges_keyed`,
  HashMap<i64,u32> + Vec<i64>), NO per-node String cache. Output text renders in
  a few chunked `IN (...)` reads. The head insert STREAMS in 16k-row chunks —
  peak held is one chunk, never the whole result.
- **Statement count**: O(1) reads + O(rows/chunk) writes ≈ ~22 statements total
  (vs 220), independent of graph depth. No per-node query loop.
- **Parity**: row-identical to the SQL fixpoint (0/0 set-diff + 2 e2e tests).
  `DL_NO_HALT_BFS=1` forces the SQL path — the A/B lever.

Result: port_reach 1850ms -> 451ms, and off the slow-rule list on the daemon.

## NEXT: depth-lattice shape (entry_reach_node, op_reach_node)

These are the SAME graph walk with a **depth column** instead of a halt rule.
On the live daemon `entry_reach_node` is now the biggest single rule (~2.3-3.8s).

    # entry_reach_node: no tag, depth, depth cap, no halt
    rel entry_reach_node(node: sym, d: int) key(node) merge(MinBy(d)).
    entry_reach_node(node, 0) <- entry_seed(node).
    entry_reach_node(to, d0 + 1) <- entry_reach_node(from, d0), flow_edge(from, to), d0 < 64.

    # op_reach_node: tag (op), depth, depth cap, no halt
    rel op_reach_node(op: text, n: sym, d: int) key(op, n) merge(MinBy(d)).
    op_reach_node(op, n, 0) <- op_reach_seed(op, n).
    op_reach_node(op, to, d0 + 1) <- op_reach_node(op, from, d0), flow_edge(from, to), d0 < 64.

Key facts that make this a clean extension:
- **BFS layer IS min depth.** A visited-set BFS reaches each node first by its
  shortest path, so `merge(MinBy(d))` needs no extra machinery — the first visit
  is the min-depth row. This is why the SQL MinBy lattice is pure waste here: the
  fixpoint recomputes what BFS gives for free.
- **`d0 < CAP` = "don't expand from a node at depth >= CAP".** Nodes up to depth
  CAP are recorded; expansion stops there. In BFS: pop node at depth d, expand
  only if d < CAP.
- **No halt** for these two; **no tag** for entry_reach_node.

### Design: generalize `walk.rs` to one function

    pub fn multi_source_walk(
        adj: &[Vec<u32>],
        starts: &[(u32 /*tag*/, u32 /*node*/, i64 /*depth*/)],  // base frontier
        halt: Option<&[bool]>,       // None = no stop rule
        depth_cap: Option<i64>,      // None = expand until the visited set closes
    ) -> Vec<(u32, u32, i64)>        // (tag, node, MIN depth)

- port_reach: starts depth = 0 (unused), halt = Some, cap = None; ignore output d.
- entry_reach_node: single tag (a constant 0), starts depth 0, halt = None,
  cap = Some(64); head is (node, d) so drop the tag on write.
- op_reach_node: real tag, starts depth 0, halt = None, cap = Some(64).
- Keep the visited set per tag; record (tag,node) at first (min) depth.
- All starts share depth 0 in these rules, so plain BFS layering = min depth. If
  a future shape seeds mixed depths, switch that case to a 0-1 BFS / bucket queue
  (note it, don't build it speculatively).

### Recognizer changes (`try_native_halt_bfs` -> generalize)

- Head may be 2 or 3 cols. Identify an optional DEPTH col by the head's
  `key(...) merge(MinBy(dcol))`. The remaining non-key cols are tag? + node.
  - 2-col + key+MinBy: (node, depth), no tag.  [entry_reach_node]
  - 3-col + key+MinBy: (tag, node, depth).       [op_reach_node]
  - 2-col, no key:     (tag, node), no depth.    [port_reach, today]
- Recursive body: optionally `!halt(mid,_)`, optionally the depth arith
  `head(tag?, from, d0), ..., edge(from,to), d0 < CAP` producing `(…, to, d0+1)`.
  Parse CAP from the `d0 < N` Cmp; verify the head depth term is `d0 + 1`.
- Base rules: produce (tag?, node, depth) directly (entry_seed at depth 0).
- The `key(...) merge(MinBy(d))` head currently forces `naive_fallback` in
  rebuild_derived (see the `m.key.is_some()` guard) BEFORE reaching the native
  dispatch — MOVE the native attempt ahead of that guard, or the depth rules
  never get here.

### KEY SIMPLIFICATION (found while generalizing walk.rs)

The depth rules are EASIER than port_reach, not harder, because their node
columns are `sym`, not `text`:
- `entry_reach_node(node: sym, d: int)` — node is `sym`; `entry_seed.node` is
  `sym`; `flow_edge` is `sym`. Everything stays in i64 the WHOLE time. The head
  write is the raw i64 sym cell (`Value::Int`), NO `_strings` decode, NO text
  render loop at all. port_reach was the hard case (text head cols forced the
  decode); these skip it entirely.
- `op_reach_node(op: text, n: sym, d: int)` — only the TAG (`op`) is text; node
  `n` is sym (write i64 direct), depth is int.
- So per head column, write by type: sym node -> `Value::Int(key)`; int depth ->
  `Value::Int(depth)`; text/sym tag -> its cell as-is. The current recognizer
  BAILS on a sym head col ("head column is sym") — that guard must become
  "sym node col is fine, write the i64 directly; only a sym TAG needs its cell".
- `load_edges_keyed(sym=true)` already returns the i64 key space; for a sym-node
  head the BFS node ids map back to `id2key[nid]` = the sym cell to write. Done.
- Net: entry_reach_node should be even faster than port_reach (no render phase).

Also: the base/seed rules here take NO edge hop (`entry_reach_node(node, 0) <-
entry_seed(node).`) — the start frontier is the seed nodes themselves at depth 0,
not their one-hop image. The executor already just reads the base head rows as
the frontier, so this needs no special handling — but the depth in the base row
is the literal `0` from the head term, so the base-rule read must capture the
depth column too (port_reach had no depth col).

### TODO checklist
- [x] generalize walk.rs -> multi_source_walk (depth + optional halt + cap);
      multi_source_halt_bfs kept as a thin wrapper. 18 unit tests.
- [x] add depth/cycle/cap unit tests (min-depth, cap boundary, longer-then-shorter
      path dedups to min).
- [x] generalize the recognizer: try_native_depth_walk + structural_key dedup;
      moved ahead of the key/agg naive_fallback guard. (merge 2026-07-12)
- [x] e2e parity: entry_reach_node & op_reach_node native vs DL_NO_HALT_BFS,
      row-identical INCLUDING the depth column (raw-SQLite assertion).
- [x] Fix A (origin-independent dedup) + Fix B (per-tick shared adjacency cache,
      cleared at tick entry, no cross-tick retention). See
      `plans/2026-07-12-depth-walk-AB-fixes.md`.
- [ ] measure on the daemon; expect entry_reach_node ~2.3s -> a few hundred ms.
      (hermetic synthetic fixture rounds to 0ms; needs a live-daemon read.)
- [ ] once confirmed on the daemon, delete the depth rules' `key/MinBy` waiver
      from the perf ledger — the lattice was only ever emulating BFS min-depth.

## Non-goals / watch-outs
- Do NOT collapse cycles with scc.rs here — a depth lattice and a halt rule both
  break SCC condensation (a cycle has no single depth; a halt splits a component).
  BFS with a visited set is the right primitive.
- Keep the strict recognizer + `DL_NO_HALT_BFS` parity lever. A miss must fall
  through to SQL, never silently change rows.
- The `.dl` discovery merge double-loads a file's rules (port_reach's fixpoint
  ran 110 rounds x2 = 220). The recognizer dedups rules by Debug; the SQL path
  does not. Separate bug worth filing — it wastes ~half the statements on EVERY
  fixpoint rule in a double-loaded file.
