# Auto-factorization lab, verdict

Header: `plans/2026-07-31-auto-factorization-header.md`. Lab ran in a worktree
off base `7405a438`. Lab files lived at `v6/prolog/labs/auto_factorization/`
and die on landing; this document is the durable output and is written to stand
alone.

Everything below is measured. Where the lab could not measure something it says
so instead of estimating.

Python is the OFFLINE REFEREE and nothing else. Every computation the lab
performs is spelled in dl6 and graded through the reference engine, or carries a
quoted refusal, or is a named engine gap. The ledger in section A is the index
of which, and it is the first thing to read.

---

## A. Computation ledger: one row per computation, no exceptions

Legend: **(a)** expressed in dl6 and graded through the real engine, receipt
named. **(b)** NAMED REFUSAL, the exact thrown text quoted. **(c)** ENGINE GAP,
expressible in principle, no construct exists.

| # | computation | class | dl6 rel / refusal | receipt |
|---|---|---|---|---|
| A1 | transitive reachability over the dependency graph | **(a)** | `reach/3` bounded recursion, `factorize.dl6` | 388 rows, feeds A2 |
| A2 | toposort depth (height above dependency leaves) | **(a)** | `onward_positive` max aggregate + `coalesce` zero floor, `file_depth/2` | R1: 62/62 vs networkx, 0 mismatches |
| A3 | cycle detection on the file graph | **(a)** | `cycle_file/1` from `reach(x,x,_)` | 0 rows, matches networkx 0 SCCs > 1 |
| A4 | SCC condensation (depth under cycles) | **(c)** | no construct. Mutual reachability plus a min-index representative is the shape (`components.dl6` proves it), but each condensation costs a SECOND full closure per graph, and section 7 shows one closure already walls under 1,000 files. Not run. | consequence measured at A15 |
| A5 | internal / crossing edge counts, ALL FOUR axes | **(a)** | `axis_internal_total/3`, `axis_crossing_total/3` over `file_group(axis, path, grp)`, `numbering.dl6` | R2: 4 axes x every group x 2 columns, 0 mismatches |
| A6 | modularity Q, all four axes | **(a)** | `modularity_scaled/2`, exact integer `Q * 4 * m * m` via `sum(4*m*e - d*d)`; caller divides once | R2b: file -0.0278, folder 0.4374, package 0.6300, plane 0.4997, all 4 exact vs networkx |
| A7 | undirected edge set and node degrees | **(a)** | `und_edge/2` canonicalised on `node_index`, `node_degree/2` | 123 undirected edges, feeds A6 |
| A8 | label propagation (community detection proper) | **(b)** | ``not_stratified`` from `dl6_oracle.pl` on `lpa.dl6`. Printed verbatim: `ERROR: [Thread main] -g oracle(...): Unknown message: not_stratified` | R3b |
| A9 | Louvain / Leiden | **(c)** | no construct and none is proposed. The inner loop reads assignments left by earlier moves in the SAME pass, so the gain for node 50 depends on whether node 12 already moved. That is a sequential fold, not a rule firing over all rows. Buying the referee is the correct answer, not a construct. | section 3a |
| A10 | connected components (min-label over closed reachability) | **(a)** | `component/2` via `component_index(node, min(idx))` outside the recursion, `components.dl6` | R3c: matches `networkx.connected_components` |
| A11 | `min`/`max` over a TEXT column | **(b)** | compiler: `unsupported_construct: compiler refused rule 'aggregate_operand_not_number'`. Engine: ``lists:min_list/3: Arithmetic: `a/0' is not a function`` | R3d, 4-line repro |
| A12 | counterfactual cut, edge algebra | **(a)** | `dep_after/3` = surviving edges `not(cut_edge(...))` union tail->iface union iface->head, `cuts.dl6` | R4b: 6/6 cuts, `edges_after` exact vs networkx |
| A13 | cut depth, acyclic counterfactuals | **(a)** | `max_depth_after/2` over `reach_after/4` with `cut_name` threaded through every rel | R4b: 5 of 6 exact |
| A14 | cut depth, cyclic counterfactuals | **(c)** | falls to A4. The engine returns the hop floor (64) where the referee condenses and returns 5. | R4b: caught on `v6/prolog=>v6/prolog/compile` |
| A15 | cut cycle detection | **(a)** | `cycle_after/2` from `reach_after(c,x,x,_)`, added AFTER A14 was caught so the wrong number names itself instead of lying | R4b: flags exactly the cut networkx finds cyclic |
| A16 | candidate ranking (question 5's ordering) | **(a)** | no ORDER BY and no LIMIT exist; rank is a COUNT of what beats you. `dense_rank/2` counts distinct scores, `competition_rank/2` counts rivals, differing by ONE column because a bare rel is a set. `best_cut/2` via `min`. | R5b: 21 candidates, both ranks exact, best cut agrees |
| A17 | folder quotient graph and its depth | **(a)** | `folder_dep/2`, `folder_reach/3`, `folder_depth/2`, `numbering.dl6` | R6c: matches networkx quotient heights |
| A18 | folder cycle detection | **(a)** | `folder_cycle/1` | R6c: finds exactly `v6/prolog` and `v6/prolog/compile` |
| A19 | derived file prefix (dense rank of depth within folder) | **(a)** | `lower_depth/2` set-deduped, `prefix_positive(path, count(other_depth))`, `file_prefix/2` | R6: 62/62 vs networkx |
| A20 | hand-prefix violation check (the HAND ERROR classification) | **(a)** | `same_folder_path/2` over `reach`, `hand_violation/4` where `tail_prefix < head_prefix` | R6d: 2 violations, same 2 the referee finds |
| A21 | derived-prefix self-consistency check | **(a)** | `derived_violation/4`, same rule with `file_prefix` substituted | R6d: 0 rows over 123 edges |
| A22 | agree / differ / metric-blind classification | **(a)** | `hand_agrees/2`, `hand_differs/3`, `metric_blind/2` (`file_depth(path,0)`, `hand > 0`, `not(dep(path,_))`) | R6d: 15 / 23 / 1, matches the referee |
| A23 | reading the numeric prefix off a filename | **(c)** | no string split. The atlas program hit the same wall and said so. Today the prefix must arrive as a world row (`hand_prefix/2`), which is the correct residency anyway (a host job), but it means the language cannot classify its own filenames unaided. | `numbering.dl6` header |
| A24 | canonical ordering of two text keys | **(c)** | `expression('<'/2, ordered_comparison, 0, infix('<'), both_number)`: ordered comparison is number-only, so a text pair cannot be put in canonical order to dedup an undirected edge. Worked around with a world-fed `node_index/2`. | `numbering.dl6` header, forced A7's shape |
| A25 | minimal file-move set that deletes a folder cycle | **(c)** | max-flow min-cut. Each augmenting path reads residual capacities left by the previous augmentation in the same run, the same sequential-state objection as A9. Bought from networkx; no construct proposed. | section 4c |
| A26 | Louvain communities, max cluster size, cut Q deltas | referee only | A9's consequence: any number computed FROM a Louvain partition inherits A9. Reported as referee output and labelled as such wherever it appears. | sections 3b, 4a, 4b |
| A27 | `dot -Tplain` rank comparison | referee only | the header's own grading reference, an external tool by definition. | R1b |

Twenty of the twenty-seven rows are dl6-expressed and graded. Two are named
refusals with quoted text. Five are gaps, and four of the five (A4, A9, A14,
A25) are the SAME structural gap wearing four hats: an algorithm whose step
reads state written earlier in the same step.

**The single most useful sentence to take from this ledger:** every metric in
this lab is expressible except the ones that need sequential intra-step state,
and modularity, which everyone assumes needs floats, is exact in integers.

---

## 0. The fact base, and why it is trustworthy

Two planes feed the analysis.

**Plane one, the atlas.** `v6/DATAFLOW-ATLAS-flat-td.dot` is the only checked-in
projection carrying the complete node and edge set the atlas derives. Parsing it
recovers 421 nodes and 809 edges, and every derived count matches the atlas's
own `v6/DATAFLOW-ATLAS.md` tables exactly:

| quantity | atlas doc | lab parse |
|---|---:|---:|
| nodes | 421 | 421 |
| edges | 809 | 809 |
| planes cli/extractor/prolog/shell/sqlite/typescript | 7/23/265/17/13/96 | 7/23/265/17/13/96 |
| `calls` | 404 | 404 |
| `import` | 127 | 127 |
| `import_unnamed` | 15 | 15 |
| `resides` | 183 | 183 |
| `statement` | 13 | 13 |
| `flag` + `record` | 15 + 7 | 22 (one style, see below) |
| all six `bridge_*` kinds | 1+1+10+24+2+7 = 45 | 45 (one style) |

Edge KIND survives the `.dot` only as far as the style string, and three style
strings are shared. `flag` and `record` collapse into one class, and the six
`bridge_*` kinds collapse into one. The recovered kind is therefore a class, not
the kind, and nothing in this lab depends on separating members of a collapsed
class. The atlas files were read and never written.

**Plane two, file-level TypeScript imports, regex-shaped and said so.** The
atlas does not cover `v6/dl/src` at all, and question 6 names it. A regex over
`from "..."` specifiers supplies file-level import edges for the packages the
atlas is silent on. It is admissible because it is cross-graded: over the files
BOTH planes see, the regex plane's file-level import edges are compared against
the atlas's extractor-derived ones.

```
crossgrade_overlap_atlas   38
crossgrade_overlap_regex   38
crossgrade_only_regex       0
crossgrade_only_atlas       0
```

Zero invented edges, zero missed edges, over 38 real ones. That is the receipt
that the regex plane can be trusted where the atlas is silent.

**The analysis graph.** Files in the three rename-target packages, edges
induced. Prolog file edges come from the atlas's xref-derived `calls` projected
to files; TypeScript file edges come from the regex plane. `gen_emitted/` and
`gen_served/` are compiler output carrying no hand prefix and are excluded from
every axis; `v6/dl/src/0_generated/` is langium output, kept in the graph as a
real dependency and marked `generated` in the rename table.

```
files 62   edges 123   cycles 0
per package: dl 16, prolog 27, tsv2 19
```

Zero cycles at file granularity across all three packages. That is worth stating
plainly because it is what makes every depth number below defined at all.

---

## 1. Toposort depth as a rel: HOLDS, and dot ranks are not the answer key

**Definition used.** `depth(file)` is height above the dependency leaves: 0 when
the file depends on nothing inside the analysed set, otherwise
`1 + max(depth of its dependencies)`. Computed over the SCC condensation, so a
cycle would collapse to one node and its members would share one depth. This is
the direction the repo's own prefixes already use: `0_types.ts` is imported by
everything and sits at the bottom.

**Result.** Max depth 6, 62 condensation nodes for 62 files.

| depth | files |
|---:|---:|
| 0 | 14 |
| 1 | 20 |
| 2 | 12 |
| 3 | 3 |
| 4 | 6 |
| 5 | 5 |
| 6 | 2 |

**In dl6.** `factorize.dl6` derives it with a bounded recursive `reach` rel, a
`max` aggregate outside the recursion, and a `coalesce` zero floor for sinks:

```
rel reach(tail: text, head: text, hops: int).
reach(tail, head, 1) <- dep(tail, head).
reach(tail, head, hops) <-
  reach(tail, mid, prior), dep(mid, head), prior < 64, hops := prior + 1.

