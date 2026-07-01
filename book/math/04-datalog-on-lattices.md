# 4. Datalog on lattices

**The question:** [chapter 3](03-semirings.md) showed one recursive `path` rule
computes shortest distance by swapping in the `(min, +)` semiring. But where does the
*value* live? A reachability fact is just present or absent; a shortest-distance fact
carries a number, and that number changes as the fixpoint runs. The clean way to say
"a fact carries a value that merges monotonically" is: the value is an element of a
lattice ([chapter 1](01-order-and-lattices.md)), and the rule aggregates with the
lattice's `meet` or `join`. This chapter says which aggregations compose with
recursion and which do not, and reads Dijkstra and Bellman–Ford as the same scheduled
least fixpoint.

Running example unchanged: `main → run → parse → lex`, `lex → run`, `run → log`,
`helper` dead. Put a weight on each edge (the cost of the call) and ask for the
shortest call chain to `log`.

## 4.1 A fact's value is a lattice element

Plain datalog computes a *set* of facts: a fact is in or out. Lattice datalog attaches
to each key a value from a lattice `L`, and when two derivations produce the same key
with different values, it merges them with the lattice operation instead of keeping
both rows.[^cite-flix]

| Plain datalog               | Lattice datalog                                   |
| --------------------------- | ------------------------------------------------- |
| `reaches(x, y)` present/absent | `dist(x, y) = d` where `d ∈ ℝ≥0 ∪ {∞}`         |
| merge = set union           | merge = lattice `meet`/`join` of the two values   |
| fixpoint over `(2^Facts, ⊆)` | fixpoint over `(Keys → L)`, pointwise ordered     |
| monotone = only add facts   | monotone = each cell only moves one way in `L`    |

For shortest distance the lattice is `(ℝ≥0 ∪ {∞}, ≥)` with `meet = min` and bottom
`∞`. "Better" means smaller, so the fixpoint starts every cell at `∞` (no path known)
and each derivation can only *lower* it. The chain descends to the least fixpoint, the
mirror of [chapter 2](02-fixpoints.md)'s ascending boolean chain. The order is flipped;
the theorem is the same.

```ts
// dist(z) = min over edges (y → z) of dist(y) + w(y, z), with dist(source) = 0.
// The cell value lives in the lattice (number, ordered by ≥, meet = min, ⊥ = ∞).
// `meet` merges two derivations of the same key; the rule body is one `times` step.
type Dist = Map<string, number>;                 // node → best-known distance (⊥ = ∞)

const meet = (a: number, b: number) => Math.min(a, b);   // lattice merge
const relaxEdge = (d: number, w: number) => d + w;       // ⊗ along one edge

function distStep(dist: Dist, edges: [string, string, number][]): Dist {
  const out = new Map(dist);
  for (const [y, z, w] of edges) {
    const dy = dist.get(y) ?? Infinity;
    if (dy === Infinity) continue;
    const cand = relaxEdge(dy, w);                // dist(y) + w(y,z)
    out.set(z, meet(out.get(z) ?? Infinity, cand)); // dist(z) := min(dist(z), cand)
  }
  return out;                                     // every cell only ever decreases
}
```

## 4.2 Monotone aggregation composes with recursion

`min` and `max` over a bounded lattice are *monotone* aggregations: feeding the body
more (or smaller) inputs can only move the head's value one way along the lattice
order, never reverse it. That is exactly the monotonicity [chapter 2](02-fixpoints.md)
needs, so a `min`/`max` aggregate can sit *inside* a recursive rule and the fixpoint
still converges to a unique least answer.[^cite-flix]

The key word is **bounded**. `min` over `[0, ∞]` is fine because every cell is pinned
above `0`: it can only descend a finite number of times before it cannot descend
further (no negative-weight cycle, [chapter 2](02-fixpoints.md) exercise 4). `max` over
a bounded-above lattice (capacities `≤` some ceiling) is the mirror. The bound is what
turns "monotone on an infinite carrier" into "terminates."

## 4.3 Non-monotone aggregation needs stratification

