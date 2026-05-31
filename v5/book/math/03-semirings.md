# 3. Semirings

**The question:** the engine answers "does `main` reach `log`?" with a recursive
`path` rule. The same shape of rule should answer "what is the shortest call chain?",
"how many distinct call paths are there?", and "what is the widest bottleneck along
the best path?". Do you need four engines? No. You need one recursive query and four
**semirings**. Swap the algebra, keep the rule.

Running example: `main → run → parse → lex`, `lex → run`, `run → log`, `helper` dead.

## 3.1 What a semiring is

A **semiring** `(K, ⊕, ⊗, 0̄, 1̄)` is a set `K` with two operations:

| Element / op | Name  | Law                                                | Reading                           |
| ------------ | ----- | -------------------------------------------------- | --------------------------------- |
| `⊕` (plus)   | sum   | associative, commutative, identity `0̄`            | combine alternative derivations   |
| `⊗` (times)  | product | associative, identity `1̄`, distributes over `⊕`  | combine the parts of one derivation |
| `0̄`          | zero  | `a ⊕ 0̄ = a`, `a ⊗ 0̄ = 0̄`                          | "no derivation"; absorbs products |
| `1̄`          | one   | `a ⊗ 1̄ = a`                                        | "empty derivation"; the unit edge |

Two readings make this concrete for a path query:

- **`⊗` joins the parts of a single derivation.** A path `x → y → z` is built by
  combining the part `x → y` with the part `y → z`. `⊗` is how their annotations
  compose along one path.
- **`⊕` combines alternative derivations.** If there are two different paths from `x`
  to `z`, `⊕` merges their annotations into the answer for the pair `(x, z)`.

`0̄` is the annotation of "no path" (identity of `⊕`, absorbs `⊗`: extending a
non-path is still a non-path). `1̄` is the annotation of the trivial empty path
(identity of `⊗`).

## 3.2 One rule, four semirings

The recursive rule is the same in every case. Only `⊕` and `⊗` change:

```
   path(x, z)  ⊕=  path(x, y)  ⊗  edge(y, z)
```

Read it as: the annotation of reaching `z` from `x` gets `⊕`-merged with, for every
intermediate `y`, the product of "reach `y` from `x`" and "the edge `y → z`."

| Property            | `K`            | `⊕`   | `⊗`   | `0̄`  | `1̄` | answers                              |
| ------------------- | -------------- | ----- | ----- | ---- | --- | ------------------------------------ |
| reachability        | `{true,false}` | `OR`  | `AND` | false | true | is there any path?                   |
| shortest path       | `ℝ≥0 ∪ {∞}`    | `min` | `+`   | `∞`  | `0` | length of the shortest path          |
| count paths         | `ℕ`            | `+`   | `×`   | `0`  | `1` | how many distinct paths              |
| bottleneck / widest | `ℝ≥0 ∪ {∞}`    | `max` | `min` | `0`  | `∞` | widest minimum-capacity path         |

Each row is a complete reading of one classic algorithm as the *same* fixpoint with a
different algebra:

- **reachability** `(OR, AND)`: a path exists iff some intermediate reaches `z` AND an
  edge completes it; OR over alternatives. This is the boolean fixpoint of
  [chapter 2](02-fixpoints.md).
- **shortest** `(min, +)`: a path's length is the sum (`+`) of its edge weights;
  the answer for a pair is the smallest (`min`) over alternative paths. This is the
  tropical semiring; chapter 4 schedules it as Dijkstra / Bellman–Ford.
- **count** `(+, ×)`: the number of paths through `y` times the number of edges
  `y → z`, summed over alternatives. Plain arithmetic counts derivations. This is also
  why naive datalog with `(+, ×)` over a cyclic graph diverges: a cycle has infinitely
  many paths (chapter 3 of the parent book, the counting problem).
- **bottleneck** `(max, min)`: the capacity of a path is its narrowest edge (`min`);
  the best path is the widest such (`max`). Maximum-capacity / widest-path routing.

## 3.3 Provenance semirings

Green, Karvounarakis, and Tannen (2007) made this precise for databases: annotate
each base tuple with an element of a semiring, propagate annotations through the
query (`⊗` for joins, `⊕` for unions/projections), and the output annotation answers
a *family* of questions depending on which semiring you chose.[^cite-gkt] The free-est
choice (the polynomial semiring `ℕ[X]`) records the entire derivation as a
polynomial; every other property is a homomorphism out of it. One evaluation, then
read off trust, multiplicity, security level, or probability by mapping the polynomial
into the target semiring.

Abo Khamis et al. push this into recursion: **Datalog°** ("datalog over semirings")
runs a recursive datalog program over an arbitrary (suitably ordered) semiring, so the
recursive `path` rule above computes shortest paths, counts, or reachability by
parameterization, with a single convergence theory.[^cite-datalogo]

