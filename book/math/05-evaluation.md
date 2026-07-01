# 5. Evaluation

**The question:** [chapter 2](02-fixpoints.md) said evaluation is climbing from `⊥` by
a monotone function until a round changes nothing. That is *correct*, but the obvious
implementation redoes the entire join every round, including the facts that were
already present last round and cannot possibly derive anything new. This chapter is the
two ways to run the climb: **naive** (re-derive everything each round) and
**semi-naive** (join only the new facts, the frontier), plus the order strata run in.

Running example unchanged: `main → run → parse → lex`, `lex → run`, `run → log`,
`helper` dead. Ask the recursive `reaches(x, y)`.

## 5.1 Naive evaluation

Naive evaluation applies every rule to the *whole* current relation each round, unions
the results back, and repeats until a round adds nothing. It is the literal reading of
[chapter 2](02-fixpoints.md)'s `leastFixpoint`.

```
   reaches(x, y) <- edge(x, y).
   reaches(x, z) <- reaches(x, y), edge(y, z).

   round 0:  base edges
   round 1:  join ALL reaches against ALL edges  →  length-2 pairs
   round 2:  join ALL reaches against ALL edges  →  length-3 pairs (+ re-derive 1,2)
   round 3:  join ALL reaches against ALL edges  →  re-derive everything, add nothing
```

It is correct (monotone climb on a finite lattice, [chapter 2](02-fixpoints.md)) and
wasteful: round 2 re-joins every length-1 and length-2 pair it already had, deriving
them again only to have the `INSERT OR IGNORE` discard the duplicates. On a graph with
a long chain the redundant work is `O(rounds · full-relation)`.

## 5.2 Semi-naive: the delta / frontier

A pair newly derived this round is the *only* kind of fact that can produce something
new next round. A pair that was already present last round, joined again, derives only
pairs that were also already present. So keep a **delta** (the frontier): the facts
added in the *previous* round, and join only those.[^cite-br]

```
   Δ⁰ = base edges
   Δⁿ⁺¹ = ( join Δⁿ against edge )  minus  (everything already derived)
   stop when Δ = ∅
```

The minus is what makes it semi-naive rather than naive: a fact already in the relation
is never put back in the frontier, so it is never re-joined. The total work becomes
proportional to the number of *distinct derivations*, not derivations times rounds.

```ts
// Semi-naive transitive closure. `frontier` holds only the rows added last round;
// joining the full relation against the frontier (instead of full × full) is the
// whole optimization. New rows that are genuinely new become next round's frontier.
type Pair = string;                               // "x→y"
function seminaive(edge: Set<Pair>): Set<Pair> {
  const all = new Set(edge);                      // everything derived so far
  let frontier = new Set(edge);                   // Δ⁰ = base edges
  while (frontier.size > 0) {
    const next = new Set<Pair>();
    for (const xy of frontier) {                  // join ONLY the frontier …
      const [x, y] = split(xy);
      for (const z of succ(edge, y)) {            // … against base edges (y → z)
        const xz = join(x, z);
        if (!all.has(xz)) { all.add(xz); next.add(xz); } // keep only the genuinely new
      }
    }
    frontier = next;                              // Δⁿ⁺¹: this round's new rows
  }
  return all;
}
```

The two forms reach the *identical* least fixpoint ([chapter 2](02-fixpoints.md));
semi-naive just refuses to recompute the rows it already has. The parent book's §7.1
calls this out as the optimization the engine has "only half-taken."

## 5.3 Stratified evaluation order

Negation and non-monotone aggregation ([chapter 4](04-datalog-on-lattices.md)) break
monotonicity, so they cannot share a fixpoint with the relations they read. The
evaluation order is therefore: **stratum by stratum, bottom up**, each stratum a
self-contained positive fixpoint.

```
   stratum 0:  base + positive-recursive rules     → run §5.1/§5.2 to convergence
   stratum 1:  rules that negate stratum-0 rels    → now read a FINISHED relation
   stratum 2:  rules that negate stratum-1 rels    → …
```

Each stratum is a complete least-fixpoint computation in its own right; only after it
converges does the next stratum start. That is why a `not P` or a `count` over P in a
higher stratum reads a relation that will not change under it: P was finalized in a
lower stratum. The strata themselves come from SCC-ing the rule dependency graph and
assigning a level (positive edge `≥`, negative edge strictly `>`), the
[parent chapter 7](../07-the-fast-paths.md) §7.4 procedure.

**Intuition:** naive redoes every derivation each round; semi-naive joins only last
round's frontier; and strata serialize the climb so non-monotone steps always read a
finished relation.

## In your engine

