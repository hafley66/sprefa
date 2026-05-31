# 2. Fixpoints

**The question:** the engine evaluates a recursive query by applying rules over and
over until nothing changes. Why does that stop? Why does it stop at the *right*
answer, and why is that answer unique no matter the firing order? Chapter 1 gave the
lattice; this chapter gives the theorem that makes the loop correct: Knaster–Tarski.

Running example unchanged: `main → run → parse → lex`, `lex → run`, `run → log`,
`helper` dead.

## 2.1 Monotone functions

A function `f: L → L` on a lattice is **monotone** when it preserves the order:

```
   a ≤ b   ⇒   f(a) ≤ f(b)        (more input, never less output)
```

Datalog rules are monotone over the fact-set lattice (`⊆`). Adding facts to the input
can only add facts to the output of a rule, never retract one. That is the whole
reason positive datalog has a clean meaning. The moment you add negation (`not P`),
monotonicity breaks: adding a `P` fact can *remove* a `not P` derivation. That break
is why chapter 4 and chapter 5 need stratification.

```ts
// One round of evaluation: apply every rule to the current facts, union the
// results back in. `applyRule` is itself monotone, and union is monotone, so the
// whole step is monotone: more facts in ⇒ at least as many facts out.
type Facts = Set<string>;
const step = (rules: Rule[], db: Facts): Facts => {
  let out = new Set(db);                       // keep what we have (only grow)
  for (const r of rules) for (const f of applyRule(r, db)) out.add(f);
  return out;                                  // db ⊆ out, always
};
```

## 2.2 Ascending chains

Start at `⊥` (the empty fact set) and iterate `f`:

```
   ⊥  ≤  f(⊥)  ≤  f(f(⊥))  ≤  f³(⊥)  ≤  ...
```

Each link holds because `f` is monotone and `⊥ ≤ f(⊥)` (bottom is below everything),
so applying `f` to a `≤` step gives another `≤` step. This is an **ascending chain**.
In a **finite** lattice the chain cannot ascend forever: it strictly increases until
it cannot, then `fⁿ⁺¹(⊥) = fⁿ(⊥)`. That repeated value `x` satisfies `f(x) = x`: a
**fixpoint**.

The fact universe is finite (finitely many functions, finitely many call edges), so
the chain is finite, so the loop terminates. Termination is not an algorithm trick; it
is a property of monotone functions on finite lattices.

## 2.3 Knaster–Tarski

Tarski's theorem (1955) says more than "a fixpoint exists":[^cite-tarski]

> A monotone function on a **complete lattice** has a complete lattice of fixpoints;
> in particular it has a **least** fixpoint `lfp(f)` and a greatest fixpoint.

The least fixpoint is the one datalog wants: the smallest fact set closed under the
rules, with no facts that no rule justifies (no spurious "self-supporting" facts, the
chapter-3 cycle problem). Completeness matters because it guarantees the least
fixpoint exists even when the lattice is infinite; for a finite lattice the iteration
in 2.2 actually reaches it. Because the least fixpoint is *unique* (chapter 1:
antisymmetry forces a unique least element), the firing order of rules does not change
the answer.

```ts
// Climb from ⊥ applying f until a round changes nothing. On a finite lattice this
// returns lfp(f): the least x with f(x) = x. (Knaster–Tarski, Tarski 1955.)
function leastFixpoint<T>(L: Lattice<T>, f: (x: T) => T): T {
  let x = L.bottom;          // start at the empty answer
  while (true) {
    const next = f(x);       // f is monotone, so x ≤ next (the chain ascends)
    if (L.leq(next, x)) return x; // next ≤ x AND x ≤ next ⇒ next = x: fixpoint
    x = next;                // strictly grew: keep climbing
  }
}
```

