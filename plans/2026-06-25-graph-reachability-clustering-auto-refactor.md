# Graph reachability, clustering, and auto-refactor

A concept sketch for turning the engine's existing graph relations into
reachability / clustering queries that drive automated refactoring. Not a
spec — a map of what's already in the tree vs. what's missing, so the idea has
somewhere to land.

## The substrate (already built-in)

The engine already materializes a rich set of graph relations per tick, lazily
(gated on use):

| relation | graph |
|---|---|
| `type_edge(from, to, kind)` / `_rev` | type graph: field/variant/impl/generic (Rust syn, Kotlin ts, TS oxc) |
| `call_edge(caller, callee, kind)` / `_rev` | resolved call graph (caller-sym → callee-sym) |
| `call_site(caller, callee, file, line)` | call occurrences (line-scoped) |
| `df_node(id, kind, var, fn, …)` / `df_edge(from, to)` | intra-procedural dataflow |
| `node(id, kind, file, lo, hi, parent)` / `child(parent, child)` | the CST as a relation (christmas #3, done) |
| `loop_over`, `nest(call_id, loop_id, depth, …)`, `allocates(fn)` | loop/nesting/alloc — raw material for symbolic Big-O |
| `module_edge(src, dst)` / `crate_edge` | file/file + workspace import graphs |
| `ref(id, string, file, lo, hi)` + `node_file_span_idx` | byte-located rewrite coordinates |

And one cross-cutting operator already on top of them:

- **`closure(edge)`** — transitive reachability over any edge relation, via the
  SCC condensation (cached across ticks, recondensed only on row churn). This is
  *the* reachability primitive: `reaches(X, Y) <- closure("call_edge"), …` already
  answers "can A reach B through the call graph." Works for `type_edge`,
  `df_edge`, `module_edge` alike.

So **transitive reachability is already expressible** for every graph the engine
builds. What's *not* there: the higher-order questions that make refactoring
automatic.

## The missing layers

### 1. Clustering / cohesion (the "who groups with whom" question)

`closure` gives reachability (a binary can-reach). It does not give:

- **Connected components / SCCs as first-class sets** — `closure` walks them but
  doesn't expose the component id. A `comp(id, node)` relation over an edge would
  let a program ask "what's the island this fn lives on."
- **Cohesion / coupling metrics** — fan-in/fan-out per node, edge weight by call
  count, the cluster's internal edge density vs. its boundary. Christmas #12
  (window aggregates: rank/row_number/nth) is the blocker — these need
  per-group ranking and counts the store has but the language doesn't surface.
- **Community detection** — Louvain / label propagation over the weighted graph
  to find natural clusters. Out of scope as a builtin; but a `cmd`-driven shell
  pass (dump `call_edge` → run a clusterer → read back `cluster(node, id)`) is the
  escape hatch until #1 (data-driven coords) lands for real.

### 2. Scope and binding (the "what does this see / own" question)

Christmas #10 (partial): the CST gives properly-nested spans, so "innermost node
covering byte C in file F" is an indexed range scan. But **binding/visibility and
free-variable analysis are open.** That's what you need to answer:

- "Is name X in scope at coord C" (name resolution).
- "What does byte range [lo,hi) reference but not bind" (free vars — the
  extraction target for a move).
- "Which decl owns this use" (the scope tree).

Needs: #3 (CST, done) + binding rules (an external index or a per-language
decl/use walker) + the interval index (done). The free-var set is the input to
any "extract this range and fix its imports" refactor.

### 3. Edit algebra (the "act on it" question)

Christmas #4 (open): the sink is replace-one-span only. Auto-refactor needs:

- `insert_before` / `insert_after` / `delete` / `wrap` operators.
- A multi-edit transaction with **overlap detection** (two edits to the same span
  bail loudly — mirrors the gen two-writers rule).
- File-level sinks (#13): create / rename / delete, beyond what `--move`
  hardcodes.

Located rewrite coordinates already exist (`ref`/`node` byte spans + `_where_bytes`),
so the edit targets are first-class. The missing piece is the algebra of edits +
a verified transaction.

### 4. Verify-rollback (the "did it work" question)

Christmas #14 (open): apply the edit to a scratch worktree, run a checker via the
`cmd` op, keep only if it passes. The reactive tick + `changed_line` rails already
give the "what did the edit touch" half; the scratch-worktree + keep-if-pass is
the missing harness.

## The refactor loop this enables

```
discover   : scan + type_edge + call_edge + df_edge + node/scope
cluster    : comp(id, node) + cohesion metrics  (#12 + a component relation)
propose    : "these 5 fns form an island with high internal cohesion and a
              narrow boundary → extract module / move to file F"
edit       : edit algebra (#4) over the located spans, multi-edit txn
verify     : scratch worktree + checker (#14), keep-if-pass
```

Each layer is a christmas-list item; the substrate is done. The first useful
increment is probably **(1) component + fan-in/out over `call_edge`**, because it
needs only #12 (window aggregates) and immediately surfaces "the island of fns
that only talk to each other" — the extraction candidates — without touching the
edit side.

## Ties to the christmas list

- #1 data-driven scan — **done** (this arc).
- #3 CST-as-relation — done. #9 whole-match span — done.
- #10 scope — partial (interval + point index landed; binding/free-var open).
- #12 window aggregates — open. **Unlocks clustering metrics.**
- #4 edit algebra — open. **Unlocks acting on clusters.**
- #13 file sinks — open. #14 verify-rollback — open.
- `closure(edge)` — the reachability primitive, already in tree.

## Open questions

- Is a `comp(id, node)` relation worth a builtin (Tarjan over a named edge, like
  `closure`), or does `closure` + a thin derived rule suffice today?
- Cohesion metrics: surface via #12 aggregates, or a dedicated `cohesion(cluster,
  internal, boundary)` builtin?
- Community detection: defer to a `cmd` shell pass until the in-engine aggregate
  story (#12) lands?
