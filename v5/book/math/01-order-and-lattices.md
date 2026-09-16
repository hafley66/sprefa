# 1. Order and lattices

**The question:** the parent book keeps saying the fixpoint "settles in exactly one
place" and that incremental maintenance "merges partial answers." Merges them how?
When two rules each derive a fact about `run`, what combines them, and why is the
combination unambiguous? The answer is a lattice. This chapter builds it from the
order relation up.

We reuse the running example: `main → run → parse → lex`, with `lex → run` (a
cycle), `run → log` (a sink), and `helper` (dead).

## 1.1 Partial orders (posets)

A **partial order** is a set with a relation `≤` that is:

| Law           | Statement                          | Reading                                  |
| ------------- | ---------------------------------- | ---------------------------------------- |
| reflexive     | `a ≤ a`                            | everything is `≤` itself                 |
| antisymmetric | `a ≤ b` and `b ≤ a` ⇒ `a = b`      | no two distinct elements tie both ways   |
| transitive    | `a ≤ b` and `b ≤ c` ⇒ `a ≤ c`      | the order chains                         |

A set with such a relation is a **poset**. The word *partial* is the point: some
pairs are **incomparable**, neither `a ≤ b` nor `b ≤ a`. Contrast a **total** order,
where every pair is comparable (the integers under `≤`).

In the running example, "reaches" is a poset on the four SCCs once you collapse the
cycle (chapter 6). `[main]` reaches `[run,parse,lex]` reaches `[log]`, but `[log]`
and `[helper]` are incomparable: neither reaches the other.

```ts
// A poset is a carrier set plus a `leq` satisfying the three laws.
// We never store the laws; we promise the relation obeys them.
interface Poset<T> {
  leq(a: T, b: T): boolean; // reflexive, antisymmetric, transitive
}
```

## 1.2 Join and meet

Given two elements, two questions:

- **Join** (`a ⊔ b`, the *least upper bound* / lub): the smallest element that is `≥`
  both. The tightest common over-approximation.
- **Meet** (`a ⊓ b`, the *greatest lower bound* / glb): the largest element that is `≤`
  both. The tightest common under-approximation.

A poset where every pair has both a join and a meet is a **lattice**. Add `top` (`⊤`,
above everything) and `bottom` (`⊥`, below everything) and the lattice is **bounded**.
A **complete lattice** has a join and a meet for *every* subset, not just pairs;
`⊤ = join of everything`, `⊥ = join of nothing`.

The join is the merge operator. When two derivations each pin down part of an answer,
their join is the unique smallest answer consistent with both. "Unique" is exactly
antisymmetry: if two elements were both least-upper-bounds, each would be `≤` the
other, so they would be equal.

## 1.3 Four lattices you already use

| Lattice          | Carrier              | `≤`        | join `⊔`   | meet `⊓`   | `⊥`     | `⊤`     |
| ---------------- | -------------------- | ---------- | ---------- | ---------- | ------- | ------- |
| numbers          | a bounded `[lo, hi]`  | `≤`        | `max`      | `min`      | `lo`    | `hi`    |
| subsets          | `P(U)` (powerset)    | `⊆`        | `∪`        | `∩`        | `∅`     | `U`     |
| booleans         | `{false, true}`      | `false≤true` | `OR`     | `AND`      | `false` | `true`  |
| divisibility     | positive integers    | `a ∣ b`    | `lcm`      | `gcd`      | `1`     | (none)¹ |

Read each row as one fact about merging:

- **numbers:** the merge of two distances is the smaller (`min`) or the larger
  (`max`) depending on which direction you bound. Chapter 4 uses `min` for shortest
  paths.
- **subsets:** the merge of two reachability sets is their union. The set of facts
  the engine has derived is a subset of the universe of possible facts; deriving more
  moves up.
- **booleans:** the merge of two "is reachable?" answers is `OR`. This is the
  reachability lattice (chapter 2 ties the boolean fixpoint to reachability).
- **divisibility:** included to show `≤` need not be numeric size. `6 ∣ 12` but `4`
  and `6` are incomparable; their join is `lcm(4,6) = 12`, their meet is `gcd = 2`.

## 1.4 The point

A lattice is the answer to "how do I combine two partial answers into one, without
having to pick an order?" The join is associative, commutative, and idempotent, so
the merge does not care which partial answer arrived first or how many times. That is
exactly what a datalog engine needs: rules fire in arbitrary order, the same fact can
be derived many ways, and the result must be the same. The engine merges by set union
(`⊆` lattice); chapter 4 generalizes the value at each fact to an arbitrary lattice.