`count` and `sum` are *not* monotone in the lattice sense: adding one more input row
to a `count` changes the answer in a way the next round cannot un-change by adding
more rows, and on a cycle the count never settles (the
[chapter 3](03-semirings.md) divergence: a cycle has infinitely many paths). So `count`
and `sum` cannot sit inside the recursion that feeds them. They must read a relation
that is *already finished*.

The fix is the same layering as negation ([parent chapter 7](../07-the-fast-paths.md),
§7.4): put the aggregate in a **stratum strictly above** every rule that defines its
input. Within a stratum, only monotone work (positive joins, `min`/`max`); a
non-monotone aggregate (or a `not`) forces a stratum boundary.

| Aggregation     | Monotone? | May recurse? | Placement                          |
| --------------- | --------- | ------------ | ---------------------------------- |
| `min` over `[0,∞]` | yes    | yes          | inside the recursive stratum       |
| `max` over `[0,c]` | yes    | yes          | inside the recursive stratum       |
| `count`         | no        | no           | stratum above its input            |
| `sum`           | no        | no           | stratum above its input            |
| `not P`         | no        | no           | stratum strictly above P's defs    |

This is the line Flix draws as well: `min`/`max` (lattice `meet`/`join`) are first-class
inside recursion; counting-style aggregates are stratified.[^cite-flix]

## 4.4 Dijkstra and Bellman–Ford as scheduled least fixpoints

The `dist` rule in §4.1 *is* the shortest-path computation. Dijkstra and Bellman–Ford
are two **schedules** for reaching its least fixpoint, the difference being only in
*which order* edges get relaxed, not in what answer they compute.

- **Bellman–Ford** is the naive schedule: relax every edge each round, repeat until a
  round lowers nothing. That is exactly `distStep` in a loop, the
  [chapter 2](02-fixpoints.md) least-fixpoint climb, just descending. It needs no
  priority structure and tolerates the full graph; its cost is the `O(V·E)` of redoing
  every edge each round.
- **Dijkstra** is the clever schedule for non-negative weights: always finalize the
  closest unfinalized node next, so each edge relaxes once. The priority queue is a
  *scheduling order over the same relaxations*; the fixpoint it lands on is identical.

```ts
// Same lattice, same relaxation, two schedules. Both return the least fixpoint of
// dist(z) = min over (y → z) of dist(y) + w.
function bellmanFord(edges: [string,string,number][], source: string): Dist {
  let dist: Dist = new Map([[source, 0]]);        // ⊥ everywhere else (∞)
  for (;;) {                                       // naive: relax all edges per round
    const next = distStep(dist, edges);
    if (sameDist(next, dist)) return dist;         // a round lowered nothing: fixpoint
    dist = next;
  }
}
// Dijkstra differs ONLY in which edge to relax next (pop the min-dist frontier node),
// reaching the identical `dist` map for non-negative w in one pass per edge.
```

The engine never runs Dijkstra: its closure is reachability `(OR, AND)`, the boolean
corner of [chapter 3](03-semirings.md). But the shape is one swap away. The recursive
`path` rule with `(min, +)` over the distance lattice is the shortest-path engine, and
the only choice left is the schedule.

**Intuition:** a fact's value is a lattice element; `min`/`max` merge derivations and
may recurse because they are monotone; `count`/`sum` and `not` cannot recurse and get
their own stratum above their input.

## In your engine

The v5 engine computes the boolean corner only: `reaches` is set-membership, not a
lattice-valued `dist`. The machinery that *would* host a lattice rule is already there:

- The semi-naive fixpoint in `rebuild_derived` (`../../src/engine.rs`) is the
  least-fixpoint climb; a `min`-aggregated head would descend through the same loop.
- `stratify` (`../../src/engine.rs`) already places a `not P` in a stratum above P's
  definitions. A non-monotone `count`/`sum` aggregate would route through the identical
  dependency-graph SCC check (§4.3): reject the aggregate if its input sits in the same
  recursive cycle.

So the distance lattice is a value-space change, not an evaluation-engine change. The
stratifier is the part that already knows §4.3's rule.

## Exercises

