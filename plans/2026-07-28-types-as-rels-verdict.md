# TYPES-AS-RELS VERDICT (lab, 2026-07-28)

Contract: `plans/2026-07-28-types-as-rels-header.md`. Lab:
`v6/prolog/labs/types_as_rels/` (dies on landing; last full copy is the commit
recorded at the bottom of this file).

Run: `swipl -q -l v6/prolog/labs/types_as_rels/types_as_rels.pl -g go -g halt`
-> 31 PASS, 0 fail, nothing else on stdout or stderr.
Untouched and re-verified: conformance `go.pl` 110 pass / 0 fail;
`v6/prolog/compile/scripts/roundtrip.sh` ALL GRADES PASS.

---

## VERDICT LINE

**The unification hypothesis HOLDS for the value plane, with two amendments,
and it CRACKS on exactly one thing: cycles.**

- Holds: a struct decl is a `rel` decl with three declaration facts pinned
  (key = all content columns, id column = a content bind, lifetime = the IVM
  support the engine already counts). An enum is N variant rels sharing one id
  space. Nesting is never physical. The lab derives all of it from machinery
  that is already ruled and already emitted; the only new NAME needed is one
  stdlib bind, `content_id/N`, which is an ordinary `bind_goal`
  (ARCH.pl:302), not a construct.
- Amendment 1 (shape): variable-arity values do not fit. A list's identity is
  its whole element sequence, and `key(...)` names positions inside ONE rel,
  so an indexed list header row cannot state its own key. Fixed-arity cons
  cells restore the property. Souffle made the same call for the same reason
  (records are fixed-length; lists are `[head, tail]` with `nil`).
- Amendment 2 (merge): see the lattice section. The kind words (`set`, `log`,
  keyed-latest) are already three named merge rules, and the policy bundle
  reads better with a merge bit than without one.
- The crack: **content-addressed identity cannot express a cyclic reference
  graph**, because a parent's id is computed FROM its children's ids. Every
  interned value graph is a DAG by construction (graded:
  `interned_graph_is_a_dag`). Programs that need a cycle (mutually recursive
  nominal types, doubly linked structures, a parent pointer) must use
  EXTRINSIC keys, and under extrinsic keys support counting stops being a
  complete collector (graded: `extrinsic_key_cycle_leaks` -- two rows, no
  root, both support 1, neither reachable, `collect/3` removes nothing).

So the honest shape is: ONE construct (`rel`), TWO identity policies (content
and extrinsic), and the cycle is the line between them. "One construct plus a
policy bundle" is right; "one policy bundle" is not.

---

## THE SHORTHAND TABLE

What a struct decl means, term by term, in the surface that already exists
(`rel name(cols) [log|set] [keep(..)] [key(..)]`, SYNTAX.md:52):

| shorthand word | expands to | already exists? |
|---|---|---|
| `struct` | `rel` + `set` | yes, both |
| struct identity | `key(<every content column>)` | yes, `keyed/2` (engine.pl:97) |
| the id column | one extra column bound by `Id := content_id(Type, Cols...)` | `bind_goal` exists; `content_id` is a new stdlib function |
| immutability | nothing. A changed field is a different key, so it is a different row. Keyed replace never fires | yes, by omission |
| lifetime | nothing. A row lives while a rule derives it; support counting is the store's job | yes (ARCH.pl:151 `count_ivm`) |
| nested field | an ordinary column holding a child id | yes, a plain int column |
| `enum E { A{..}, B{..} }` | N `rel`s, one per variant, sharing the id space because the type name is part of the content hash | yes |
| "which variant" | a derived level rule `body_tag(Id, 'page') <- body_page(Id, _).` | yes, a level rule |
| `[T]` / `list(T)` | `rel cons(id, head, tail) set key(2,3)` + `rel nil(id) set key(1)` | yes |
| dot path `a.b.c` | a join chain over the ref columns | yes, ordinary body atoms |

Nothing in the right-hand column is new. That is the whole result.

---

## THE FIVE MANDATORY CHECKS

### 1. JSON round-trip, byte-identical, with shared substructure

`round_trip_term_identical`, `round_trip_text_byte_identical`,
`shared_subtree_stored_once`, `worked_example_row_and_edge_counts`,
`cons_round_trips_too`.