```ts
// The minimal lattice an engine merges over. `bottom` is the empty starting
// answer; `join` merges two partial answers; `leq` decides "did we stop growing?"
interface Lattice<T> {
  leq(a: T, b: T): boolean; // the partial order (1.1)
  join(a: T, b: T): T;      // least upper bound: merge two answers (1.2)
  bottom: T;                // the identity of join: join(bottom, a) === a
}

// The set-union lattice the core engine uses: a fact set, merged by union.
const subsetLattice = <E>(): Lattice<Set<E>> => ({
  leq: (a, b) => [...a].every((x) => b.has(x)), // a ⊆ b
  join: (a, b) => new Set([...a, ...b]),         // a ∪ b
  bottom: new Set<E>(),                          // ∅
});
```

**Intuition:** a lattice is how you merge two partial answers unambiguously: the join
is the unique smallest answer consistent with both.

## Exercises

1. **Identify the operators.** For the powerset of `{run, parse, lex}` under `⊆`, give
   `{run} ⊔ {parse}`, `{run, parse} ⊓ {parse, lex}`, `⊥`, and `⊤`.

2. **Booleans are numbers.** Show the boolean lattice `{false ≤ true}` is the numbers
   lattice on `{0, 1}` with `max`/`min`. Which is join, which is meet?

3. **A poset that is not a lattice.** Give a four-element poset where some pair has no
   least upper bound. (Hint: two minimal elements both below two maximal elements,
   with no single one in between.)

4. **The powerset is complete.** Argue that for any set `U`, `(P(U), ⊆)` is a complete
   lattice: every subset of `P(U)` (a family of sets) has both a join and a meet.

5. **Reaches is a poset, not a lattice (yet).** On the four SCCs of the running
   example, is "reaches" a lattice? Find a pair with no join inside the four-element
   set.

## Answers

1. `{run} ⊔ {parse} = {run, parse}` (union). `{run, parse} ⊓ {parse, lex} = {parse}`
   (intersection). `⊥ = ∅`. `⊤ = {run, parse, lex}`.

2. Map `false → 0`, `true → 1`. Then `OR = max` (join: `max(0,1) = 1 = true`) and
   `AND = min` (meet). The order `0 ≤ 1` matches `false ≤ true`.

3. Carrier `{⊥, a, b, ⊤}` is a lattice. Instead take `{a, b, c, d}` with `a ≤ c`,
   `a ≤ d`, `b ≤ c`, `b ≤ d`, and `a, b` incomparable, `c, d` incomparable. Then
   `a ⊔ b` would have to be a single least element `≥` both `a` and `b`; both `c` and
   `d` qualify as upper bounds but neither is `≤` the other, so there is no *least*
   one. No join, so not a lattice.

4. For a family `F ⊆ P(U)`: its join is `⋃ F` (the union of all sets in `F`), which is
   the smallest set containing every member, so it is the lub. Its meet is `⋂ F` (with
   `⋂ ∅ = U`), the largest set contained in every member, the glb. Both exist for every
   `F`, including infinite families, so `(P(U), ⊆)` is complete. `⊥ = ∅`, `⊤ = U`.

5. Not a lattice. Take `[log]` and `[helper]`: an upper bound is an SCC that both
   reach, but `[log]` reaches nothing and `[helper]` reaches nothing, and there is no
   element `≥` both inside the set, so no join. "Reaches" is a poset on the SCCs, and
   it becomes a lattice only if you close it under joins (add a synthetic top), which
   is not what reachability is. The merge lattice the engine actually uses is the
   powerset of facts (exercise 4), which is complete.

---

[^1]: Divisibility on *all* positive integers has bottom `1` but no top: there is no
finite number divisible by every integer. Restricting to divisors of a fixed `n` gives
a complete lattice with `⊤ = n`.

## Citations

- Davey, B. A. and Priestley, H. A. *Introduction to Lattices and Order*, 1st ed.,
  Cambridge University Press, 1990. The standard textbook; chapters 1–2 cover posets,
  lattices, and completeness.[^cite-dp]
- Birkhoff, G. *Lattice Theory*, American Mathematical Society Colloquium
  Publications, vol. 25, 1940. The origin text; dense but definitive on complete
  lattices.[^cite-bh]
- Stoll, R. R. *Set Theory and Logic*, Dover, 1979 (reprint of 1963). Cheap, careful
  treatment of relations, orders, and bounds from first principles.[^cite-stoll]

[^cite-dp]: Davey & Priestley, *Introduction to Lattices and Order*, CUP 1990.
[^cite-bh]: Birkhoff, *Lattice Theory*, AMS 1940.
[^cite-stoll]: Stoll, *Set Theory and Logic*, Dover 1979.
