# 7. Incremental

**The question:** the engine recomputes derived relations when a source fact changes
([chapter 5](05-evaluation.md)). Edit `b.rs`, and the whole `reaches` fixpoint reruns,
even though only one call edge moved. That is the wholesale-recompute problem. The
theory that fixes it treats a relation as a *stream of changes* and asks each operator
for its *derivative*: given the input delta, produce the output delta directly. This
chapter is that calculus (DBSP, Differential Dataflow), why its standard implementation
conflicts with bounded memory, and the non-derivative route the engine takes for
reachability specifically.

Running example unchanged: `main → run → parse → lex`, `lex → run`, `run → log`,
`helper` dead. Edit one file; ask what `reaches` should change.

## 7.1 The wholesale-recompute problem

A source edit changes a handful of base facts. Naive maintenance reruns the entire
[chapter 5](05-evaluation.md) fixpoint over the full relation, doing work proportional
to the *result* size, not the *change* size. On a 23k-edge graph, flipping one edge
re-derives the whole closure. The goal of incremental maintenance is work proportional
to `|Δinput| + |Δoutput|`, the size of what actually moved.

## 7.2 DBSP: a relation is a stream of weighted changes

DBSP models a relation not as a set but as a **Z-set**: a map from row to an integer
weight. Weight `+1` means "this row is present," and a *change* is itself a Z-set with
positive weights for insertions and **negative weights for retractions**.[^cite-dbsp]

A query `Q` over relations becomes a circuit, and every operator has a **derivative**:
a companion operator that maps *input deltas* to *output deltas*. Maintaining a view is
then: feed the input change `ΔI` through the derivative circuit `Q′`, get the output
change `ΔV`, apply it. No recomputation of unchanged rows.

```ts
// A Z-set: row → weight. +1 present, -1 retracted, 0 absent. A "change" is a Z-set
// of deltas (inserts positive, deletes negative). Maintaining a view V = Q(I) means
// applying Q's DERIVATIVE Q′ to the input delta: ΔV = Q′(ΔI), never recomputing Q(I).
type ZSet<R> = Map<R, number>;                    // row → weight

const add = <R>(a: ZSet<R>, b: ZSet<R>): ZSet<R> => {  // merge two changes
  const out = new Map(a);
  for (const [r, w] of b) {
    const n = (out.get(r) ?? 0) + w;
    if (n === 0) out.delete(r); else out.set(r, n); // weight 0 ⇒ row gone
  }
  return out;
};

// The maintenance step: derivative of the query applied to the input delta.
function viewDelta<I, V>(Qprime: (di: ZSet<I>) => ZSet<V>, inputDelta: ZSet<I>): ZSet<V> {
  return Qprime(inputDelta);                       // ΔV, apply this to the stored view
}
```

The retraction case is the hard one: deleting an input edge must retract every derived
fact that *only* it justified, while keeping facts some other derivation still
supports. Z-set weights handle this by arithmetic, a derived row survives iff its total
weight stays positive after the negative delta propagates.

## 7.3 Differential Dataflow and the residency conflict

Differential Dataflow (DD) is the same derivative idea generalized over a *partial
order* of timestamps (so iterative fixpoints and incremental updates compose), the
engine behind Materialize.[^cite-dd] It is the strongest known general incremental
engine.

It has a cost that matters here: DD keeps **arrangements**, indexed, RAM-resident
copies of every relation and intermediate, so it can diff against prior state. That is
exactly the bounded-RSS discipline this project refuses
([parent chapter 5](../05-where-bytes-live.md)). The relation lives on disk in SQLite;
holding every arrangement in memory is the residency model the engine is built to
avoid.

So the move is: **borrow the calculus, not the residency.** The Z-set derivative is the
right way to *think* about a change (delta in, delta out, retraction as negative
weight). Keeping every arrangement resident in RAM is the part to leave behind.

## 7.4 The non-DD route for reachability

For reachability specifically there is a cheaper route than the general derivative,
because [chapter 6](06-graph-cores.md) already turned the graph into a DAG:

1. **Incremental SCC maintenance.** An edge insert or delete usually does not
   restructure the components. Incremental cycle-detection / SCC algorithms update the
   condensation in time proportional to the affected region, not the whole
   graph.[^cite-icd] Only an edge that creates or breaks a cycle touches component
   structure.

2. **Delete-rederive / counting on the condensed DAG.** For deletions, the classic
   choices are DRed (delete the over-approximation, then re-derive what survives) and
   **counting** (store, per derived fact, how many derivations support it; a delete
   decrements, and the fact goes only when the count hits zero).[^cite-dred]

The catch with counting is that it is *unsound on a cyclic graph*: a cycle lets a fact
support its own count, so the count never reaches zero even after every external
justification is gone (the [chapter 3](03-semirings.md) self-support problem). On a
**DAG** that cannot happen, no fact is in its own support, so a zero count genuinely
means "no surviving derivation." This is the payoff of condensation reaching past
query speed: collapsing cycles to super-nodes is exactly what makes counting-based
incremental deletion *sound*.

**Intuition:** incremental maintenance is differentiation, delta in, delta out, with
retraction as a negative weight; borrow DBSP's calculus but not DD's RAM-resident
arrangements, and for reachability lean on condensation, which makes cheap
counting-based deletion sound by removing the cycles that would otherwise support
themselves.