- The per-stratum convergence loop is `rebuild_derived` in `../../src/engine.rs`. It
  runs the **naive** form: each iteration re-applies every rule in the stratum
  (`INSERT OR IGNORE ... SELECT`), counts `delta`, and stops when `delta == 0`. The
  duplicates are filtered by the primary key on insert, so it is correct, with the
  wasted re-joins §5.1 describes. `seminaive` above is the frontier form it has not
  taken yet.
- The stratum order is `stratify` in `../../src/engine.rs`, the real stratifier:
  intern each relation, build head→body edges (negative through a `not`), `scc::tarjan`
  the rule graph, reject a negative edge inside a recursive SCC, then assign each rule's
  stratum as the max over outgoing condensed edges (`comp_stratum`). `rebuild_derived`
  walks the returned groups ascending, so §5.3's bottom-up order is exactly the loop
  `for group in stratify(...)`.

The split is clean: `stratify` decides the order (§5.3), `rebuild_derived` runs each
group's fixpoint (§5.1, the naive variant).

## Exercises

1. **Count the joins.** On the chain `a→b→c→d→e`, count how many `(reaches, edge)` join
   pairs naive evaluation considers across all rounds versus semi-naive. Why does the
   gap grow with chain length?

2. **Frontier on a cycle.** Run `seminaive` on the running example's cycle
   `run→parse→lex→run`. Show that `reaches(run, run)` enters the frontier exactly once,
   and that the frontier empties even though the graph is cyclic. What keeps it from
   looping forever?

3. **Why the minus.** Drop the `if (!all.has(xz))` guard from `seminaive` (put every
   joined row in the next frontier). Does it still terminate? Does it still give the
   right answer? What property is lost?

4. **Strata for unused.** Give the strata for
   `reaches(x,y) <- edge(x,y). reaches(x,z) <- reaches(x,y), edge(y,z). unused(n) <- def(n), !reaches(_, n).`
   Why must `unused` wait for `reaches` to fully converge, and what would go wrong if it
   ran in stratum 0?

## Answers

1. Naive: each of the ~4 rounds re-joins the full `reaches` relation against `edge`, so
   it considers (growing relation size) × (rounds), roughly `O(n²)` join attempts on an
   `n`-chain. Semi-naive joins only each round's frontier, and each pair is in the
   frontier exactly once, so it considers `O(n)` join attempts total (each derivation
   produced once). The gap is the re-derivations naive repeats every round; it grows
   because the relation is largest in the last rounds, when naive still re-scans all of
   it.

2. Base frontier includes `run→parse`. Joining gives `run→lex`, then `run→run` (closing
   the cycle); `run→run` is new, so it enters the frontier once. Next round it joins
   against `edge` to produce `run→parse`, `run→lex`, which are already in `all`, so the
   guard drops them and the frontier goes empty. The `all`-membership guard is what
   stops it: a cyclic graph has finitely many *distinct* pairs, and each enters the
   frontier at most once, so the frontier drains even though paths are unbounded in
   length.

3. Without the guard it does *not* terminate on a cycle: `run→run` would re-enter the
   frontier every round forever (the cycle re-derives it endlessly). The answer set
   `all` would still be correct if you stopped, but there is no stopping: the lost
   property is finiteness of the frontier. The guard is precisely the "minus everything
   already derived" of §5.2; it is what makes semi-naive a fixpoint rather than an
   infinite re-derivation.

4. Stratum 0: both `reaches` rules (positive transitive closure, one recursive SCC).
   Stratum 1: `unused`, because it negates `reaches`. `unused` must wait because `!reaches(_, n)`
   asks "is `n` reached by nothing," and that is only true once `reaches` is complete; in
   stratum 0 the relation is still being built, so a half-built `reaches` would report a
   node as unused before its incoming edge was derived. Running `unused` in stratum 0
   makes the answer depend on rule-firing order, which stratification exists to prevent.

## Citations

- Bancilhon, F., Ramakrishnan, R. *An Amateur's Introduction to Recursive Query
  Processing Strategies.* SIGMOD 1986. The survey that lays out naive vs semi-naive and
  the delta/frontier; read the semi-naive section.[^cite-br]
- Abiteboul, S., Hull, R., Vianu, V. *Foundations of Databases.* Addison-Wesley, 1995
  (the "Alice book"). The datalog chapters give the precise semi-naive operator and the
  stratified-evaluation theorem.[^cite-alice]

[^cite-br]: Bancilhon, Ramakrishnan, *An Amateur's Introduction to Recursive Query
Processing Strategies*, SIGMOD 1986.
[^cite-alice]: Abiteboul, Hull, Vianu, *Foundations of Databases* (the Alice book),
Addison-Wesley, 1995.