```ts
// One semiring interface; four instances; one recursive rule that closes over it.
interface Semiring<K> {
  zero: K;                 // 0̄: identity of plus, absorbs times
  one: K;                  // 1̄: identity of times (the empty/unit derivation)
  plus(a: K, b: K): K;     // ⊕: combine alternative derivations
  times(a: K, b: K): K;    // ⊗: combine the parts of one derivation
}

const reach:      Semiring<boolean> = { zero: false, one: true,  plus: (a,b)=>a||b,        times: (a,b)=>a&&b };
const shortest:   Semiring<number>  = { zero: Infinity, one: 0,  plus: Math.min,           times: (a,b)=>a+b  };
const countPaths: Semiring<number>  = { zero: 0, one: 1,         plus: (a,b)=>a+b,         times: (a,b)=>a*b  };
const bottleneck: Semiring<number>  = { zero: 0, one: Infinity,  plus: Math.max,           times: Math.min    };

// path(x,z) ⊕= path(x,y) ⊗ edge(y,z), as one relaxation pass over all edges.
// Run to a fixpoint (chapter 2). Swap `S` to switch the property computed.
type Mat<K> = Map<string, Map<string, K>>; // path[x][z]
function relax<K>(S: Semiring<K>, path: Mat<K>, edge: Mat<K>): Mat<K> {
  const out = clone(path);
  for (const [x, viaX] of path)             // for each known reach x → y …
    for (const [y, pxy] of viaX)
      for (const [z, eyz] of edge.get(y) ?? []) {   // … extend by edge y → z
        const prev = out.get(x)?.get(z) ?? S.zero;
        set(out, x, z, S.plus(prev, S.times(pxy, eyz))); // ⊕= part ⊗ edge
      }
  return out;
}
```

**Intuition:** a recursive query is an algebra waiting for its semiring; `⊗` walks one
derivation, `⊕` merges the alternatives, and the property you compute is whichever
semiring you plug in.

## Exercises

1. **Define the bottleneck semiring.** State `(K, ⊕, ⊗, 0̄, 1̄)` for widest-path and
   check the two identities: `a ⊕ 0̄ = a` and `a ⊗ 1̄ = a`. What real quantity does
   `1̄ = ∞` represent?

2. **What does `(max, min)` compute?** On `main → run` (cap 5), `run → log` (cap 2),
   `run → parse` (cap 9), give `path(main, log)` and `path(main, parse)` under the
   bottleneck semiring, and say in words what the number means.

3. **Why count diverges on a cycle.** Run `(+, ×)` over the running example's cycle
   `run → parse → lex → run`. Show the count for `path(run, run)` grows without bound
   and explain why reachability `(OR, AND)` does not.

4. **Reachability is a homomorphism of count.** Define `h: ℕ → {true,false}` by
   `h(0) = false`, `h(n>0) = true`. Show `h` carries the count semiring to the
   reachability semiring (`h(a+b) = h(a) OR h(b)`, `h(a×b) = h(a) AND h(b)`). What does
   this say about computing both from one run?

## Answers

1. `K = ℝ≥0 ∪ {∞}`, `⊕ = max`, `⊗ = min`, `0̄ = 0`, `1̄ = ∞`. Identities:
   `max(a, 0) = a` for `a ≥ 0`; `min(a, ∞) = a`. `1̄ = ∞` is the capacity of the
   trivial empty path: traversing nothing imposes no width limit, so the unit edge has
   unbounded capacity.

2. `path(main, parse) = min(5, 9) = 5` (the narrowest edge on `main → run → parse`).
   `path(main, log) = min(5, 2) = 2`. The number is the maximum amount that can flow
   along the best single path: the path's bottleneck capacity.

3. With `(+, ×)`, every lap around `run → parse → lex → run` is a new distinct path,
   so `path(run, run)` counts 1, 2, 3, … laps and never converges. Counting is not
   bounded above on a cyclic graph, so the least fixpoint over `(ℕ, +, ×)` does not
   exist for cycles (you need a `∞` or a "≥1" collapse). Reachability `(OR, AND)`
   saturates: once `path(run, run) = true`, `OR`-ing more `true` changes nothing, so
   the fixpoint settles in one round. Idempotent `⊕` (like `OR`, `min`, `max`) is what
   makes cyclic convergence safe; non-idempotent `⊕` (`+`) needs a finite/acyclic
   structure or a different convergence argument.

4. `h(a + b)`: if either of `a, b > 0` then `a + b > 0` so `h = true = h(a) OR h(b)`;
   if both `0` then `h = false`. `h(a × b)`: positive iff both positive, matching `AND`.
   So `h` is a semiring homomorphism. Consequence: you could run the count semiring
   once (where it converges) and read off reachability by applying `h`, the
   provenance-semiring idea: evaluate in the richer semiring, project to the property
   you want.

## Citations

- Green, T. J., Karvounarakis, G., Tannen, V. *Provenance semirings.* PODS 2007. The
  paper that unified annotated relations under a single algebra; read §2–3.[^cite-gkt]
- Abo Khamis, M., Ngo, H. Q., Pichler, R., Suciu, D., Wang, Y. R. *Datalog° / Convergence
  of Datalog over (Pre-)Semirings.* PODS 2022 / JACM. Recursion over arbitrary
  semirings with one convergence theory.[^cite-datalogo]

[^cite-gkt]: Green, Karvounarakis, Tannen, *Provenance Semirings*, PODS 2007.
[^cite-datalogo]: Abo Khamis et al., *Convergence of Datalog over (Pre-)Semirings*
(Datalogo), PODS 2022.