## In your engine

The v5 engine is at the *coarse* end of this spectrum and honest about it:

- `tick` / `tick_paths` in `../../src/engine.rs` reconcile sources incrementally
  (hash/mtime fast-path, retract by `_prov` orphan sweep), then rebuild derived
  **wholesale** if any source fact changed (§7.1). That is recompute, not a derivative
  circuit.
- Source-side retraction (`retract_paths`) is already the negative-weight idea in
  miniature: a row survives iff some remaining path still provides its `__src`.
  Derived-side incrementality (§7.2–7.4) is the unbuilt next step.
- The condensation (`../../src/scc.rs`, [chapter 6](06-graph-cores.md)) is the piece
  §7.4 needs: it is what would make counting-based derived deletion sound, since the
  closure is computed on the acyclic condensed DAG, not the cyclic original.

## Exercises

1. **Z-set retraction.** Edge `run → log` has two derivations of `reaches(main, log)`
   (via `run` directly, and via the cycle). Write the Z-set delta for deleting one
   derivation. Does `reaches(main, log)` survive? State the weight arithmetic.

2. **Why DD costs RAM.** Explain in one sentence why DD must keep arrangements resident,
   and what it could not compute incrementally without them.

3. **Counting is unsound on a cycle.** Take the 2-cycle `x ⇄ y` with one external edge
   `a → x`. Show that counting-based deletion of `a → x` leaves `reaches(x, y)` with a
   nonzero count even though `x` is no longer externally reached. Why does condensing
   `{x, y}` to one node fix it?

4. **Which edits are cheap.** Classify these edits to the running example by whether
   they touch SCC structure: (a) add `log → main`, (b) delete `run → log`, (c) add
   `helper → run`. Which force a condensation rebuild and which are local?

## Answers

1. Let `reaches(main, log)` have weight `+1` per surviving derivation. With two
   derivations it would be tracked as count `2` (or two distinct support rows). Deleting
   one derivation is a delta of weight `-1` on that support; the total drops `2 → 1`,
   still positive, so `reaches(main, log)` survives. It is retracted only when the count
   reaches `0`, i.e. when the *last* derivation is deleted. That is the `add` arithmetic
   in §7.2: a row is gone only when its total weight hits zero.

2. DD must keep arrangements (indexed prior state) resident because a derivative diffs
   the new delta against what the operator held before; without that stored state it
   cannot tell which output rows changed. It could not do incremental *joins* or
   *iterative fixpoints* without them, since both need the prior indexed relation to
   diff against.

3. `a → x`, `x → y`, `y → x`. `reaches(x, y)` is derived from `x → y` directly and also
   re-justified around the cycle (`x → y → x → y`), so counting credits it with
   cycle-internal support. Deleting `a → x` removes the external reason `x` is reachable,
   but the cycle keeps re-supporting `reaches(x, y)`, so its count never hits zero, the
   fact wrongly survives. Condensing `{x, y}` to one super-node removes the internal
   cycle from the graph counting runs on; on the resulting DAG no fact is in its own
   support, so a zero count is sound.

4. (a) add `log → main` creates a cycle through `main → run → log → main` (and merges
   `main` and `log` into the big SCC): **touches SCC structure**, condensation rebuild.
   (b) delete `run → log` removes a cross-component edge only (`log` was already its own
   SCC, a sink): **local**, no component merges or splits. (c) add `helper → run`
   attaches the isolated `helper` to the cycle's component as a predecessor but adds no
   cycle: a new cross-component edge, **local** to the DAG, no SCC merge.

## Citations

- Budiu, M., McSherry, F., Ryzhyk, L., Tannen, V. *DBSP: Automatic Incremental View
  Maintenance for Rich Query Languages.* VLDB 2023. The Z-set / derivative calculus of
  §7.2; the cleanest statement of "a relation is a stream of changes."[^cite-dbsp]
- McSherry, F., Murray, D. G., Isaacs, R., Isard, M. *Differential Dataflow.* CIDR 2013.
  The derivative idea over a partial order of timestamps; the arrangements of
  §7.3.[^cite-dd]
- Gupta, A., Mumick, I. S., Subrahmanian, V. S. *Maintaining Views Incrementally.*
  SIGMOD 1993. The DRed (delete-and-rederive) and counting algorithms of
  §7.4.[^cite-dred]
- Bender, M. A., Fineman, J. T., Gilbert, S., Tarjan, R. E. *A New Approach to
  Incremental Cycle Detection and Related Problems.* ACM TALG 12(2), 2015. Incremental
  SCC / cycle maintenance, the §7.4 step that updates the condensation
  locally.[^cite-icd]

[^cite-dbsp]: Budiu, McSherry, Ryzhyk, Tannen, *DBSP: Automatic Incremental View
Maintenance for Rich Query Languages*, VLDB 2023.
[^cite-dd]: McSherry, Murray, Isaacs, Isard, *Differential Dataflow*, CIDR 2013.
[^cite-dred]: Gupta, Mumick, Subrahmanian, *Maintaining Views Incrementally*, SIGMOD
1993.
[^cite-icd]: Bender, Fineman, Gilbert, Tarjan, *A New Approach to Incremental Cycle
Detection and Related Problems*, ACM TALG 12(2), 2015.