rel onward_positive(path: text, depth: int).
onward_positive(path, max(hops)) <- reach(path, _, hops).

rel file_depth(path: text, depth: int).
file_depth(path, depth) <- file_folder(path, _), coalesce(onward_positive(path, depth), 0).
```

Run through `dl6_oracle.pl` over a 185-arrival schedule: 388 `reach` rows, 62
`file_depth` rows, **0 mismatches against networkx**. The shape is the one the
atlas already proved; nothing new was needed.

**Grading against `dot -Tplain`, which is the part the header got half right.**
Graphviz assigns ranks by network simplex, which MINIMISES total edge length
subject to `rank(tail) > rank(head)`. Longest-path height MAXIMISES distance to
a leaf. They are different objectives and they do not agree:

```
nodes_ranked        62
max_depth            6      max_dot_rank        6
equal_rank_nodes    47      mismatch_count     15
depth_uphill_edges   0      dot_uphill_edges    0
mismatch_all_pulled_down  true
```

47 of 62 agree; every one of the 15 differences has dot's rank HIGHER, meaning
dot pulled a leaf down toward its single consumer to shorten the edge.
`v6/tsv2/cli/0_inventory.ts` is the clean example: height 0 (it imports nothing),
dot rank 4, because its only consumer `bop.ts` sits at rank 5.

**So the receipt that is actually valid is the shared property, not equality:**
both orderings put every one of the 123 dependency edges strictly downhill, and
both reach the same maximum. `dot -Tplain` ranks are a layout, not a toposort
depth, and should not be used as an answer key for one. That correction is a
finding, not a failure of the leg.

---

## 2. Cohesion and coupling per axis: the numbers for the CURRENT partition

Internal = both endpoints in the group. Crossing = exactly one endpoint, counted
by both endpoint groups so per-group ratios are comparable. `Q` is networkx
modularity over the undirected projection.

| axis | groups | internal | crossing | Q |
|---|---:|---:|---:|---:|
| file | 62 | 0 | 123 | -0.0278 |
| folder | 7 | 79 | 44 | 0.4374 |
| package | 3 | 123 | 0 | 0.6300 |
| language plane | 2 | 123 | 0 | 0.4997 |

Per folder:

| folder | internal | crossing | ratio |
|---|---:|---:|---:|
| `v6/dl/src` | 25 | 2 | 0.926 |
| `v6/dl/src/0_generated` | 2 | 2 | 0.500 |
| `v6/prolog` | 9 | 28 | 0.243 |
| `v6/prolog/compile` | 23 | 28 | 0.451 |
| `v6/tsv2/cli` | 1 | 2 | 0.333 |
| `v6/tsv2/runtime` | 10 | 13 | 0.435 |
| `v6/tsv2/serve` | 9 | 13 | 0.409 |

Two readings the numbers force:

- The package axis has **zero** crossing edges. At file granularity in this fact
  base the three packages are disconnected components. `dl` and `tsv2` do not
  import each other, and the prolog and TypeScript planes only meet through the
  atlas's hand-written bridge rules, which are symbol-level and cross no file
  import. Every interesting cut therefore lives INSIDE a package, and the
  cross-language "fake hop" question can only be asked at symbol granularity.
- `v6/prolog` is the weakest folder by a wide margin: 9 internal against 28
  crossing, ratio 0.243. It is the folder the rest of section 4 keeps returning
  to.

**In dl6, on every axis, with modularity exact.** `numbering.dl6` carries the
axis as a COLUMN (`file_group(axis, path, grp)`), so one rule pair covers all
four axes; the two-clause `axis_crossing` rule is what makes one crossing edge
counted by both endpoint groups. Graded: **0 mismatches** across 4 axes, every
group, both columns.

Modularity itself is exact in integers, which is worth spelling out because the
language has no float and the obvious conclusion is that Q is out of reach:

```
Q = sum over groups of ( e/m - (d/2m)^2 )    so    Q * 4*m*m = sum of (4*m*e - d*d)
```

Every term on the right is an integer. `modularity_scaled(axis, sum(term))`
derives it and the caller divides once, outside the language, where a float is
free. Graded against networkx: file -0.0278, folder 0.4374, package 0.6300,
plane 0.4997. **All four exact.**

**A DEFECT IN THE DRAFT THAT THE REFEREE CAUGHT, which is the reason to have
one.** The first version read `und_internal_total` directly, and the file axis
came back 0.0 against the referee's -0.0278. Cause: every group with zero
internal edges produces NO ROW from the `count` aggregate, so its term left the
sum entirely, and those terms are exactly the negative ones. On the file axis
that is every group. The fix is one rel:

```
rel und_internal_filled(axis: text, grp: text, edges: int).
und_internal_filled(axis, grp, edges) <-
  axis_group(axis, grp), coalesce(und_internal_total(axis, grp, edges), 0).
