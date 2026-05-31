# Further study

The two side surveys ([logic languages](logic-language-survey/README.md),
[beyond manual paging](beyond-manual-paging/README.md)) mapped two neighbourhoods.
This page is the backlog of the rest: concepts adjacent to a reactive
Datalog-over-code engine that are worth their own survey later, grouped by how
close they sit to what `dl` already is. Each row says why it matters *here* and
whether there is already a skill or math chapter touching it.

## Closest to the engine — join and evaluation

| Concept | Why it matters here | Status |
|---|---|---|
| **Worst-case optimal joins** — Leapfrog Triejoin, generic join, free join (Ngo–Ré–Rudra; Veldhuizen 2014) | Binary-join Datalog is provably suboptimal on cyclic query patterns. The `reaches`-style joins are exactly the shape WCOJ fixes. The biggest *evaluation* idea not yet touched | unexplored |
| **Bitmap relational representation** — Roaring bitmaps | Datalog joins are set AND/OR. Roaring makes relations compact and the joins SIMD-fast and resident. Pairs with the succinct-structures survey | unexplored |
| **Derivation source tracking** — semiring-annotated derivations (Green–Karvounarakis–Tannen 2007) | The clean algebra under "which source rows support this derived fact", the thing the incremental deletes already chase by hand. Extends [math chapter 3](math/03-semirings.md) | partial |

## The program-analysis bridge — the big missing link

| Concept | Why it matters here | Status |
|---|---|---|
| **CFL- / Dyck-reachability** (Reps 1998) | Turns plain graph reachability into *context-sensitive* interprocedural analysis: matched-paren edges = matched call/return. One step from "who calls whom" to real static analysis | unexplored |
| **IFDS / IDE** (Reps–Horwitz–Sagiv 1995) | The framework that compiles dataflow analyses down to CFL-reachability. Doop and friends are built on it | unexplored |
| **Points-to: Andersen vs Steensgaard** | The canonical Datalog-as-analysis workload; calibrates what the engine would need to host real analyses | unexplored |

## Incremental and reactive — architecture forks

| Concept | Why it matters here | Status |
|---|---|---|
| **Demand-driven vs change-driven incremental** — Salsa/Adapton (pull) vs Differential Dataflow (push) | The real architecture fork: recompute-on-query versus propagate-on-edit. rust-analyzer is Salsa, Materialize is Differential. `dl` is closer to push; knowing the pull side sharpens the choice | skill `dataflow-incremental`, not surveyed |
| **Progress tracking / frontiers** — Timely's partial-order timestamps | How a streaming engine knows a round is *done* under concurrent edits | skill-adjacent |

## Storage and consistency

| Concept | Why it matters here | Status |
|---|---|---|
| **LSM-trees vs B-trees** (RocksDB/LevelDB) | The engine is write-heavy (every edit mutates facts); SQLite is a B-tree. LSM is the write-optimised alternative | unexplored |
| **MVCC / snapshot isolation** | How a query sees a consistent snapshot while edits stream in. Hit the moment reads and edits overlap | unexplored |
| **CRDTs** | Merge of fact stores across the repo/rev layers of the data model. Relevant if cross-repo returns | unexplored |

## Term-level and optimisation

| Concept | Why it matters here | Status |
|---|---|---|
| **E-graphs / equality saturation** — egg (Willsey 2021) | Compact representation of many equivalent terms. Could power query optimisation or the type-IR | unexplored |
| **Fine-grained complexity** — BMM / APSP conjectures | Tells you what is *provably* not improvable about transitive closure, so you stop trying | unexplored |

## Suggested order

The three highest-leverage, cheapest-first:

1. **Worst-case optimal joins** — changes how you think about every recursive
   rule; sits right next to the magic-sets and semi-naive threads.
2. **CFL-reachability + IFDS** — reframes the whole engine as a static-analysis
   platform rather than a graph database.
3. **Roaring bitmaps** — the cheapest concrete win, and it composes with the
   succinct-structures survey already written.

When one of these gets its own survey, give it a directory of the same shape as
the other two (a `README.md` frame plus a numbered detail chapter) and link it
from the [book index](README.md).
