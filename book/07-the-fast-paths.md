# 7. The fast paths: the loops that make it scale

> the loops that make it scale: semi-naive fixpoint, Tarjan/condensation, seeded reachability, stratification, auto-index, with citations and exercises.

**The question:** chapters 1–6 built the model and an honest, correct evaluation.
But "correct" ran the kernel reachability in 197s and the resolved call graph in
30s. This chapter is the handful of loops and algorithms that turn correct into
fast, each with its one idea, why it works, a citation, and an exercise. None of
them were invented here. The skill is seeing which one a problem needs.

We reuse the running example throughout: `main → run → parse → lex`, with
`lex → run` (a cycle), `run → log` (a sink), and `helper` (dead).

```
   main ──▶ run ──▶ parse ──▶ lex
             │        ▲          │
             │        └──────────┘
             ▼
            log                     helper (isolated)
```

## 7.1 The evaluation loop: the semi-naive fixpoint

The core of the engine is one loop: apply every rule, see if new facts appeared,
repeat until a pass adds nothing.

```
   loop {
     delta = 0
     for rule in stratum:  delta += apply(rule)   // INSERT OR IGNORE ... SELECT
     if delta == 0 { break }
   }
```

**Why it terminates and is unique:** every rule only *adds* facts, so the set of
derived facts grows monotonically inside a finite universe. A monotone operator on
a complete lattice has a unique **least fixpoint** (the Knaster–Tarski theorem,
Tarski 1955). The loop walks straight to it.

The word *semi-naive* is the optimization we have only half-taken: on each round
you should join only the facts that are **new this round** (the frontier), not the
whole relation, because re-joining old facts can derive nothing new. We currently
re-run the full join (`INSERT OR IGNORE` discards the duplicates). Correct, with
wasted work. The frontier version is Bancilhon & Ramakrishnan (1986).

> Cite: Tarski, *A lattice-theoretical fixpoint theorem* (1955). Bancilhon &
> Ramakrishnan, *An Amateur's Introduction to Recursive Query Processing
> Strategies*, SIGMOD 1986. Abiteboul, Hull, Vianu, *Foundations of Databases*
> (1995), the datalog chapters.

**Intuition:** monotone growth in a finite world always settles, and it settles in
exactly one place.

## 7.2 Condensation: Tarjan, then a DAG walk

The fixpoint computes the full `reaches` relation, which on a cyclic graph is
Θ(V²). The trick from chapter 3: collapse each cycle to one super-node. What
remains is a **DAG**, and on a DAG reachability is a topological-order walk with no
thrashing.

Finding the cycles is **Tarjan's SCC algorithm**: a single depth-first search.
Give each node an `index` (visit order) and a `lowlink` (the smallest index it can
climb back to while staying on the stack). The one invariant:

```
   lowlink[v] == index[v]   ⇒   v is the entry of an SCC
                                pop the stack down to v; that group is one component
```

One DFS, each node and edge touched once, so **O(V + E)**. That linear pass is why
condensing the kernel's 23k edges took milliseconds.

> Cite: Tarjan, *Depth-first search and linear graph algorithms*, SIAM J. Comput.
> 1(2), 1972.

**Intuition:** members of a cycle reach the same things, so treat each cycle as one
node; then reachability is trivial.

## 7.3 Seeded reachability: forward and reverse

A *point query* pins one end and walks: "what does `run` reach?" is a BFS out from
`run`'s component; "who reaches `log`?" is a BFS *in* to `log`'s component over the
**reversed** condensed edges. Reversing a DAG's edges preserves its SCCs, so the
reverse direction is the identical algorithm over `cadj_rev`.

```
   reaches_from(run):  run ─▶─▶─▶          (walk out-edges)
   reached_by(log):       ◀─◀─◀ log        (walk in-edges)
```

This beats the SQL view because the view's recursive query cannot start from the
seed: it builds the whole component closure, then filters. The seeded walk touches
only what the seed connects to. On the kernel that is 30µs vs the view's ~2s.

**Intuition:** a point query is a seed plus a direction; never compute the pairs you
did not ask for.

## 7.4 Stratification: negation needs layers

Add `not` and the fixpoint loses monotonicity: deriving a fact can remove another's
justification, so there may be no least fixpoint at all (`p :- not p`). The fix is
to **layer** the relations so that whenever a rule uses `not P`, P is fully computed
in an earlier layer. Build the predicate dependency graph (edge head→body, marked
negative through a `not`), and:

```
   SCC the dependency graph            (Tarjan again, on the RULE graph)
   negative edge inside an SCC   ⇒  reject (negation tangled in a cycle)
   else assign strata             positive edge: ≥ ,  negative edge: strictly >
   evaluate strata bottom-up      each is a positive fixpoint (§7.1)
```

Now every `not P` reads a finished P, so the answer no longer depends on rule
order. This is **stratified negation**, the standard way to give recursive datalog
a clean meaning.

> Cite: Apt, Blair, Walker, *Towards a Theory of Declarative Knowledge* (1988).
> Przymusinski, *On the Declarative Semantics of Deductive Databases* (1988). For
> the non-stratifiable case: Van Gelder, Ross, Schlipf, *The well-founded semantics
> for general logic programs*, JACM 1991.

**Intuition:** negation must look down at something already finished, never sideways
at something still being built.

## 7.5 Auto-index: turn scans into seeks

A rule joins relations on a shared variable. Without an index, the join is a nested
scan; with one, it is a seek.

```
   calls(c1,c2) <- fndef(c1, p, s, e), callsite(c2, p, l), ...
                              \_______________/
                          shared var p = the join key

   no index:  for each fndef (F): scan ALL callsite (C)   =  O(F · C)
   indexed:   for each fndef (F): seek callsite WHERE path=p = O(F · log C)
```