```

This is the language design review's finding A11 ("count never 0") biting a real
program. An aggregate emits no row for an empty group, so any formula where an
empty group still owes a term must put the zero back by hand. Silent, and
wrong-in-the-safe-looking-direction: the number stayed plausible.

**SLOT-TYPE-AXIS: resolved as not-applicable on this fact base, with the reason.**
The header asks for a TS-type axis from sig facts. The atlas emits no `sig`
records into its drawn node set (its extractor projections are defs, call sites
and import specifiers), and prolog has no types at all. The lab did not
manufacture a type axis it had no facts for. What the axis WOULD be, if the
extractor's `sig` family were projected: group each symbol by the declared type
of its interface binding, which under the repo's own interface-bound law
(`export const SqlRunner: ISqlRunner`) is a real and populated grouping for
TypeScript and empty for prolog. That asymmetry is the honest answer to the slot:
the type axis is per-plane and does not exist on three of the six planes.

---

## 3. Community detection: buy research first, then what can and cannot be lowered

### 3a. Candidate research (run before any clustering code)

Retrieved live 2026-07-31 from the npm registry API, GitHub API, PyPI and
project docs.

| candidate | role | license | determinism | verdict |
|---|---|---|---|---|
| **networkx** `louvain_communities` + `modularity()` | referee (primary) | BSD-3-Clause | `seed` controls node-shuffle order; order-dependence documented | 17.1k stars, pushed 2026-07-31, directed-aware formulas for both Louvain and the scorer, and `modularity()` is separable from any one candidate's output. Chosen. |
| **leidenalg** (vtraag) | referee (secondary) | GPL-3.0 | not confirmed at API level | 787 stars, 182k downloads/week, actively pushed 2026-07-28. Fixes Louvain's disconnected-community defect by construction. GPL is a non-issue for an offline grading tool. Not installed here; named as the cross-check to reach for if networkx's Louvain is ever doubted. |
| **igraph** (C core + python bindings) | referee (tertiary) | GPL-2.0-or-later | not confirmed | Independent codebase, useful only as a tiebreaker. `community_leiden` in the C core is undirected-only per leidenalg's own docs; `community_infomap` is documented O(n^3)+ and unusable past a few thousand nodes. Naming trap: the PyPI name `python-igraph` is a deprecated shim as of 1.0.0 (2025-10-23), the live distribution is `igraph`. |
| **label propagation as a SQL/datalog fixpoint** | in-language | n/a | must be designed in | The ONLY structurally compatible candidate. O(V+E) per round, bulk read then bulk write, which is the semi-naive shape. TigerGraph's GSQL `ACCUM`/`WHILE` implementation is the closest real prior art. Documented hazards: synchronous updates provably oscillate on near-bipartite graphs, and ties need an explicit deterministic break. |
| **graphology-communities-louvain** | neither | MIT | opt-in seeded `rng` | 2.0.2, published 2024-12-17, MIT, directed and weighted supported. Rejected as referee because it would grade its own computation, and not lowerable (sequential per-node moves plus a graph-aggregation restart). |
| **ngraph.leiden** | neither | MIT | `randomSeed` claimed | Claims directed modularity, CPM, weights, resolution and a determinism seed, at 3 GitHub stars. The gap between the claims and any observed usage is the reason not to reach for it. Not lowerable regardless. |
| **jLouvain**, **ngraph.louvain** | neither | MIT | undocumented | jLouvain last pushed 2021-07-29, over four years stale. ngraph.louvain 24 stars, superseded by the same author's Leiden port. |
| **onager** (DuckDB extension), **DuckPGQ** | neither | MIT/Apache-2.0; n/a | n/a | onager self-declares "early development, bugs and breaking changes expected". DuckPGQ ships PageRank, weakly-connected components and shortest paths, no community detection. Both DuckDB-specific and so wrong host for a SQLite/`@libsql` engine. |
| **Neo4j GDS** | referee (optional) | commercial / community GPL | `seed` is a warm-start community id, not confirmed as an RNG seed | A third independent engine, at the operational cost of standing up Neo4j. Only worth it if networkx and leidenalg ever disagree. |

**The structural question, answered.** Louvain and Leiden are NOT set-oriented
fixpoints and cannot be lowered without abandoning their own definitions. Their
inner loop computes each node's modularity gain against the assignments left by
every prior move in the SAME pass, so the gain for node 50 depends on whether
node 12 already moved. That is a sequential fold, not a rule firing over all
rows. Leiden is worse on this axis: its refinement phase is a second sequential
local-move pass, added specifically to repair a defect Louvain's sequential
design produces. Batching the moves against the previous settled snapshot would
make it expressible, but that is a different algorithm with no equivalence proof
found in the searched literature.

**Verdict: buy the referee (networkx), lower label propagation, never lower
Louvain.**

### 3b. Referee numbers on the real graph

| method | communities | Q | sizes |
|---|---:|---:|---|
| Louvain (networkx, seed 1) | 5 | 0.6364 | 23, 19, 16, 3, 1 |
| greedy modularity (CNM) | 5 | 0.6364 | 23, 19, 16, 3, 1 |
| async label propagation | 6 | 0.6331 | 24, 19, 12, 4, 2, 1 |
| **the existing package partition** | **3** | **0.6300** | **27, 19, 16** |
| the existing folder partition | 7 | 0.4374 | |

**The headline of this question.** The hand-made package boundary scores 0.6300
against Louvain's 0.6364. The gap is 0.0064, one percent. The three packages are
already very close to the modularity optimum, and the only thing Louvain does
differently is split the 27-file prolog package into 23 + 3 + 1 while leaving
`tsv2` (19) and `dl` (16) exactly intact. Every structural finding in this lab
points at the same place, and this is the first of them: **the prolog package is
the one that is not one thing.**

### 3c. What the language can and cannot express: one NAMED REFUSAL

Label propagation written the way the algorithm defines it, as a per-round
majority (here minimum) vote over the previous round's labels:

```
rel label(round: int, node: text, tag: text).
label(0, node, node) <- neighbor(node, _).
rel picked_tag(round: int, node: text, tag: text).
picked_tag(round, node, min(tag)) <- candidate_tag(round, node, tag).
label(next_round, node, tag) <-
  picked_tag(round, node, tag), round < 8, next_round := round + 1.
```

**REFUSAL, reference engine, quoted verbatim:**

```
ERROR: [Thread main] -g oracle('.../lpa.dl6','.../lpa.schedule.json'):
       Unknown message: not_stratified
```

The `label` rel is headed by a
clause reading `picked_tag`, which reads `label` through a `min` aggregate. That
is recursion through an aggregate, and the `not_stratified` guard IS the
semantics (per the tabling verdict). The refusal is correct, and it means label
propagation as such has no dl6 spelling today.

Two side-findings from that run, both worth carrying:

- The refusal printed as `Unknown message: not_stratified` with no file, no
  line and no rule. This is the language design review's finding B4 reproduced
  in the wild: `prolog:message//1` clauses do not exist for these terms.
- Reaching the refusal at all needed the whole program; there is no cheaper way
  to ask "is this shape legal".

**What IS expressible: connected components by min-label over an already-closed
reachability rel.** The aggregate sits OUTSIDE the recursion, so it stratifies:

```
rel linked(node: text, other: text).
linked(node, other) <- link(node, other).
linked(node, other) <- linked(node, mid), link(mid, other).
linked(node, node) <- file_folder(node, _).

rel component_index(node: text, idx: int).
component_index(node, min(other_idx)) <- linked_index(node, other_idx).
```

Graded against `networkx.connected_components`: **matches, 4 components,
sizes 26 / 19 / 16 / 1.** The three packages, plus `oracle_dump.pl`, which has
in-degree 0 and out-degree 0 and is therefore its own component (a file nothing
loads and which loads nothing inside the analysed set, worth a look on its own).

This is the degenerate end of community detection: it recovers the disconnected
packages and nothing finer, where Louvain splits the 26-file prolog component
into 23 + 3. Stating that plainly is the point. **dl6 today reaches connected
components and stops. Everything between components and Louvain needs an
algorithm whose step reads state written earlier in the same step, which is the
one shape the fixpoint model does not have.**

### 3d. A REAL ENGINE DEFECT found on the way

`min`/`max` over a TEXT column. Four-line repro, two rows:

```
rel item(group: text, tag: text).
rel lowest(group: text, tag: text).
lowest(group, min(tag)) <- item(group, tag).
```