1. **Relax by hand.** Weight the running example: `main→run = 1`, `run→parse = 1`,
   `parse→lex = 1`, `lex→run = 1`, `run→log = 5`. Run `distStep` from `source = main`
   round by round. What is `dist(log)`, and how many rounds until a round lowers
   nothing?

2. **Why the bound matters.** Add a negative-weight edge `lex→run = -3` (so the cycle
   `run→parse→lex→run` sums to `1 + 1 - 3 = -1`). Trace two rounds of `distStep` on
   `dist(run)`. Why does the fixpoint not exist? Tie it to "bounded below" in §4.2.

3. **Aggregate placement.** You want `fanout(f) = count of g such that calls(f, g)`
   *and* `reaches(f, g)` recursive in the same program. Which relation goes in which
   stratum, and why can `fanout` not be computed in the recursive stratum?

4. **Same answer, two schedules.** Argue that Bellman–Ford and Dijkstra return the
   same `dist` map on a graph with non-negative weights. What does Dijkstra assume that
   lets it finalize a node and never revisit it?

## Answers

1. Round 1: `run = 1`. Round 2: `parse = 2`, `log = 6`. Round 3: `lex = 3`. Round 4:
   `run = min(1, 3+1) = 1` (no change), nothing else lowers. So `dist(log) = 6`, and
   round 4 is the first that lowers nothing (3 rounds add, the 4th confirms). The cycle
   re-relaxes `run` but `1` is already minimal, so it stays.

2. With `lex→run = -3`: round 1 `run = 1`; round 2 `parse = 2`; round 3 `lex = 3`;
   round 4 `run = min(1, 3 + (-3)) = 0`; round 5 `parse = 1`, `lex = 2`; round 6
   `run = min(0, 2-3) = -1`; it keeps dropping by 1 each lap forever. The cell `dist(run)`
   is no longer bounded below, so the descending chain never terminates and there is no
   least fixpoint. §4.2's monotone-aggregation guarantee assumed a finite lower bound;
   a negative cycle removes it.

3. `calls` and `reaches` are the recursive stratum (`reaches` is the positive
   transitive closure of `calls`, monotone, may recurse). `fanout` is `count`, which is
   non-monotone, so it goes in a stratum strictly above `calls`. If `fanout` were in the
   recursive stratum, each round that adds a `calls` edge would change the count, and on
   a cycle the count never settles, so the fixpoint would not converge (§4.3, the
   [chapter 3](03-semirings.md) counting divergence).

4. Both compute the least fixpoint of the same monotone `min`-relaxation operator, and
   [chapter 2](02-fixpoints.md)'s uniqueness says a monotone operator on a complete
   lattice has exactly one least fixpoint regardless of firing order, so the answer is
   identical. Dijkstra assumes non-negative weights: that lets it prove the closest
   unfinalized node's distance is already final (no later, longer prefix could lower
   it), so it finalizes once and never revisits. Bellman–Ford makes no such assumption
   and so tolerates negative edges (but not negative cycles, exercise 2).

## Citations

- Madsen, M., Yee, M., Lhoták, O. *From Datalog to Flix: A Declarative Language for
  Fixed Points on Lattices.* PLDI 2016. The clean statement of lattice-valued datalog:
  `min`/`max` recurse, non-monotone aggregates stratify; read §2–4.[^cite-flix]
- Apt, K. R., Blair, H. A., Walker, A. *Towards a Theory of Declarative Knowledge.*
  Foundations of Deductive Databases and Logic Programming, 1988. The stratification
  result the aggregate/negation layering rests on.[^cite-abw]
- Van Gelder, A., Ross, K. A., Schlipf, J. S. *The Well-Founded Semantics for General
  Logic Programs.* JACM 38(3), 1991. What to do when a program is not stratifiable (the
  negation-in-a-cycle case §4.3 rejects).[^cite-wfs]

[^cite-flix]: Madsen, Yee, Lhoták, *From Datalog to Flix*, PLDI 2016.
[^cite-abw]: Apt, Blair, Walker, *Towards a Theory of Declarative Knowledge*, 1988.
[^cite-wfs]: Van Gelder, Ross, Schlipf, *The Well-Founded Semantics for General Logic
Programs*, JACM 38(3), 1991.