The engine's evaluation loop is exactly this, specialized to the `⊆` lattice, with
`L.leq(next, x)` implemented as "this round added zero new rows" (`delta == 0`). See
[chapter 5](05-evaluation.md) for the naive vs semi-naive forms of `f` and
[the parent book's chapter 7](../07-the-fast-paths.md) for the engine's loop.

## 2.4 Reachability is the boolean fixpoint

Specialize the lattice to "is there a fact `reaches(x, y)`?" The carrier is the
powerset of node pairs; `f` adds `reaches(x, z)` whenever `reaches(x, y)` and
`edge(y, z)`. Starting from the base edges and climbing to the least fixpoint gives
the full reachability relation. Per pair, the value is a boolean (`OR` to merge, the
chapter-1 boolean lattice); over all pairs, the value is a subset (`∪` to merge).

On the running example:

| Round | New `reaches` facts derived                                                  |
| ----- | ---------------------------------------------------------------------------- |
| 0     | base edges: main→run, run→parse, parse→lex, lex→run, run→log                  |
| 1     | main→parse, run→lex, parse→run, lex→parse, run→run (cycle closes), main→log…  |
| 2     | the remaining cross-cycle pairs (main→lex, parse→log, lex→log, …)            |
| 3     | adds nothing: fixpoint                                                        |

`helper` never appears: no rule derives a pair touching it, so the least fixpoint
leaves it out. That is the least fixpoint doing its job: it includes a fact only if
some derivation forces it.

**Intuition:** evaluation is climbing from bottom by a monotone function in a finite
world; it always stops, and it stops at the one least fixpoint, the smallest fact set
the rules force.

## Exercises

1. **Count the rounds.** Run the `reaches` fixpoint on the chain `a→b→c→d` (no cycle).
   How many rounds add facts before one adds nothing? Give the new pairs per round.

2. **Why least, not greatest.** The greatest fixpoint of the `reaches` operator
   includes self-supporting pairs that no base edge justifies. Give a concrete
   spurious fact the greatest fixpoint would admit on a graph with a 2-cycle `x↔y`,
   and say why the least fixpoint excludes it. (Connect to chapter 3.)

3. **Monotonicity breaks.** Show `f(P) = {a}` if `a ∉ P` else `∅` is not monotone, and
   that it has no fixpoint reachable by climbing from `⊥`. Which datalog feature does
   this model?

4. **Finite ⇒ terminates.** The argument in 2.2 needs the lattice to be finite. The
   `min`-distance lattice on `[0, ∞]` is infinite. What extra property makes the
   distance fixpoint still terminate? (Forward-ref chapter 4.)

## Answers

1. Three rounds add facts. Round 1: `a→c, b→d`. Round 2: `a→d`. Round 3: adds nothing
   (every reachable pair already present), so it stops. Total pairs: `a→b, a→c, a→d,
   b→c, b→d, c→d`.

2. With `x↔y`, the operator's greatest fixpoint can include facts like "x is in the
   set" justified only by "y is in the set" justified only by "x is in the set":
   circular support with no base anchor. The least fixpoint starts at `⊥` and only
   adds a fact when a rule body is already satisfied by facts present, so a mutually
   self-supporting pair with no external justification is never added. This is exactly
   the chapter-3 counting/self-support problem: a cycle can justify itself in the
   greatest fixpoint.

3. `f(∅) = {a}`, `f({a}) = ∅`, `f(∅) = {a}`: it oscillates, never settling, so no
   reachable fixpoint. It is not monotone: `∅ ⊆ {a}` but `f(∅) = {a} ⊄ ∅ = f({a})`.
   This models `p :- not p`: negation inside recursion, which is why such programs are
   rejected by stratification (chapters 4–5).

4. The operator is monotone *and* the distances only ever decrease toward a finite
   lower bound (no negative cycles), so each cell strictly decreases a bounded number
   of times. Bounded-decreasing on a well-founded order terminates even though the
   carrier is infinite. Chapter 4 (Dijkstra/Bellman–Ford) is this argument made
   precise.

## Citations

- Tarski, A. *A lattice-theoretical fixpoint theorem and its applications.* Pacific
  Journal of Mathematics 5(2):285–309, 1955. The least/greatest fixpoint theorem on a
  complete lattice; the foundation under every datalog engine.[^cite-tarski]

[^cite-tarski]: Tarski, *A lattice-theoretical fixpoint theorem and its applications*,
Pacific J. Math. 5(2), 1955.