| door | behaviour |
|---|---|
| compiler (`compile_dl6/2`) | named refusal: `unsupported_construct: aggregate_operand_not_number` |
| reference engine (`dl6_oracle.pl`) | raw crash: ``lists:min_list/3: Arithmetic: `a/0' is not a function`` |

Both doors reject, so this is not a semantic divergence. It is the SEVENTH
member of the mirrored-cross-plane-check class the org-refactor arc closed six
of, and it runs in the other direction: the check lives compiler-side only and
the engine has no mirror, so it dies in a library predicate instead of refusing
by name. The workaround the lab used is to carry an int index alongside the
label and join the text back afterwards.

**Recommended ARCH row: `aggregate_operand_not_number_engine_mirror`.** Repro
above; the fix is a shared-side check in the `0_program_check.pl` pattern.

---

## 4. The counterfactual cut ("fake hops"): zero engine changes needed

A cut is `cut(name, edge_set)`. Applying it deletes every edge in the set, adds
one node `IFACE:<name>`, and adds `tail -> IFACE` for each distinct tail and
`IFACE -> head` for each distinct head. N*M edges become N+M.

**SLOT-METRIC: resolved as edge-count delta, and here is why the obvious answer
is wrong.** Modularity cannot be the ranking key for a cut, because a cut
CHANGES THE NODE SET. Q before and Q after are computed over different graphs
and are not the same quantity, and the new node belongs to no existing group, so
its assignment is a free parameter that moves the number. The lab reports Q both
ways (interface isolated, interface folded into the tail group) and ranks on the
edge delta, which is exactly "how many dependency arrows does this interface
erase" and is invariant to all of that. Depth delta and max-cluster delta are
reported beside it because a cut that erases edges while adding a hop is a real
trade the number alone hides.

**SLOT-CUT-GRANULARITY: resolved as BOTH, because they answer different
questions.** Edge-set cuts answer "what interface should exist". Node-relocation
cuts (section 4c) answer "what is in the wrong folder". The second one turned
out to be the one with the actionable answer on this codebase.

### 4a. Exhaustive over every folder boundary, file granularity

Every ordered folder pair carrying at least one edge. Six candidates, no
heuristic, no search.

| cut | spliced | tails | heads | edge delta | depth | max cluster | Q(folder) |
|---|---:|---:|---:|---:|---|---|---|
| `v6/prolog/compile => v6/prolog` | 21 | 7 | 8 | **-6** | 6 -> 7 | 24 -> 24 | 0.4374 -> 0.4977 |
| `v6/tsv2/serve => v6/tsv2/runtime` | 12 | 6 | 6 | 0 | 6 -> 7 | 24 -> 24 | 0.4374 -> 0.4465 |
| `v6/prolog => v6/prolog/compile` | 7 | 5 | 3 | +1 | **6 -> 5** | 24 -> 25 | 0.4374 -> 0.4468 |
| `v6/dl/src => v6/dl/src/0_generated` | 2 | 1 | 2 | +1 | 6 -> 6 | 24 -> 23 | 0.4374 -> 0.4370 |
| `v6/tsv2/cli => v6/tsv2/runtime` | 1 | 1 | 1 | +1 | 6 -> 6 | 24 -> 23 | 0.4374 -> 0.4355 |
| `v6/tsv2/cli => v6/tsv2/serve` | 1 | 1 | 1 | +1 | 6 -> 6 | 24 -> 23 | 0.4374 -> 0.4355 |

Only one cut of six pays for itself in edges, and it is the same boundary
everything else points at. The `v6/prolog => v6/prolog/compile` row is the
interesting one for a different reason: it is the only cut that REDUCES depth
(6 -> 5), because the seven edges it splices are the ones running the wrong way.

### 4b. Symbol granularity, where the cross-language hops live

The atlas symbol graph minus `resides` (containment, not dataflow): 421 nodes,
626 edges, 200-plus file-to-file boundaries. Top by edge reduction, minimum 3
spliced edges:

| cut | spliced | tails | heads | edge delta | max cluster |
|---|---:|---:|---:|---:|---|
| `lower.pl => analyze.pl` | 51 | 25 | 14 | **-12** | 97 -> 79 |
| `compile.pl => analyze.pl` | 22 | 2 | 11 | -9 | 97 -> 79 |
| `0_program_check.pl => 0_type_plane.pl` | 22 | 8 | 7 | -7 | 97 -> 97 |
| `3_clock_check.pl => analyze.pl` | 16 | 6 | 6 | -4 | 97 -> 79 |
| `strat.pl => analyze.pl` | 10 | 4 | 4 | -2 | 97 -> 79 |
| `0_refusal_messages.pl => analyze.pl` | 8 | 2 | 4 | -2 | 97 -> 79 |

`analyze.pl` is the head of five of the top six. A facade in front of it is the
single highest-value interface the graph proposes, and `lower.pl => analyze.pl`
alone erases 12 edges and drops the largest cluster from 97 symbols to 79.

**The bridge family cut, which is the user's "fake hop" asked literally.**
Funnelling all 45 cross-language bridge edges through one interface node:

```
spliced 45   tails 28   heads 30   edges 626 -> 639   edge_delta +13
depth 4 -> 5
louvain communities 26 -> 18   max cluster 97 -> 79   Q 0.7382 -> 0.7397
```

**It loses.** 28 tails and 30 heads exceed 45 spliced edges, so a single
cross-language gateway ADDS 13 edges and one hop of depth. It buys a
max-cluster drop and 0.0015 of modularity. The honest verdict is that the
cross-language bridges are already near-minimal fan: they are 45 nearly-distinct
pairs, not a bundle, and there is no gateway to extract. That is a defensible
result for the atlas's design, arrived at adversarially.

The reference-partition Q at symbol granularity is near zero (-0.0183) because
grouping symbols by file after deleting `resides` leaves each file with almost
no internal edges. That number is reported and should not be read as a quality
signal; the Louvain Q (0.7382) is the one with content at this granularity.

### 4c. Node relocation, and the exact minimal answer

The other granularity: move ONE file to another folder inside its own package
and re-measure. Exhaustive over every (file, folder) pair.

Base: `folder_cycles 1, internal 79, crossing 44, Q 0.4374`.

| file | from | to | cycles | crossing | Q | dQ |
|---|---|---|---|---|---|---:|
| `1_host_expand.pl` | `v6/prolog` | `v6/prolog/compile` | 1 -> 1 | 44 -> 39 | 0.4374 -> 0.4700 | +0.0326 |
| `3_clock_check.pl` | `v6/prolog/compile` | `v6/prolog` | 1 -> 1 | 44 -> 41 | 0.4374 -> 0.4674 | +0.0300 |
| `0_refusal_messages.pl` | `v6/prolog` | `v6/prolog/compile` | 1 -> 1 | 44 -> 41 | 0.4374 -> 0.4587 | +0.0213 |
| `0_coalesce_expand.pl` | `v6/prolog` | `v6/prolog/compile` | 1 -> 1 | 44 -> 42 | 0.4374 -> 0.4517 | +0.0143 |
| `0_seq_expand.pl` | `v6/prolog` | `v6/prolog/compile` | 1 -> 1 | 44 -> 42 | 0.4374 -> 0.4517 | +0.0143 |

**Not one single move breaks the folder cycle.** Every row reads 1 -> 1. The
`v6/prolog <-> v6/prolog/compile` cycle (7 edges down, 21 edges up) needs a SET,
and finding the smallest set is not a search problem: with two folders it is the
minimum-cost closure problem, which networkx's max-flow solves exactly.

```
keep v6/prolog/compile above v6/prolog:  2 moves, cycles -> 0
    v6/prolog/0_refusal_messages.pl        (into compile/)
    v6/prolog/compile/registry.pl          (out of compile/)
  crossing 44 -> 46, Q 0.4374 -> 0.4271

keep v6/prolog above v6/prolog/compile:  9 moves, cycles -> 0
    3_clock_check.pl, 6_profile.pl, analyze.pl, compile.pl, emit_ts.pl,
    lower.pl, print_dl.pl, strat.pl, sweep.pl    (all out of compile/)
  crossing 44 -> 27, Q 0.4374 -> 0.5210
```

Both are exact minima for their orientation, not heuristics. The two-move option
is the cheapest edit and slightly worsens the ratio; the nine-move option is
flattening `compile/` into `v6/prolog` and it is the structurally better answer
by every number (crossing 44 -> 27, Q 0.4374 -> 0.5210, which lands above
Louvain's own folder-level score).

`registry.pl` is worth naming on its own: 13 files depend on it, out-degree 0,
the single most depended-upon file in the prolog package, and it is the one file
appearing in the minimal cut for both orientations. It lives in `compile/` and
six files outside `compile/` reach into that folder to get it.

### 4d. In dl6

The cut is rel algebra with a union, exactly as the header predicted, with zero
engine changes:

```
rel dep_after(cut_name: text, tail: text, head: text).
dep_after(cut_name, tail, head) <-
  cut_iface(cut_name, _), dep(tail, head), not(cut_edge(cut_name, tail, head)).
dep_after(cut_name, tail, iface) <-
  cut_edge(cut_name, tail, _), cut_iface(cut_name, iface).
dep_after(cut_name, iface, head) <-
  cut_edge(cut_name, _, head), cut_iface(cut_name, iface).
