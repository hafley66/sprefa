# acyclic: the three spellings

2026-08-14. User decisions already made: divergence gets DETECTED ("no,
detect it"); the guard defaults ON for every single-self-ref column ("acyc
default is whatever we want", coordinator picked default-on, opt-out spelling
reserved); bounds and all grouping spell in parens (`rulings.pl:
template_bound_spelling`). This file records the three candidate spellings for
the acyclic constraint itself, undecided.

## The plane split that shapes all three

| | option | acyclic |
|---|---|---|
| constrains | one row's value: payload-or-none | the graph the column forms across rows |
| plane | column type | rel-level invariant |
| checkable | at the row | only by walking other rows |

The dependency: `none` is acyclicity's base case. A required self-ref column
(`parent: node`, no option) that is also acyclic is unsatisfiable for any
nonempty rel — every chain needs a bottom and only `none` supplies one. So the
linear case always pairs acyclic with option, while acyclic alone still
applies where option never appears (an edge rel between two entity endpoints
wanting DAG-ness). Hosting acyclic UNDER option (`option.acyclic`) would
strand that second case; the spellings below keep the planes separate.

## A — wrapper composition (compiler guard)

```
rel node(name: text, parent: acyclic(option(node))) key(1).
```

Fits the `type_wrapper` inventory (`0_type_plane.pl:145-151`) and the parens
law. Guard = arrival-time chain walk, O(chain) per arrival, bounded because
one self-typed column means out-degree <= 1 (the minted companion rel's
UNIQUE(owner) enforces it); reaching the starting node throws
`parent_cycle(Node, path([...]))`. rx lowering: expand over the parent join
until none.

## B — rel-level constraint clause

```
rel node(name: text, parent: option(node)) key(1) acyclic(parent).
```

Reads as what it is (a graph invariant beside `key`), and generalizes to
multi-column shapes (`acyclic(left, right)` on a tree enum, or an edge rel).
Cost: a new decl-clause production in the grammar, and it is the only
spelling of the three that puts constraint vocabulary outside the type
expression.

## C — stdlib refinement template (library code, rides option-in-stdlib)

```
rel node(name: text, parent: std.acyclic(node)) key(1).
```

Pure dl6: the template mints a set-shaped reachability rule plus
`violation(Node) <- reaches(Node, Node)` as a diagnostic rel. Legal because
set recursion saturates on cyclic data (certified 2026-08-14,
`fixtures/17_recursive_enum.pl`), so the checker terminates on the very thing
it checks. Cost fork vs A: maintains a transitive closure on every arrival
(more write cost) and yields a queryable `reaches` rel for free. Depends on:
enum/refinement templates + the mount machinery
(`plans/2026-08-14-option-in-stdlib.PLAN.md` slices 1 and 4).

## Standing recommendation (coordinator, not yet user-decided)

A now as the compiler guard; C later as the library form once the
option-in-stdlib arc lands, with A's spelling desugaring to C's semantics at
that point so programs never change. B stays unbuilt.

## Addendum 2026-08-14 (user decision)

Spelling A wins: user said "A for now then". Default-on, so
`parent: option(node)` alone carries the guard and `acyclic(option(node))`
is the explicit synonym. `acyclic(...)` around anything that is not an
option of the declaring rel is a named throw. B stays unbuilt; C stays the
later library form.