The worked value (schema.pl `route_tree`) is a route with an enum body and two
children, one of which repeats the SAME view value. 245 characters of json in,
245 identical characters out, and the term is `==` to the input term.

Storage receipt for that value (indexed lists, content ids): **9 rows, 4
edges** -- 3 `route`, 3 `list`, 1 `view`, 1 `body_page`, 1 `body_redirect`.
The tree TEXT contains `"title":"T"` twice; the graph holds ONE `view` row.
That is the "tree view duplicates, graph stores once" claim, graded.

### 2. Policy-bundle derivation

`policy_bundle_insert_share_release`, `policy_bundle_support_counts`.

The op tape and its deltas, produced by the model, not asserted by prose:

| op | boundary deltas | support after |
|---|---|---|
| insert value V | `[+row(list, 1)]` | 1 |
| insert the SAME V from a second parent | `[]` | 2 |
| release one parent | `[]` | 1 |
| release the last parent | `[-row(list, 1)]` | 0, row gone |

The empty delta on the share step is not new behavior: an equal-row write is a
no-op (rulings.pl `r_equal_row_write`, engine.pl:247-248). The support rising
to 2 is not new either (ARCH.pl:68-71, per-ROW origin support). PASS = no new
construct was needed.

### 3. Domination scenario pair

Two roots sharing one view value. Mint order under the counter policy (the
readable one; ids are quoted by mint position so the log is hand-checkable):

```
1  list("x","y")     2  view("T", 1)     3  body_page(2)
4  list()            5  route("/a", 3, 4)     6  route("/b", 3, 4)
```

| scenario | tick log (the boundary delta set) | rows left |
|---|---|---|
| release root 5, shared child (`domination_shared_child_survives`) | `[-row(route, 5)]` | 5, support(3) drops 2 -> 1 |
| then release root 6, sole owner (`domination_sole_owner_cascades`) | `[-row(body_page,3), -row(list,1), -row(list,4), -row(route,6), -row(view,2)]` | 0 |
| SAME store, SQL `ON DELETE CASCADE` on release 5 (`fk_cascade_kills_shared_child`) | `[-row(body_page,3), -row(list,1), -row(list,4), -row(route,5), -row(view,2)]` | 1 (route 6, **dangling**) |

The FK run also leaves `ref(6,3)` and `ref(6,4)` pointing at nothing
(`fk_cascade_leaves_dangling_refs`). That is the decisive pricing result for
Q3(a): **SQL's own cascade implements the wrong semantics for shared
children.** It walks the dead parent's children with no regard for other
referrers.

Cascade is ONE tick, not one tick per level: `collect/3` is a set-at-a-time
fixpoint, the same shape as the reference engine recomputing a level closure
per tick (engine.pl:286, 295). The removal ORDER inside the tick is not
observable, because the boundary delta is a set (rulings.pl `r7_boundary_diff`).

### 4. Match-path lowering at depth 1, 2, 3

Exact emitted strings (`match_path_depth_1/2/3`, text shape follows
lower.pl:304-333: quoted identifiers, `WITHOUT ROWID` PKs):

```sql
-- depth 1: route.path
SELECT r0."path" FROM "route" r0 WHERE r0."id" = ?
-- depth 2: route.body(redirect).to
SELECT r1."to" FROM "route" r0 JOIN "body_redirect" r1 ON r1."id" = r0."body" WHERE r0."id" = ?
-- depth 3: route.body(page).view.title
SELECT r2."title" FROM "route" r0 JOIN "body_page" r1 ON r1."id" = r0."body" JOIN "view" r2 ON r2."id" = r1."view" WHERE r0."id" = ?
```

`match_path_join_count_is_depth` grades joins = depth - 1 exactly. The inline
json1 counterpart (`inline_json_alternative_is_one_table`) has zero joins:

```sql
SELECT json_extract(r0."body", '$.view.title') FROM "route" r0 WHERE r0."id" = ?
```

Depth cost, counted rather than measured (the honest form; nothing here is
benchmarked):

| representation | index probes per read | bytes for a child shared by k parents | child matchable on its own? | child shares? |
|---|---|---|---|---|
| ref columns, depth d | d PK probes on `WITHOUT ROWID` tables | 1 copy | yes | yes |
| inline json1 | 0 probes, 1 json parse per row | k copies | no (must parse) | no |