```

The candidate set arrives as world rows (`cut_edge`, `cut_iface`), so the search
is exhaustive by construction rather than guided. Depth after each cut rides the
same bounded recursion with `cut_name` threaded through every rel, because the
counterfactual graphs are one rel and not one graph per run.

Graded: `edges_after` exact on **6 of 6** cuts, `max_depth_after` exact on **5
of 6**.

**The sixth is a real finding and it was found by grading, not by inspection.**
`v6/prolog => v6/prolog/compile` returned 64 where the referee returned 5. 64 is
the hop floor. Splicing an interface into an ACYCLIC graph can still create a
cycle: three of that cut's heads sit in `compile/`, two of them depend back on
`v6/prolog` files, and some of those files are tails of the same cut, so
`tail -> IFACE -> head -> ... -> tail` closes. The referee condenses the SCC and
answers 5; the bounded recursion cannot condense and hits its floor.

The repair was not to raise the floor. `cuts.dl6` gained one rel so the wrong
number names itself:

```
rel cycle_after(cut_name: text, node: text).
cycle_after(cut_name, node) <- reach_after(cut_name, node, node, _).
```

The grade now asserts that the engine FLAGS exactly the cuts the referee finds
cyclic, and compares depth only where neither side has a cycle. This is ledger
row A15, and it is the pattern the atlas header already argues for: a resource
or expressiveness cliff should arrive as a named row, never as a plausible
number.

**What stays a gap (A4/A14):** condensed depth under a cut. The shape exists
(mutual reachability plus a min-index representative, proven by
`components.dl6`), but it costs a SECOND full closure per counterfactual graph,
and section 7 shows one closure already walls under 1,000 files. Not run, and
the reason is the same scale finding rather than an expressiveness one.

**Ranking (A16).** There is no `ORDER BY` and no `LIMIT`, so a rank is a count
of what beats you, the same rank-by-counting shape the file prefix uses. Both
definitions are derived, and they differ by exactly ONE column because a bare
rel is a set:

```
beaten_by_name(cut, other_name)   -> competition_rank   counts rivals
beaten_by_score(cut, other_delta) -> dense_rank         counts distinct scores
```

Graded over 21 candidates (6 file-level plus the 15 symbol-level): both ranks
exact against python's sort, and `best_cut/2` via `min` names
`lower.pl => analyze.pl` on both sides. Top-K needs no construct; what is
genuinely absent is ordering as an OUTPUT, which no metric here needs.

---

## 5. Auto-search: exhaustive, and the ranking is degenerate on this codebase

The search space here is small enough that "smallest correct first" means no
search at all: 6 folder boundaries at file granularity, 200-plus at symbol
granularity, 45 bridge edges, and 62 files x folder-in-package relocations. All
enumerated, none sampled.

The result that matters is that the ranking is nearly degenerate. Of six folder
boundary cuts, five cost edges and one saves them. Of every single-file
relocation, none removes the one structural defect the graph has. The auto-search
did not find a rich landscape of trade-offs; it found one defect, named it four
different ways, and computed the exact minimal repair.

That is a finding about this codebase, not about the method. It also means the
method's next real test is a corpus with more than 62 files (section 7).

---

## 6. Auto-numbering: the rename table, and every disagreement classified

### 6a. How the number is derived

- **Depth** as in section 1, over the whole package graph so cross-folder
  dependencies count.
- **Prefix** = the dense rank of a file's depth among the distinct depths its
  OWN FOLDER holds. Files at the same depth share a prefix, which is what the
  existing convention already does (`v6/dl/src` has five `0_` files today).
- **Tiebreak inside a shared prefix**, stated as the header requires: most
  depended-upon first (in-degree descending), then path ascending. It orders the
  listing inside one layer; it does not split the layer.
- **Folder ordering** = the same height over the folder quotient graph. Where
  the quotient has a cycle the folders share a rank and the tie falls to name,
  which is exactly what happens to `v6/prolog` and `v6/prolog/compile` and is
  reported rather than hidden.

### 6b. Classification rule

Decided by the dependency closure, not by taste.

- **HAND ERROR**: a dependency path A -> B exists with `hand(A) < hand(B)`. The
  hand numbering contradicts itself; no metric is needed to see it. The DEPENDER
  carries the error, since raising the head would break its own consistent
  relations.
- **SCALE SHIFT**: numbers differ, every dependency-implied order still holds
  under the hand numbering. The hand ladder is looser or offset. Not an error on
  either side.
- **METRIC BLIND**: derived depth 0 with zero in-package dependencies while the
  hand number is high. Candidate for a dependency the fact base cannot see;
  listed for adjudication with its out-of-package import count as evidence,
  never auto-called.

### 6c. Counts

| bucket | files |
|---|---:|
| agree | 15 |
| scale shift | 16 |
| **hand error** | **6** |
| metric blind | 1 |
| generated (excluded from renaming) | 3 |
| unnumbered today | 21 |
| **total** | **62** |

**Derived-side violations: 0.** Every one of the 123 dependency edges runs
downhill under the derived numbering. That is the receipt that the proposal is
at least self-consistent, and it is checked, not assumed.

### 6d. The six hand errors, all in the prolog package

Same-folder contradictions (2 paths, 1 file):

```
v6/prolog/0_refusal_messages.pl (hand 0) -> v6/prolog/1_expansion.pl   (hand 1)
v6/prolog/0_refusal_messages.pl (hand 0) -> v6/prolog/1_host_expand.pl (hand 1)
```

Cross-folder contradictions, against the derived folder ranking (8 paths, 6
files):

```
0_body_walk.pl           -> compile/registry.pl
0_program_check.pl       -> compile/registry.pl
0_relation_edge_expand.pl-> compile/registry.pl
0_relation_pattern.pl    -> compile/registry.pl
1_host_expand.pl         -> compile/registry.pl
0_refusal_messages.pl    -> compile/registry.pl
0_refusal_messages.pl    -> compile/analyze.pl
0_refusal_messages.pl    -> compile/3_clock_check.pl
```

Both halves are DERIVED IN DL6, not just in the referee. `hand_violation/4`
reads `same_folder_path(tail, head), hand_prefix(tail, tp), hand_prefix(head,
hp), tp < hp` and returns exactly the two paths above. `derived_violation/4` is
the same rule with `file_prefix` substituted and returns **zero rows over 123
edges**, which is the proposal's self-consistency receipt.

The cross-folder half is reported separately and deliberately NOT counted as a
numbering contradiction: while the folder quotient is cyclic there is no folder
order for a number to contradict, so calling these errors would be the metric
overclaiming. `cycle_reach_in/4` derives them as what they are, reach-ins
between two folders that `folder_cycle/1` has already named. The cycle is the
finding; these eight paths are its symptom.

Verified in source, not inferred: `v6/prolog/0_refusal_messages.pl:19` reads
`:- use_module('compile/3_clock_check', [clock_refusal_reason/1]).` A file
numbered 0 in the parent folder importing a file numbered 3 in a child folder.
Its true derived depth is 5 of a maximum 6; it is one of the topmost files in
the prolog package and it is numbered as if it were the base layer.

Six of six hand errors are in one package, and all of them are the
`v6/prolog <-> v6/prolog/compile` cycle wearing a different hat. This is the
fourth independent route to the same defect.

### 6e. The one metric error candidate

`v6/dl/src/5_diag.ts`: hand 5, derived 0, in-degree 1, out-degree 0. It imports
only `sprefa-store-engine/src/lower/ast.ts`, which is outside the analysed set,
so the fact base sees a leaf. The hand number 5 is not a dependency claim at
all; it is a reading-order claim (diagnostics read after the runtime). Nothing
contradicts it, so it is not a hand error either.

**This is the honest limit of the whole exercise, and it deserves to be said
plainly: the existing prefixes encode reading order, and dependency depth is
only one input to reading order.** 16 of 38 numbered files land in "scale shift"
for the same reason. Where the two disagree without a contradiction, the metric
does not get to overrule the author.

### 6f. The rename table

Full table with every column: `out/rename_table.md` and `out/rename_table.tsv`
in the lab (machine-readable JSON in `out/classification.json`). Since the lab
dies on landing, the table is reproduced below in full. Columns: depth is the
absolute height, `hand` is today's prefix, `derived` is the proposal, `in`/`out`
are in-package degrees.

NOTHING BELOW IS APPLIED. Renaming touches every importer and is its own arc.

| package | current path | depth | hand | derived | in | out | proposed path | verdict |
|---|---|---:|---:|---:|---:|---:|---|---|
| dl | `v6/dl/src/0_types.ts` | 0 | 0 | 0 | 9 | 0 | `(unchanged)` | agree |
| dl | `v6/dl/src/0_digest.ts` | 0 | 0 | 0 | 2 | 0 | `(unchanged)` | agree |
| dl | `v6/dl/src/5_diag.ts` | 0 | 5 | 0 | 1 | 0 | `v6/dl/src/0_diag.ts` | metric_blind |
| dl | `v6/dl/src/0_trace.ts` | 1 | 0 | 1 | 4 | 1 | `v6/dl/src/1_trace.ts` | scale_shift |
| dl | `v6/dl/src/0_row.ts` | 1 | 0 | 1 | 2 | 1 | `v6/dl/src/1_row.ts` | scale_shift |
| dl | `v6/dl/src/2_schema.ts` | 1 | 2 | 1 | 1 | 2 | `v6/dl/src/1_schema.ts` | scale_shift |
| dl | `v6/dl/src/0_ast_bridge.ts` | 2 | 0 | 2 | 1 | 3 | `v6/dl/src/2_ast_bridge.ts` | scale_shift |
| dl | `v6/dl/src/1_binds.ts` | 2 | 1 | 2 | 1 | 2 | `v6/dl/src/2_binds.ts` | scale_shift |
| dl | `v6/dl/src/1_hosts.ts` | 2 | 1 | 2 | 1 | 4 | `v6/dl/src/2_hosts.ts` | scale_shift |
| dl | `v6/dl/src/3_runtime.ts` | 2 | 3 | 2 | 1 | 4 | `v6/dl/src/2_runtime.ts` | scale_shift |
| dl | `v6/dl/src/4_ingest.ts` | 2 | 4 | 2 | 1 | 2 | `v6/dl/src/2_ingest.ts` | scale_shift |
| dl | `v6/dl/src/6_http.ts` | 3 | 6 | 3 | 1 | 7 | `v6/dl/src/3_http.ts` | scale_shift |
| dl | `v6/dl/src/main.ts` | 4 | - | 4 | 0 | 1 | `v6/dl/src/4_main.ts` | unnumbered |
| dl | `v6/dl/src/0_generated/ast.ts` | 0 | - | 0 | 2 | 0 | `v6/dl/src/0_generated/0_ast.ts` | generated |
| dl | `v6/dl/src/0_generated/grammar.ts` | 0 | - | 0 | 1 | 0 | `v6/dl/src/0_generated/0_grammar.ts` | generated |
| dl | `v6/dl/src/0_generated/module.ts` | 1 | - | 1 | 1 | 2 | `v6/dl/src/0_generated/1_module.ts` | generated |
| prolog | `v6/prolog/0_type_plane.pl` | 0 | 0 | 0 | 7 | 0 | `(unchanged)` | agree |
| prolog | `v6/prolog/0_coalesce_expand.pl` | 0 | 0 | 0 | 2 | 0 | `(unchanged)` | agree |
| prolog | `v6/prolog/0_enum_expand.pl` | 0 | 0 | 0 | 2 | 0 | `(unchanged)` | agree |
| prolog | `v6/prolog/0_seq_expand.pl` | 0 | 0 | 0 | 2 | 0 | `(unchanged)` | agree |
| prolog | `v6/prolog/0_graph.pl` | 0 | 0 | 0 | 1 | 0 | `(unchanged)` | agree |
| prolog | `v6/prolog/0_body_walk.pl` | 1 | 0 | 1 | 6 | 1 | `v6/prolog/1_body_walk.pl` | hand_error |
| prolog | `v6/prolog/1_expansion.pl` | 1 | 1 | 1 | 2 | 1 | `(unchanged)` | agree |
| prolog | `v6/prolog/0_match_expand.pl` | 1 | 0 | 1 | 0 | 1 | `v6/prolog/1_match_expand.pl` | scale_shift |
| prolog | `v6/prolog/1_host_expand.pl` | 2 | 1 | 2 | 5 | 2 | `v6/prolog/2_host_expand.pl` | hand_error |
| prolog | `v6/prolog/0_program_check.pl` | 2 | 0 | 2 | 3 | 3 | `v6/prolog/2_program_check.pl` | hand_error |
| prolog | `v6/prolog/0_relation_edge_expand.pl` | 2 | 0 | 2 | 0 | 2 | `v6/prolog/2_relation_edge_expand.pl` | hand_error |
| prolog | `v6/prolog/0_relation_pattern.pl` | 2 | 0 | 2 | 0 | 3 | `v6/prolog/2_relation_pattern.pl` | hand_error |
| prolog | `v6/prolog/0_refusal_messages.pl` | 5 | 0 | 3 | 0 | 3 | `v6/prolog/3_refusal_messages.pl` | hand_error |
| prolog | `v6/prolog/compile/registry.pl` | 0 | - | 0 | 13 | 0 | `v6/prolog/compile/0_registry.pl` | unnumbered |
| prolog | `v6/prolog/compile/oracle_dump.pl` | 0 | - | 0 | 0 | 0 | `v6/prolog/compile/0_oracle_dump.pl` | unnumbered |
| prolog | `v6/prolog/compile/parse_dl.pl` | 1 | - | 1 | 1 | 1 | `v6/prolog/compile/1_parse_dl.pl` | unnumbered |
| prolog | `v6/prolog/compile/1_emit_registry_docs.pl` | 1 | 1 | 1 | 0 | 1 | `(unchanged)` | agree |
| prolog | `v6/prolog/compile/2_emit_cli_inventory.pl` | 1 | 2 | 1 | 0 | 1 | `v6/prolog/compile/1_emit_cli_inventory.pl` | scale_shift |
| prolog | `v6/prolog/compile/analyze.pl` | 3 | - | 2 | 7 | 5 | `v6/prolog/compile/2_analyze.pl` | unnumbered |
| prolog | `v6/prolog/compile/lower.pl` | 4 | - | 3 | 3 | 7 | `v6/prolog/compile/3_lower.pl` | unnumbered |
| prolog | `v6/prolog/compile/3_clock_check.pl` | 4 | 3 | 3 | 2 | 7 | `(unchanged)` | agree |
| prolog | `v6/prolog/compile/strat.pl` | 4 | - | 3 | 1 | 2 | `v6/prolog/compile/3_strat.pl` | unnumbered |
| prolog | `v6/prolog/compile/print_dl.pl` | 4 | - | 3 | 0 | 2 | `v6/prolog/compile/3_print_dl.pl` | unnumbered |
| prolog | `v6/prolog/compile/compile.pl` | 5 | - | 4 | 2 | 9 | `v6/prolog/compile/4_compile.pl` | unnumbered |
| prolog | `v6/prolog/compile/emit_ts.pl` | 5 | - | 4 | 1 | 4 | `v6/prolog/compile/4_emit_ts.pl` | unnumbered |
| prolog | `v6/prolog/compile/6_profile.pl` | 6 | 6 | 5 | 0 | 1 | `v6/prolog/compile/5_profile.pl` | scale_shift |
| prolog | `v6/prolog/compile/sweep.pl` | 6 | - | 5 | 0 | 4 | `v6/prolog/compile/5_sweep.pl` | unnumbered |
| tsv2 | `v6/tsv2/cli/0_inventory.ts` | 0 | 0 | 0 | 1 | 0 | `(unchanged)` | agree |
| tsv2 | `v6/tsv2/cli/bop.ts` | 5 | - | 1 | 0 | 3 | `v6/tsv2/cli/1_bop.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/types.ts` | 0 | - | 0 | 16 | 0 | `v6/tsv2/runtime/0_types.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/rows.ts` | 1 | - | 1 | 2 | 1 | `v6/tsv2/runtime/1_rows.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/ticklog.ts` | 1 | - | 1 | 2 | 1 | `v6/tsv2/runtime/1_ticklog.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/2_boot.ts` | 1 | 2 | 1 | 1 | 1 | `v6/tsv2/runtime/1_boot.ts` | scale_shift |
| tsv2 | `v6/tsv2/runtime/scratchStore.ts` | 1 | - | 1 | 1 | 1 | `v6/tsv2/runtime/1_scratchStore.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/serveStats.ts` | 1 | - | 1 | 1 | 1 | `v6/tsv2/runtime/1_serveStats.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/1_incremental.ts` | 1 | 1 | 1 | 0 | 1 | `(unchanged)` | agree |
| tsv2 | `v6/tsv2/runtime/diff.ts` | 1 | - | 1 | 0 | 1 | `v6/tsv2/runtime/1_diff.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/structPlane.ts` | 1 | - | 1 | 0 | 1 | `v6/tsv2/runtime/1_structPlane.ts` | unnumbered |
| tsv2 | `v6/tsv2/runtime/tickLoop.ts` | 2 | - | 2 | 0 | 2 | `v6/tsv2/runtime/2_tickLoop.ts` | unnumbered |
| tsv2 | `v6/tsv2/serve/0_trace.ts` | 1 | 0 | 0 | 3 | 1 | `(unchanged)` | agree |
| tsv2 | `v6/tsv2/serve/0_compile.ts` | 1 | 0 | 0 | 1 | 1 | `(unchanged)` | agree |
| tsv2 | `v6/tsv2/serve/1_hosts.ts` | 2 | 1 | 1 | 2 | 3 | `(unchanged)` | agree |
| tsv2 | `v6/tsv2/serve/2_binds.ts` | 2 | 2 | 1 | 1 | 2 | `v6/tsv2/serve/1_binds.ts` | scale_shift |
| tsv2 | `v6/tsv2/serve/3_engine.ts` | 3 | 3 | 2 | 1 | 6 | `v6/tsv2/serve/2_engine.ts` | scale_shift |
| tsv2 | `v6/tsv2/serve/4_http.ts` | 4 | 4 | 3 | 2 | 7 | `v6/tsv2/serve/3_http.ts` | scale_shift |
| tsv2 | `v6/tsv2/serve/main.ts` | 5 | - | 4 | 0 | 1 | `v6/tsv2/serve/4_main.ts` | unnumbered |

### 6g. Folder ordering

| package | folder | derived rank | note |
|---|---|---:|---|
| dl | `v6/dl/src/0_generated` | 0 | |
| dl | `v6/dl/src` | 1 | |
| tsv2 | `v6/tsv2/runtime` | 0 | |
| tsv2 | `v6/tsv2/serve` | 1 | |
| tsv2 | `v6/tsv2/cli` | 2 | |
| prolog | `v6/prolog` | 0 | tie, quotient is cyclic |
| prolog | `v6/prolog/compile` | 1 | tie, quotient is cyclic |

The tsv2 and dl folder orderings are unambiguous and match how the packages are
already read. The prolog one is not derivable while the cycle exists; that is
the same finding again, and section 4c gives the two exact repairs that make it
derivable.

### 6h. In dl6

The prefix is derived in-language by counting distinct lower depths within the
folder, which is the classic rank-by-counting shape and needs no ordering
construct:

```
rel folder_depth_seen(folder: text, depth: int).
folder_depth_seen(folder, depth) <- file_depth(path, depth), file_folder(path, folder).