On the kernel F≈16k, C≈96k, so the scan is ~1.5e9 row touches (the slow 22s). An
index on `path` collapses it to seeks (1.9s). The analysis is purely syntactic:
index every column a variable reaches across ≥2 body atoms. The precise version,
**Soufflé's automatic index selection**, computes the *minimal* set of composite,
ordered indexes covering all search patterns, via a min-chain-cover (Dilworth's
theorem) on the lattice of column orders.

> Cite: Subotić, Jordan, Guo, Scholz, *Automatic Index Selection for Large-Scale
> Datalog Computation*, VLDB 2018. Jordan, Scholz, Subotić, *Soufflé: On Synthesis
> of Program Analyzers*, CAV 2016. Dilworth, *A decomposition theorem for partially
> ordered sets*, Annals of Math. 1950.

**Intuition:** every shared variable in a rule body is a join; index its columns and
the join stops scanning.

## Exercises

1. **Fixpoint by hand.** Run `anc(x,y) <- parent(x,y). anc(x,y) <- parent(x,z), anc(z,y).`
   over the chain `parent(a,b), parent(b,c), parent(c,d)`. How many rounds until
   the pass adds nothing? Which derivations in round 3 were re-checked but added
   nothing (the work semi-naive would skip)?

2. **Tarjan trace.** Run Tarjan on the running example starting at `main`. Write
   `index` and `lowlink` for each node and the moment each SCC is popped. What are
   the four SCCs?

3. **Why reverse is free.** Argue that reversing every edge of a graph leaves its
   SCCs unchanged. (Hint: what does an edge reversal do to a cycle?)

4. **Seeded walks.** On the condensed DAG `[main] → [run,parse,lex] → [log]`,
   compute `reaches_from(run)` and `reached_by(log)` by hand. Why does `helper`
   appear in neither?

5. **Stratify.** Give the strata for `calls(...)` and `unused(n) <- def(n,_,_), !calls(_,n)`.
   Then explain why `win(x) <- move(x,y), !win(y).` over a graph with a 2-cycle is
   the interesting case (it is *locally* stratifiable per game position but not as a
   single relation: this is why games need well-founded semantics).

6. **Index the join.** For `calls(caller,callee) <- fndef(caller,p,s,e), callsite(callee,p,l), s<=l, l<=e, fndef(callee,_,_,_).`
   list every join key and the index it implies. Why are `s`, `l`, `e` *not* among
   them, and what kind of index (not built here) would a range join want?

## Answers

1. Three rounds add facts (`b,c,d` reachable from `a` appear over rounds), the 4th
   adds nothing and stops. In round 3, every length-1 and length-2 ancestor pair is
   re-joined; only the new length-3 pairs (`anc(a,d)`) are genuinely new. Semi-naive
   joins only the round-2 frontier against `parent`, skipping the rest.

2. `index/lowlink`: main 0/0, run 1/1, parse 2/1, lex 3/1, log 4/4, helper 5/5.
   `lex → run` pulls lowlink of lex, parse, run all down to 1. `log` pops first
   (4==4), then `run` is the entry (1==1) and pops `{run, parse, lex}`, then `main`
   (0==0), then `helper` (5==5). SCCs: `{log}`, `{run,parse,lex}`, `{main}`,
   `{helper}`.

3. A directed cycle `a→b→c→a` reversed is `a→c→b→a`, still a cycle through the same
   nodes. "Mutually reachable" is symmetric under reversal, so the partition into
   SCCs is identical. Only the DAG between components flips direction.

4. `reaches_from(run) = {run, parse, lex, log}` (run is on the cycle, so it reaches
   itself and its two cycle-mates, plus the downstream sink). `reached_by(log) =
   {main, run, parse, lex}`. `helper` is isolated: no path in or out, so it is in
   neither.

5. `calls` is stratum 0 (depends only on the base relations `def`, `use`); `unused`
   is stratum 1 because it negates `calls`. The `win` rule has `win` depending on
   `win` through a `not`, so on a cycle it forms a negative edge inside an SCC:
   unstratifiable. Its meaning (won/lost/drawn positions) needs well-founded or
   stable-model semantics, not stratification.

6. Join keys: `p` (shared by `fndef` and `callsite`) ⇒ index `fndef(path)`,
   `callsite(path)`; and `callee` (shared by `callsite` and the second `fndef`) ⇒
   `callsite(callee)`, `fndef(name)`. `s`, `l`, `e` appear in only one relational
   atom each (they live in the `<=` comparisons), so they are not equality join
   keys. A range join (`s <= l <= e`) wants an *ordered* index on the range column
   so the engine can binary-search the bound, which our single-column equality
   indexes do not provide; that is the next refinement.

## In your engine

- §7.1 semi-naive fixpoint: `rebuild_derived` in `src/engine.rs` (the per-stratum loop).
- §7.2 Tarjan + condensation: `src/scc.rs` (`tarjan`, `build_condensed`).
- §7.3 seeded reachability: `scc::reaches_from` / `scc::reached_by`, routed in `run_query`.
- §7.4 stratification: `stratify` in `src/engine.rs` (reuses `scc::tarjan` on the rule graph).
- §7.5 auto-index: `auto_indexes` / `create_auto_indexes` in `src/engine.rs`.

The same SCC pass shows up in three places now, each on a different graph: the data
call graph (reachability), the rule dependency graph (stratification), and nowhere
in indexing (that is a cover problem, not a cycle problem). Seeing which graph a
technique runs on is most of the understanding.