Break-even is k = 1: a child with exactly one referrer that is never matched
on independently pays only the probe, and inline wins. Every k > 1 pays k
copies. UNMEASURED: no benchmark backs the constant factors. Nearby receipts
that make the storage side worth caring about: the v5 root db sits at a 39x
db/corpus ratio (CLAUDE.md lazy-rel-tier row) and the index audit removed
117.7MB by dropping 509 indexes (dc9b67b1), so column and index count
dominates that database, not row count.

### 5. Compactness pricing, three spellings, one example

All three are the SAME worked example, and all three expand to the SAME six
tables (`spellings_expand_to_same_tables`, each expansion written out
separately in `lowering.pl` so the check is not vacuous).

**(a) json braces** -- 165 chars, 5 new constructs

```
rel route { path: text, body: body, children: [route] } value.
enum body { page { view: view }, redirect { to: text } }
rel view { title: text, tags: [text] } value.
```

**(b) prolog functors** -- 165 chars, 4 new constructs

```
rel route(path: text, body: body, children: list(route)) value.
rel body(page(view: view) ; redirect(to: text)) value.
rel view(title: text, tags: list(text)) value.
```

**(c) sql rels, the literal expansion** -- 232 chars, 0 new constructs

```
rel route(id, path, body, children) set key(2, 3, 4).
rel body_page(id, view) set key(2).
rel body_redirect(id, to) set key(2).
rel view(id, title, tags) set key(2, 3).
rel cons(id, head, tail) set key(2, 3).
rel nil(id) set key(1).
```