rel lower_depth(path: text, other_depth: int).
lower_depth(path, other_depth) <-
  file_depth(path, depth), file_folder(path, folder),
  folder_depth_seen(folder, other_depth), other_depth < depth.

rel prefix_positive(path: text, prefix: int).
prefix_positive(path, count(other_depth)) <- lower_depth(path, other_depth).
```

Both intermediate rels are bare rels, hence set tables, hence deduped, which is
what makes `count` a count of distinct depths rather than of files. Graded
through the oracle: 62 of 62 prefixes match networkx, **0 mismatches**.

---

## 7. SLOT-SCALE: measured, and the wall is much closer than 10k

Synthetic layered DAG (every edge runs to a strictly lower layer, 50 folders,
fanout 3, 8 layers), depth plus cohesion plus one cut, both engines, capped.

| files | edges | transitive pairs | referee depth | referee cohesion | referee cut | dl6 oracle |
|---:|---:|---:|---:|---:|---:|---|
| 100 | 249 | 1,157 | 0.001s | 0.001s | 0.001s | **4.5s** |
| 300 | 780 | 4,515 | 0.002s | 0.002s | 0.002s | **43.0s** |
| 1,000 | 2,613 | 19,088 | 0.005s | 0.006s | 0.008s | **TIMEOUT at 300s** |
| 3,000 | 7,869 | 61,386 | 0.014s | 0.018s | 0.022s | not attempted |
| 10,000 | 26,244 | not computed | 0.070s | 0.063s | 0.074s | not attempted |
| 30,000 | 78,740 | not computed | 0.212s | 0.221s | 0.408s | not attempted |

**What breaks at 10k nodes: the dl6 side, and it breaks two orders of magnitude
earlier than that.** The reference engine's wall sits between 300 and 1,000
files. From 100 to 300 the transitive-pair count grew 3.9x and the wall grew
9.6x, so the cost tracks roughly pairs^1.7; extrapolating that to 19,088 pairs
predicts about 5,100s, consistent with the 300s timeout.

The cause is not the engine being slow at rows. It is that `depth` is expressed
through an explicit transitive-closure rel, and a transitive closure over a
layered DAG is quadratic in nodes by definition. 62 files produced 388 `reach`
rows; 10,000 files would produce on the order of 10^6 to 10^7. **No amount of
engine work fixes an algorithm whose intermediate rel is the closure.** The fix
is a depth formulation that never materialises `reach`, which means either a
per-round `depth(node) = 1 + max(depth of dependency)` iteration (exactly the
shape `not_stratified` refuses, section 3c) or an ordered occurrence loop
(ARCH `pre_occurrence_loop`).

The referee side does not break. networkx computed depth, cohesion and one cut
over 30,000 files and 78,740 edges in 0.84s total. The offline referee is not
the bottleneck and will not be at v5-corpus scale.

Two caveats stated rather than glossed: the synthetic graph assigns folders
round-robin, so its modularity is near zero and it is a TIMING harness rather
than a structure model; and the 10k/30k rows omit transitive pairs because
computing them in networkx costs more than the measurement is worth.

**SLOT-SCALE resolution: the metric layer scales; the in-language expression
does not, and the blocker is the closure rel, not the engine.** Running this
against the real v5 corpus graph today means running the referee, feeding dl6
the RESULT, or reformulating depth without a closure.

---

## 8. Grading references from the header

**The prolog packaging research lane** (`plans/2026-07-31-prolog-packaging-research.md`)
prices human partitions of the same prolog code. The auto-factorizer's
independent answer, arrived at four separate ways (Louvain splitting only the
prolog package; the `v6/prolog` folder's 0.243 ratio; the single edge-positive
folder cut; all six hand-numbering errors), is that the prolog package is the
one that is not one thing, and the specific defect is the
`v6/prolog <-> v6/prolog/compile` cycle with `registry.pl` in the wrong folder.
Whether that matches the human partitions is a comparison the next reader should
make; the lab states its own answer without having read theirs, which is what
makes the comparison worth anything.

**The restart carve** (extract / store / vscode / prolog / goldens) is a
repo-granularity partition this fact base cannot grade: at file granularity the
three packages here already have zero crossing edges, so any coarser partition
containing them scores 1.0 trivially. Grading the restart carve needs a fact base
spanning the whole repo, which the atlas does not cover.

---

## 9. Slot resolutions

| slot | resolution |
|---|---|
| **SLOT-METRIC** | Edge-count delta is the ranking key. Modularity is REJECTED as a ranking key for cuts, with the reason: a cut changes the node set, so Q before and after are different quantities, and the interface node's group assignment is a free parameter. Q is reported both ways beside the ranking, never as it. |
| **SLOT-CUT-GRANULARITY** | Both, answering different questions. Edge-set cuts answer "what interface should exist" (section 4a/4b). Node-relocation cuts answer "what is in the wrong folder" (4c), and on this codebase that is the one with the actionable answer. |
| **SLOT-TYPE-AXIS** | Not applicable on this fact base and not manufactured: the atlas emits no `sig` records, and prolog has no types. The axis is per-plane and empty on four of six planes. Its shape when the extractor's sig family is projected: group symbols by the declared type of their interface binding. |
| **SLOT-SCALE** | The referee scales (30,000 files in 0.84s). The dl6 expression walls between 300 and 1,000 files because depth rides an explicit transitive-closure rel, which is quadratic by definition. Numbers in section 7. |

One slot the lab opened rather than closed, because the header did not name it
and four ledger rows share it:

**SLOT-SEQUENTIAL-STEP.** Louvain (A9), max-flow min-cut (A25), SCC condensation
(A4) and cut depth under a cycle (A14) are all blocked by the same thing: a step
that reads state written earlier in the SAME step. Three of the four should stay
bought, and the lab says so. The fourth, condensation, is already a set-oriented
fixpoint and is expressible today at the cost of a second closure, so the open
question is not whether it can be spelled but whether it deserves a cheaper
spelling given that the first closure is what walls at 1,000 files. That is a
cost question the `pre_occurrence_loop` arc would answer alongside its own.

---

## 10. Named refusals and defects, all first-class findings

| # | finding | status |
|---|---|---|
| 1 | Label propagation as defined refuses `not_stratified` in the reference engine. Recursion through an aggregate; the guard is correct and it means LPA has no dl6 spelling today. | correct refusal, recorded |
| 2 | That refusal prints as `Unknown message: not_stratified`, no file, no line, no rule. Language design review finding B4, reproduced. | pre-existing, unowned |
| 3 | `min`/`max` over a TEXT column: compiler refuses by name (`aggregate_operand_not_number`), reference engine crashes in `lists:min_list/3`. Engine-side mirror missing. Four-line repro in section 3d. | NEW, recommend ARCH row `aggregate_operand_not_number_engine_mirror` |
| 4 | `dot -Tplain` ranks are network-simplex layout, not longest-path depth. 15 of 62 differ, all pulled down toward consumers. Using dot ranks as a toposort answer key is wrong. | method correction |
| 5 | Depth via an explicit closure rel is quadratic and walls under 1,000 files. Needs a per-round formulation, which is exactly what finding 1 refuses. | blocks scale, section 7 |
| 6 | `v6/prolog <-> v6/prolog/compile` folder cycle, 7 edges down and 21 up. No single file move repairs it. Exact minimal repairs computed. | codebase defect, section 4c |
| 7 | Six hand-numbering contradictions, all in the prolog package, all reachable from the same cycle. Verified in source. | codebase defect, section 6d |
| 8 | Edge kind is not recoverable from the atlas `.dot` beyond a style class: `flag`/`record` collapse, and all six `bridge_*` kinds collapse. | fact-base limit, stated |
| 9 | An aggregate emits NO ROW for an empty group, so any formula where an empty group still owes a term silently loses it. Cost the lab a wrong modularity (0.0 against -0.0278) that looked plausible. Fix is an explicit `coalesce` against the group rel. Review finding A11 biting a real program. | pre-existing, now with a worked example |
| 10 | Ordered comparison is `both_number`, so two text keys cannot be put in canonical order. Forced a world-fed `node_index/2` to dedup an undirected edge. | expressiveness gap, section A24 |
| 11 | No string split, so the language cannot read the numeric prefix off its own filenames. The prefix must arrive as a world row. | expressiveness gap, section A23; the atlas hit the same wall |
| 12 | Splicing an interface into an acyclic graph can create a cycle, and the bounded recursion returns the hop floor rather than a depth. Repaired in-language by `cycle_after/2` so the wrong number names itself. | found by grading, repaired, section 4d |
| 13 | Four separate gaps (SCC condensation, Louvain, cut depth under cycles, min-cut) are ONE structural gap: an algorithm whose step reads state written earlier in the same step. Buying the referee is the correct answer for the last three; only condensation is worth a construct. | ledger A4/A9/A14/A25 |

---

## 11. Receipts

Every command ran under `v6/tools/run-capped.sh` with a stated budget. No
uncapped runs. Scratch output stayed in the lab's own `out/`; no daemon, no
`~/.local/state`.

| # | question | receipt | result |
|---|---|---|---|
| R0 | fact base | atlas `.dot` parse vs the atlas's own derived counts | 421 nodes, 809 edges, all 6 planes and all 7 kind classes exact |
| R0b | fact base | regex TS plane cross-graded against the extractor plane on the overlap | 38/38, 0 only-regex, 0 only-atlas |
| R1 | Q1 | `factorize.dl6` through `dl6_oracle.pl` vs networkx | 62/62 depths, 0 mismatches |
| R1b | Q1 | `dot -Tplain` on the reversed graph | 0 uphill edges both orderings, same max 6, 47/62 equal, all 15 differences pulled down |
| R1c | Q1 | sabotage: hop floor `prior < 64` -> `prior < 2` | depth 16 mismatches, prefix 14, cohesion 0 |
| R1d | Q2/Q6 | sabotage: one file fed the wrong folder | cohesion 2 mismatches, depth 0 (correct), prefix 0 (see below) |
| R2 | Q2 | `axis_internal_total` + `axis_crossing_total` in dl6 vs networkx | 4 axes x every group x 2 columns, 0 mismatches |
| R2b | Q2 | `modularity_scaled` exact integer vs networkx float | file -0.0278, folder 0.4374, package 0.6300, plane 0.4997, all 4 exact |
| R3 | Q3 | candidate research before any clustering code | 10 candidates priced, section 3a |
| R3b | Q3 | `lpa.dl6` through the reference engine | `not_stratified`, the named refusal |
| R3c | Q3 | `components.dl6` vs `networkx.connected_components` | matches |
| R3d | Q3 | `min_text_repro.dl6` through both doors | compiler names it, engine crashes |
| R4 | Q4 | exhaustive folder-boundary cuts, both granularities | 6 file-level, 200+ symbol-level, table in 4a/4b |
| R4b | Q4 | dl6 cut algebra vs networkx applied cuts | `edges_after` 6/6 exact, depth 5/6 exact, cycle flag exact on the sixth |
| R5 | Q5 | exhaustive relocation search plus exact min-cut | 2-move and 9-move minima, section 4c |
| R5b | Q5 | `dense_rank` + `competition_rank` + `best_cut` in dl6 | 21 candidates, both ranks exact, best cut agrees |
| R6 | Q6 | `derived_violation/4` in dl6 | 0 rows over 123 edges |
| R6b | Q6 | classification of all 38 numbered files | 15 agree / 16 shift / 6 hand error / 1 metric blind |
| R6c | Q6 | `folder_depth/2` + `folder_cycle/1` in dl6 vs networkx quotient | depths match, cycle found on exactly `v6/prolog` and `v6/prolog/compile` |
| R6d | Q6 | `hand_violation/4`, `hand_agrees/2`, `hand_differs/3`, `metric_blind/2` in dl6 | 2 violations / 15 / 23 / 1, all matching the referee |
| R7 | SLOT-SCALE | both engines at 100 / 300 / 1k / 3k / 10k / 30k | engine walls between 300 and 1k, referee flat to 30k |

**Sabotage receipts (R1c, R1d), because a grade that cannot go red proves
nothing.** Two independent sabotages, each flipping a different column, so no
grade passes both by accident. Truncating the hop floor moves depth (16) and
prefix (14) and leaves cohesion alone. Feeding one file the wrong folder moves
cohesion (2) and leaves depth alone, which is correct since depth never reads
folders. Between them all three graded columns are shown to be discriminating.

The folder swap did NOT move the prefix, contradicting the draft expectation
this receipt was written with, and the reason is a real coarseness worth
recording: the prefix is a dense rank over the DISTINCT DEPTHS a folder holds,
so relocating one file changes no prefix whenever its depth is already
represented in both folders' ladders. The expectation was corrected to the
measurement rather than the measurement worked around.

---

## 12. What a follow-up arc would do

Nothing in this lab is applied and nothing should be applied from it without the
user reading section 6f. Three things are ready to be decided:

1. **The prolog folder cycle.** Two exact repairs, 2 moves or 9 moves, numbers
   in section 4c. This is a real defect with a real fix, independent of any
   renaming.
2. **The `analyze.pl` facade.** The single highest-value interface the symbol
   graph proposes: 51 edges through 25 tails to 14 heads, erasing 12 edges and
   dropping the largest cluster from 97 symbols to 79.
3. **`aggregate_operand_not_number` engine mirror.** Four-line repro, and the
   `0_program_check.pl` shared-side pattern is the fix shape.

Three language-side rows the ledger earns, priced smallest first:

4. **`coalesce` against the group rel is the empty-group idiom, and nothing says
   so.** Finding 9 cost this lab a plausible wrong number. The cheapest fix is
   documentation plus a fixture; the honest fix is a check that an aggregate
   feeding an arithmetic expression over a group rel has a filled source.
5. **Ordered comparison on text (finding 10).** `both_number` on `<` forces a
   world-fed index for any canonical-ordering job. SQLite orders text natively,
   so this is a type-rule choice rather than a lowering obstacle.
6. **SCC condensation (ledger A4).** The one gap of the four worth a construct,
   because it is the only one that is a set-oriented fixpoint already: mutual
   reachability plus a min-index representative. It is expressible today at the
   cost of a second closure, so what a construct buys is the cost, not the
   expressiveness, and section 7 says the cost is what walls.

Louvain, min-cut and cut-depth-under-cycles (A9, A25, A14) should NOT get
constructs. They need state written earlier in the same step; the referee is
bought, and buying it is the standing law's own answer.

The rename table itself is a user decision and a separate arc, because renaming
touches every importer.
