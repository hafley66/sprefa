# Re-edging: two walks over one `:` table

Caret 3 design page. Rows, trees, traces. No mermaid. Sites cite today's code.

## TOC

- [Source and expansion](#source-and-expansion)
- [Rows](#rows)
- [Tree](#tree)
- [Two walks, one probe](#two-walks-one-probe)
- [Traces](#traces)
- [Re-edging: nothing overwrites](#re-edging-nothing-overwrites)
- [`key` decision](#key-decision)
- [Forks for Chris](#forks-for-chris)

## Source and expansion

```
(^A: (* (: b int))): (* (: greet (* (: u A) (: out str)))
                        (<- (greet ?U ?Out) (A ?U ?Name) (str.cons "hi " ?Name ?Out)))
(: A () Mixin)
(: Mixin (* (: greet (* (: u A) (: out str)))))
```

`dl8 expand` (macro emits two rows, never looks inside the body product):

```
(: A (* (: b int)))
(: A () (* (: greet (* ...)) (<- ...)))
```

## Rows

`(: owner name target index)`, keys `(owner, name)` and `(owner, index)`
(`src/_2_lower/_9_kernel.rs:36-39,66`). `()` label mints name = index.

| owner | name | target | index | what |
|---|---|---|---|---|
| M | A | P0 | 0 | module edge: the binding |
| P0 | b | int | 0 | column, arity counts it |
| P0 | 1 | P1 | 1 | anonymous, product-valued: namespace |
| P1 | greet | G | 0 | own label of the namespace |
| P1 | 1 | R | 1 | the rule, anonymous member of P1 |
| G | u | P0 | 0 | column |
| G | out | str | 1 | column |
| P0 | 2 | Mixin | 2 | second namespace edge |
| Mixin | greet | G2 | 0 | shadows P1.greet on the walk |

Columns take indices in source order; the `^` body takes the next index.
No reserved slot; "namespace is edge 1" holds because `b` is the one column.

## Tree

```
M
└─ A ─> P0                    M edge 0: the binding
   ├─ b ─> int                P0 edge 0, column
   ├─ 1 ─> P1                 P0 edge 1, anonymous, walked
   │   ├─ greet ─> G          P1 edge 0
   │   │   ├─ u ─> P0
   │   │   └─ out ─> str
   │   └─ 1 ─> R              P1 edge 1, rule, anonymous
   └─ 2 ─> Mixin              P0 edge 2, anonymous, walked after P1
       └─ greet ─> G2         shadowed_member(P0, greet)
```

## Two walks, one probe

`scoped()` at `src/_2_lower/_10_index.rs:170-193` is the `.closest()` walk
today: probe `(owner, name)`, then `owner = parent(owner)`, cycle-guarded.
Caret 3 adds the second direction with the same probe.

| walk | direction | step relation | today | caller |
|---|---|---|---|---|
| scope (`.closest()`) | up | `parent(owner)`: the owner of the edge whose target is `owner` (`_3_check/_1_graph.rs:55`, `_10_index.rs:191`) | built | free atom in a rule body or expression (`_8_express.rs:127`) |
| prototype (Ruby ancestors, JS `__proto__`) | down | anonymous edges of `owner` whose target is a product, index order, depth-first | not built | dot segment `A.greet` (`_8_express.rs:105`), dotted head `(A.greet ..)` (`_7_execute.rs:270`), and each stop of the up walk |

One probe, two `next` relations. Not a macro: a macro cannot see rows that
arrive from another unit, an import, or a comptime round.

## Traces

`A.greet` in an expression:

```
step 0  owner=P0   probe (P0, greet)      miss
step 1  proto(P0) = [P1, Mixin]           index order
step 2  owner=P1   probe (P1, greet)      hit G       value = G, stop (first wins)
diag    (Mixin, greet) also on the walk   shadowed_member(P0, greet), prelude rule
```

`(A ?U ?Name)` inside rule R's body, free atom `A`:

```
step 0  owner=R    probe (R, A)           miss   proto(R) = []
step 1  owner=P1   probe (P1, A)          miss   proto(P1) = [R] (rule: skipped, fork 1)
step 2  owner=P0   probe (P0, A)          miss   proto(P0) = [P1 visited, Mixin] miss
step 3  owner=M    probe (M, A)           hit P0  steady state: the module binding
```

`(greet ?U ?Out)` as R's head: step 0 R miss, step 1 P1 hit G. The rule
releases rows into G. `AGENTS.md:92` holds: R has no name, `1` is an index.

## Re-edging: nothing overwrites

| form | row | key `(owner, name)` | effect |
|---|---|---|---|
| `(: A c str)` | P0 c str 3 | fresh | new column; arity 2 |
| `(: A b str)` | P0 b str 3 | collides with row `P0 b int 0` | key-1 collision; diagnostic name unverified (no `duplicate_*` in `src/_2_lower`, `src/_3_check`, `prelude/`) |
| `(: A () X)` | P0 3 X 3 | fresh (name = index) | walk order extended; no row replaced |
| `(: A greet H)` | P0 greet H 3 | fresh on P0 | own label wins over P1.greet at step 0; `shadowed_member(P0, greet)` |

Arity of P0 = edges whose target is a type or value node = 1 (`b`).
Anonymous product-valued edges are skipped (`_3_check/_1_graph.rs:60`).

## `key` decision

`Key` (interned constructor `keyed_edge` reads at
`prelude/3_derived_rules.dl7:61-63`) becomes `dl6.key`, declared in
`std/dl6.dl7`. The rules move with it: `keyed_edge`, `key_predecessor`,
`key_has_predecessor`, `key_rank`, `composite_key`
(`prelude/3_derived_rules.dl7:61-87`, decls `prelude/1_declarations.dl7:75-100`),
`program_key`, `program_key_position` (`prelude/1_declarations.dl7:255-262`).
The `:` row's own two key sets stay: a separate uniqueness concept, not an
emitter annotation. Own lane after caret 3.

## Forks for Chris

| # | fork | options |
|---|---|---|
| 1 | proto step into a rule-valued anonymous edge | skip rules (`not (rule ?T)`), or walk them (exposes `head`/`body` labels) |
| 2 | shadow policy | diagnostic only, first wins (proposed); or check error |
| 3 | up walk probes each stop's proto chain (Ruby: lexical, then ancestors) | yes (trace above); or up walk is own labels only |
| 4 | `A.1` and `A.proto` | plain label lookups, no int-segment arm (proposed) |