| criterion | (a) braces | (b) functors | (c) rels |
|---|---|---|---|
| characters | 165 | 165 | 232 |
| new constructs beyond SYNTAX.md | 5 | 4 | 0 |
| distance from json | nearest (braces and brackets are json's own) | one step (functor instead of brace) | far (ids and key positions are visible) |
| distance from current `.dl` decls | far: `{...}` ALREADY has a meaning, the json object literal (SYNTAX.md:73), so a decl in braces overloads a live production | near: `col: type` inside parens is already a DIRECT MATCH (SYNTAX.md:80 named args), and variant functors are already how the corpus writes variants (`fresh(Tag, Body)`, `error(Status)`, dl_view/async_state_machine_with_pattern_scan.dl) | zero: it IS the current surface |
| hazard | brace collision above | `;` is prolog's disjunction, already in the reader, so no new token | none, it just says more |

Ranking under those criteria, with the criteria left visible so they can be
reweighted: **(b) > (c) > (a)**. (b) buys the compaction for the fewest new
ideas and reuses two spellings the corpus already contains; (c) is the
zero-risk floor and stays available as the desugared form the user can always
drop to; (a) is the prettiest and the only one with a real parse hazard.
No fiat: this is a ranking by stated criteria, and SLOT-DECL-SPELLING is
still the user's.

Note on the `value` policy word in (a) and (b): it is optional sugar. Spelling
(c) proves the policy is fully expressible with `key(...)` plus the id bind,
so the word buys brevity only. If it stays, it needs a vocabulary-law-legal
name; SQL offers `unique` and `distinct`, and the honest fourth option is no
word at all.

---

## Q1 -- DECL UNIFICATION

What actually differs between `rel route(id, body)` and a struct decl of the
same shape: **nothing structural**. Three declaration facts, and only the
first two are even written down:

| aspect | plain rel | struct shorthand |
|---|---|---|
| storage | one table | the same table |
| identity | `key(Positions)`, or all columns by default | `key(all content columns)`, plus a derived id column |
| mutation | keyed replace, latest wins | never reachable: a changed field is a different key |
| subscribability | yes | yes, identically (see ambiguity A3) |
| lifetime | derivable = alive | the same |

### Enum layouts, priced

| layout | tables | null padding | "which variant is id X" | add a variant | match cost when the pattern NAMES the variant |
|---|---|---|---|---|---|
| (a) one table + tag column | 1 | YES: every variant column must be nullable, which collides with lower.pl:332-333 emitting `NOT NULL` on every column | 1 PK probe | `ALTER TABLE` | 1 table, `WHERE tag = 'page'` |
| (b) N variant tables, shared id space | N | none | N probes, or 1 probe on a derived tag view | new table, additive | 1 table, no tag predicate |
| (c) variant tables + tag edge table | N+1 | none | 1 probe then 1 read | new table, additive | 2 tables |
| (d) souffle's shape: tag + ONE payload ref | 2 | none | 1 probe | none (the payload is just another record) | 2 tables |

Recommendation: **(b), with (c)'s tag table as a DERIVED rel, not a stored
one**. `body_tag(Id, 'page') <- body_page(Id, _).` is an ordinary level rule
union over the variant rels; it answers the "which variant" probe, it costs no
storage decision, and IVM maintains it. That collapses the (b)-vs-(c) choice
into "do you also want the view", which is a per-program question, not a
language one.

The existing envelope enum in each: `FetchResult { Fresh{tag, body},
Unchanged, Error{status} }` becomes, under (b), `fetch_fresh(id, tag, body)`,
`fetch_unchanged(id)`, `fetch_error(id, status)`. `Unchanged` has no fields,
so its content key is empty and the table can hold exactly one row: the
variant degenerates to a constant. Souffle reached the identical conclusion
mechanically (see prior art), encoding zero-field ADT branches as a bare
integer with no record at all.

---

## Q2 -- MINTING POLICY (surrogates themselves are ruled)

| policy | id is | order-dependent? | dedup | minting state | tick-log grade | width |
|---|---|---|---|---|---|---|
| content hash | `f(type, content)` | NO (graded: `content_ids_order_independent`) | free | none: a pure bind, no counter, no lock | stable across runs and machines | wide (sha); truncation is a birthday risk |
| dense counter | next integer | YES (graded: `counter_ids_order_dependent`) | needs the lookup anyway | a counter and a uniqueness seam | differs run to run unless ids never enter the log | small, index-friendly |

`counter_ids_order_dependent` is the sharp one: building tree_a then tree_b vs
tree_b then tree_a assigns DIFFERENT ids to the same values. The bare id SET
is identical either way, which is why the check compares the identity
ASSIGNMENT (value -> id) rather than the ids.

This is a live conflict between two standing rulings: `storage_integer_keys`
("pick integers every time") wants dense ints, `salt_minting`
(`content_addressed`) wants ids that are a function of content. They meet in a
dictionary table (hash -> dense int), which reintroduces exactly the stateful,
order-dependent minting the content policy removed. Souffle's flyweight IS
that dictionary and accepts the order dependence, because souffle never diffs
logs across runs. We do (stopping-point item 9). The repair is graded:
`rendered_text_stable_under_both_policies` -- the same value prints identically
under both policies and both build orders, so **a tick log that prints values
never sees the difference, and a tick log that prints ids can never be
byte-diffed.** That is a hard prerequisite on item 9, not a preference.

Where each breaks:
- Mutation of a shared node: content ids have no mutation. A changed field
  mints a new id; the old row lives until its support drops. Anything that
  wanted "the same node, changed" wanted an extrinsic key.
- Retraction counting, two parents one child: graded above, works, and works
  because support is per-row origin.
- Key/Q8: under content ids the key IS every content column, so `Key(Type)`
  never appears on a value rel. It stays where Q8 already put it, on state
  rels with an extrinsic key.

---

## Q3 -- DOMINATION SPELLING (the cascade question)

| option | what the generated code contains | tick log when a parent dies | rx lowering |
|---|---|---|---|
| (a) `FOREIGN KEY ... ON DELETE CASCADE` in DDL | one clause per ref column, plus `PRAGMA foreign_keys = ON` | WRONG SET: the shared child dies too, and surviving parents dangle (graded) | **NONE. See finding A6** |
| (b) domination as IVM support | nothing. No new emitted code at all | the correct set, in one tick | `scan` over ref deltas, then `expand` for the transitive rounds |
| (c) explicit edge-table column marking | a marker column plus a rule that reads it | same as (b) if the marker is honest, wrong if it lies | same as (b) |

**Answer: it dissolves.** Option (b) needs zero new machinery, zero new
surface, and zero new emitted code, and the reason it is COMPLETE rather than
merely usually-right is the acyclicity theorem: every ref points at a strictly
earlier mint (`interned_graph_is_a_dag`), so the live set and the
root-reachable set are identical (`support_equals_reachability`). Plain
refcounting leaks on cycles; here there are no cycles to leak on.

Three named gaps, none of which is a reason to build machinery:

1. **Cascade is not declarable; it is a property of how the row got there.**
   `second_root_blocks_cascade`: give the shared child a second support (an EDB
   arrival, a second demand) and it never dies, releasing every parent. Under
   ARCH.pl:68-71 that is the rule, not a defect. But it means "child dies with
   parent" cannot be promised by a decl word, and a surface that promises it
   would be lying.
2. **FK cascade is the wrong semantics** and must not be emitted for interned
   tables (receipts above). It is correct only where every child has exactly
   one parent, which is precisely the case the non-domination default avoids.
3. **Depth cost.** A cascade of depth d resolves in ONE tick but costs f(depth)
   statement rounds against `n1_statement_budget`. This is the same hazard
   already filed under the `subscription_kernel` ruling (redteam A1, recursive
   rels in scope cones), still unowned.

The rx lowering for (b), written out per the snippet law:

```ts
// support counting: one scan over ref deltas, no per-row query
const support$ = merge(refAdded$, refRemoved$).pipe(
  scan((counts, delta) => bump(counts, delta.childId, delta.sign), new Map<Id, number>()),
);
// the cascade rounds: expand, the same operator the tsv2 tickLoop already uses
const collected$ = support$.pipe(
  map(counts => zeroSupport(counts)),
  expand(zeros => zeros.length ? of(zeroSupport(applyRemovals(zeros))) : EMPTY),
);
```

---

## Q4 -- EDGE TABLES AS RELS

Yes, ordinary rels. The elem table the indexed-list modelling needs is three
columns and a two-column key:

```sql
CREATE TABLE "list_elem" ("list" INTEGER NOT NULL, "index" INTEGER NOT NULL, "item" INTEGER NOT NULL, PRIMARY KEY ("list", "index")) WITHOUT ROWID
CREATE INDEX "list_elem_item" ON "list_elem" ("item")
```

Nothing about it is privileged: dl rules can join it, measures can count it,
the LSP can show it, a rule can subscribe to it. The everytool bet applies
unchanged, and `capability(type_measurement)` (ARCH.pl:193) gets its rows for
free.

Three places self-reference bites:

1. **Bootstrapping.** The tables that describe types cannot themselves be
   described by rows in those tables without a fixpoint at load. The way out
   is the one prolog already takes: the compiler tier knows the schema
   statically and PUBLISHES it as rows; the rows are a view of the compiler's
   knowledge, never its source.
2. **Migration ticks.** Changing a type decl rewrites the tables holding
   values of that type while rules may be mid-fixpoint. Q6 below.
3. **Delta noise.** Every intern insert is a rel delta. See ambiguity A3.

---

## Q5 -- NESTED MATCH LOWERING

Covered by check 4 above (SQL at depths 1/2/3, join count = depth - 1, the
inline json1 counterpart, the depth-cost table, the k = 1 break-even).

Two rules matching into the worked example, with lowerings:

```
page_title(Path, Title) <- route(Route, Path, Body, _), body_page(Body, View), view(View, Title, _).
redirect_target(Path, To) <- route(_, Path, Body, _), body_redirect(Body, To).
```

SQL: the depth-3 and depth-2 statements quoted above, with the `WHERE r0."id"
= ?` replaced by the level rule's own bindings.

rx lowering (level rules are maintained views, so the join is a
`combineLatest` over the three rel streams; the delta form is the semi-naive
shape, each rel's delta joined against the others' current sets):

```ts
const pageTitle$ = combineLatest([route$, bodyPage$, view$]).pipe(
  map(([routes, pages, views]) => routes.flatMap(route => {
    const page = pages.find(p => p.id === route.body);
    const view = page && views.find(v => v.id === page.view);
    return view ? [{ path: route.path, title: view.title }] : [];
  })),
  distinctUntilChanged(sameRowSet),
);
```

And the intern bind itself, which is the whole "no new machinery" claim in
five lines:

```
rel view(id, title, tags) set key(2, 3).
view(Id, Title, Tags) <- view_wanted(Title, Tags), Id := content_id('view', Title, Tags).
```

```ts
const view$ = viewWanted$.pipe(
  map(rows => rows.map(row => ({ id: contentId('view', row.title, row.tags), ...row }))),
);
```

No state, no counter, no lock, no seam. That is what content addressing buys.

---

## Q6 -- MIGRATION AND COEXISTENCE

Per-column opt-in, and the compiler already has the slot. `relplan/5` carries
a per-column `int | text` list inferred by `analyze.pl:rel_column_types/5`
(lower.pl:7-10, 326-333); reference storage adds a third value, `ref(Type)`,
in the same position. Inline-flat json1 and reference storage then coexist
inside one program and inside one rel, column by column.

What the compiler needs per column: the storage kind, the referenced type
name (for join planning and for the printer), and, for list columns, the list
mode (cons or indexed).

What the oracle grade looks like during migration: **unchanged, but only if
the tick log prints canonical value text.** Migrating a column from inline to
ref changes the stored bytes and, under a counter policy, changes ids; it does
not change the value. `rendered_text_stable_under_both_policies` is the
receipt. If the log ever prints ids, every migration step is a spurious diff
and the 110-fixture grade becomes unusable exactly when it is needed most.

SLOT-JSON1-FATE, recommended fill: **json1 stays as the representation of
UNTYPED json only** (the `json_arm` ruling's obj/list terms, values with no
declared type). A field with a declared type is always a ref. That draws the
line where the type system already draws it, keeps the json arm intact, and
does not make json1 a cache (a second copy of the truth is a bug, not a fast
path).

---

## Q7 -- RECURSIVE TYPES

`route.children : [route]` is already the recursive case, and it needs
nothing: the ref graph is edges, so descendants are the engine's own fixpoint
(a recursive CTE over `route` and the list tables).

- `not_stratified` interaction: positive recursion over the ref edges is fine.
  A rule that recurses through NEGATION over the type graph is refused, and
  correctly so (the guard IS semantics, per the tabling verdict). Nothing
  about types-as-rels changes that.
- Termination: guaranteed for interned values, because the ref graph is a DAG
  (graded). A recursive CTE over interned refs cannot loop.
- Under extrinsic keys it CAN loop, so any traversal over an extrinsic-key
  graph needs a visited set or a depth cap. Same crack, third appearance.

---

## Q8 -- TYPE-CHECK RESIDENCY

| option | what it costs | what it buys | what it cannot do |
|---|---|---|---|
| (a) compiler-side prolog only (today) | zero. `books/v6/algos/unify_hm.pl` is the whole of HM in 4 clauses because unification IS prolog | correct inference, occurs check by a flag (unify_hm.pl:19) | the type graph is invisible to the LSP, to measures, and to dl rules |
| (b) dl rules over type tables | a real fixpoint program; type VARIABLES need an equivalence relation (union-find as a relation), and "no unifier exists" is a negation over a fixpoint, which pushes on stratification | self-hosting, and every capability that wants types gets rows | inference with fresh variables is awkward; the algorithm stops being 4 clauses |
| (c) hybrid: infer in prolog, PUBLISH the type graph as rows | one emitter | both of the above | nothing identified |

Recommendation **(c)**, for one concrete reason beyond taste: the checking
direction (does this program's declared type graph hold together) is a monotone
fixpoint and fits (b) perfectly, while the inference direction needs
unification, which prolog gives for free and datalog does not. Publishing the
result as rows is the part that pays for `capability(type_measurement)` and
the LSP, and it costs one emitter rather than a rewrite.

Evidence that (b) is not impossible, only expensive: souffle ships `eqrel`, an
equivalence-relation storage backed by union-find, precisely so that
reflexive/symmetric/transitive rules can be omitted (see prior art). So a
relational union-find is proven in a production datalog. It is the surrounding
inference algorithm, not the union-find, that argues for prolog.

---

## PRIOR ART: SOUFFLE

All quotes verified against primary sources this session.

**The value world is one integer.** `RamDomain` is the single element type for
every tuple value, 32-bit by default:
`src/include/souffle/RamTypes.h:39-50` (`#define RAM_DOMAIN_SIZE 32`,
`using RamDomain = int32_t`), and the docs agree: "The word size of a
primitive type is 32 bits" (https://souffle-lang.github.io/types).

**Record identity IS a content-addressed surrogate**, which directly validates
our ruled default. Symbols intern through `SymbolTable`
(`encode`/`decode`/`findOrInsert`, SymbolTable.h:109-126); records intern
through `RecordTable` with the API named exactly `pack`/`unpack`
(RecordTable.h:35-39), described in its own header as "a map between records
and their references". Both sit on one primitive, `ConcurrentFlyweight`, whose
class comment is the uniqueness claim in one sentence
(ConcurrentFlyweight.h:18-20):

> "A concurrent, almost lock-free associative datastructure that implements the
> Flyweight pattern. Assigns a unique index to each inserted key. Elements
> cannot be removed, the datastructure can only grow."

Same tuple content, same index, always. Note what souffle's ids are: dense
monotone integers assigned by insertion, so souffle gets dense AND
content-addressed by paying the order dependence discussed in Q2. It can
afford that because it never diffs one run's output against another's.

**ADT encoding, and the correction to the inline-packing recollection.** The
declaration is
`.type Expression = Number { x : number } | Variable { v : symbol } | Add {...}`,
branch identifiers are globally unique, and the doc states the encoding
(https://souffle-lang.github.io/implementation): "Each ADT branch has a unique
number... The unique number is used in an outer record that refers to the
inner record, i.e. `< branch-id, <a1, ...> >`. Using this encoding of ADTs
permits the use of fixed-length records."

Verified against the lowering itself,
`src/ast2ram/seminaive/ValueTranslator.cpp:128-158`, the split is by FIELD
COUNT, not field size:

| branch shape | encoding | our Q1 analogue |
|---|---|---|
| 0 fields (enum) | bare `SignedConstant(branchId)`, no record, no interning | a variant rel with an empty content key: exactly one possible row, degenerates to a constant |
| 1 field | one `PackRecord`: `[branch_id, arg]` | layout (d): tag + one payload column |
| >= 2 fields | two `PackRecord`s: `[branch_id, [args...]]` | layout (d) with the payload being another interned row |

**There is no bit-packing of small variants into unused RamDomain bits.** That
part of the recollection is refuted; every non-enum branch goes through
`RecordTable::pack`. What survives, and is worth copying, is the zero-field
optimization, which our layout (b) gets for free.

**Per-relation data structure specialization**
(https://souffle-lang.github.io/relations): btree (default; direct for arity
<= 6, indirect above), brie ("a specialised form of a trie... a performance
benefit for highly dense data (arity <= 2)"), eqrel ("a linear representation
of equivalence relations, by using a union-find based algorithm... The rules
for reflexivity, symmetry, and transitivity can be omitted"), and nullary
relations implemented as a boolean. The structure choice is a USER qualifier
with an arity-based default; only INDEX selection inside a structure is
automatic (a combinatorial optimization). Inspiration for per-rel storage
policy words, with the caveat that souffle did not manage to pick the
structure automatically either.

**THE KEY DELTA.** Souffle is monotonic: there is no retraction in the
mainline engine, so the intern tables never shrink and souffle never had to
solve GC or refcounting of interned values. The flyweight comment above says
it outright ("Elements cannot be removed"), and a source sweep of
RecordTable/SymbolTable/RecordTableImpl/SymbolTableImpl/ConcurrentFlyweight
found no remove, erase, refcount, or GC entry point; the only `delete` calls
are whole-table teardown. There IS an incremental line of work ("Towards
Elastic Incrementalization for Datalog", PPDP 2021,
https://souffle-lang.github.io/pdf/ppdp21incremental.pdf), unmerged, living on
an unmerged branch of the `davidwzhao/souffle` fork; it
adds an iteration number and a COUNT per tuple and auxiliary diff relations,
which is count-IVM by another name, but it operates entirely at the
relation/tuple layer and the paper never mentions the record or symbol tables.
Whether interned values are ever collected there is UNVERIFIED.

Two consequences for us, and they pull in opposite directions:
- Interning without GC is proven at very large scale (their benchmark table
  runs to 41.6M IDB tuples), so the interning half of this design carries no
  novelty risk.
- The GC half has NO souffle precedent to copy. Our whole domination question
  is exactly the part souffle never had to answer. That is why check 3 above
  had to be built rather than cited, and why the acyclicity theorem matters:
  it is the argument that our refcount is complete, and nobody upstream needed
  one.

One caution if we copy the dense-flyweight approach: `pack` narrows an internal
`size_t` index down to a 32-bit `RamDomain` on return, and no documented guard
against exceeding 2^31-1 distinct records was found. That is an inference from
the type signatures, not a documented limit, so it is UNVERIFIED, but it is the
kind of thing to check before adopting 32-bit value ids.

---

## PRIOR ART: LATTICES IN THE TYPE SYSTEM

(pending)

---

## SLOTS

| slot | state | fill |
|---|---|---|
| SLOT-DECL-SPELLING | PRICED, user picks | ranked (b) prolog functors > (c) plain rels > (a) braces, criteria table above |
| SLOT-OWNERSHIP-MARK | FILLED by analysis, for the value plane: **no mark**. Domination dissolves into support counting | OPEN for the extrinsic-key plane, where a real cascade rule is needed and FK cascade is the wrong tool |
| SLOT-ENUM-SHAPE | RECOMMENDED | (b) N variant rels sharing the id space, plus a DERIVED tag view when "which variant" is asked; (d) souffle's tag+payload priced as the alternative |
| SLOT-INTERN-SCOPE | RECOMMENDED | per TYPE. One table per type is the intern scope. Global needs a universal value table and loses typed columns; per rel breaks sharing across rels |
| SLOT-JSON1-FATE | RECOMMENDED | keep for UNTYPED json only. Typed field = ref, always. Never a cache |

---

## NUMBERED AMBIGUITIES

1. **Dense ints vs content ids.** `storage_integer_keys` and `salt_minting`
   both apply to a value's id and want different things. Souffle's answer
   (a dense flyweight index) is order-dependent. Ruling wanted: dense ids with
   ids kept out of every log, or wide content ids everywhere.
2. **The tick log must print values, not ids.** Prerequisite for stopping-point
   item 9 and for any migration grade. Graded both ways in this lab.
3. **Dictionary rels appear in boundary deltas.** Every intern insert is a
   `+row` at the boundary, so a consumer of the parent rel sees traffic from
   the value plane. Does the policy bundle need a fourth bit (not
   delta-observable), or is the noise accepted? This is the one place the
   hypothesis' three-bit bundle is short.
4. **Variable-arity intern keys do not fit `key(...)`.** An indexed list's
   identity lives in a second table. Cons cells fix it and are what souffle
   does; indexed lists need an out-of-band identity function. Measured cost of
   the choice: for the two lists `["x","y"]` and `["w","x","y"]`, cons is 4
   rows with 3 shared, indexed is 2 rows plus 5 edges with nothing shared
   (`cons_shares_tails_indexed_does_not`).
5. **Cascade is not declarable.** A second support anywhere (an EDB arrival, a
   second demand) silently prevents it. Should a decl be able to REFUSE a
   second origin for a value rel, or is silence correct?
6. **Q3(a) has no rx lowering.** `ON DELETE CASCADE` deletes rows inside
   sqlite with no operator producing the change; the rx plane learns about it
   only from the next snapshot diff. Per the standing snippet law that is a
   design defect in the option, and it is one more reason to reject (a).
7. **Brace collision.** Spelling (a) reuses `{...}`, which already denotes a
   json object value (SYNTAX.md:73). Two meanings for one production.
8. **The `value` policy word.** Optional sugar; if it lands it needs a
   vocabulary-law-legal name (SQL's `unique` or `distinct`), and the honest
   option is no word at all.
9. **Enum exhaustiveness under layout (b).** The variant list must come from
   the decl, not from the derived tag view, or exhaustiveness checks read a
   rel that only contains variants somebody happened to construct.
10. **32-bit value ids.** If we copy souffle's dense index, the narrowing to a
    32-bit id has no documented guard upstream. Check before adopting.

---

## WHAT THIS LAB DID NOT DO

- No benchmarks. Every cost in this document is counted (joins, probes, rows,
  characters) or quoted from an existing receipt; nothing is measured.
- No emitter changes, no fixture edits, no engine changes. `go.pl` and
  `roundtrip.sh` were re-run to prove it.
- The three spellings' expansions are HAND-WRITTEN, because no parser for (a)
  or (b) exists. The check grades that the three hand-written expansions
  agree, not that a parser produces them.

Lab files, for the death protocol: `v6/prolog/labs/types_as_rels/{schema,
value_model,lowering,types_as_rels}.pl`, last full copy at the commit that
lands this verdict.
