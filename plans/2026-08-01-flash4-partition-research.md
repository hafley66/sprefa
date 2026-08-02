# File partitioning + reading order: v6 research

Scope: `v6/prolog/` compiler front (27 files) and `v6/tsv2/` `runtime/` `serve/`
`cli/` `tests/` (19 + 41 files) = **87 files, 169 reference edges**. `dl/`,
`src/`, `examples/`, `gen_emitted/`, `conformance/`, `compile/{scripts,test,pipelines}`
deps are out of scope; edges to them are recorded but not part of the node set.

Method per half: prolog edges are `use_module`/`consult` declarations
(hand-collected by grep); TypeScript edges are regex over `from "..."`/`require(`
with relative specifiers resolved to in-scope files. A cross-file predicate call
in prolog requires an import, so `use_module` is the dependency proxy and
predicate-call edges are subsumed by it. All computation in python + networkx
(offline referee; no repo edits).

---

## 1. Honest assessment

What this is: a Prolog-to-TypeScript compiler for a datalog-over-rxjs language,
with a SQLite-backed TypeScript runtime (`tsv2`) and serve/CLI. The Prolog half
is a 27-file DAG that proceeds surface/type analysis -> sugar expansion ->
analyze -> lower -> emit -> driver. The TypeScript half is a lean 19-file
runtime/serve/cli plus 41 test files.

Is the file organization the main readability obstacle? Partly, but not the
main one. The folders are already near the modularity optimum (`Q` 0.36 on the
46 core files); the structural defect the prior lab named (the
`prolog <-> prolog/compile` folder cycle) has been killed by the flatten that
executed, and `compile/` now holds what is essentially two reach-in files
(`registry.pl`, `parse_dl.pl`) plus three emitters. The real obstacle is that
the **numbering is not a dependency order**. 21 of the 27 prolog files carry no
numeric prefix; the ones that do are ordered by author intent, not by
read-before-use; and the prolog root is a flat bag mixing `0_type_plane.pl`,
`1_expansion.pl`, `3_clock_check.pl`, `6_profile.pl`, with no sub-tier labels.
A person opening files in name order does not read the system; they read a
jumble.

Top 3 changes for a first-time human reader:
1. Give the whole tree one dependency-ordered reading sequence (this report's
   central deliverable). It is obtainable today because the graph is a DAG.
2. Pull `registry.pl` and `parse_dl.pl` out of `compile/` into a substrate tier
   read first. `registry` has 13 in-package dependents, the single most
   depended-on prolog file, and today everything in the prolog root reaches
   down a subfolder to get it.
3. Group the prolog root into named phase tiers (substrate / surface / expand /
   analyze / emit) so a filename's folder tells you which compiler phase you
   are in. This is chaptering, not repartitioning; the modularity cost is small.

---

## 2. Graph numbers

| graph | nodes | edges | SCCs (>1) | edge-list source |
|---|---:|---:|---:|---|
| prolog | 27 | 60 | 0 | use_module/consult |
| tsv2 core (runtime+serve+cli) | 19 | 34 | 0 | import regex |
| tsv2 all (core+tests) | 60 | 109 | 0 | import regex |
| combined core | 46 | 94 | 0 | both |
| combined all | 87 | 169 | 0 | both |

All three graphs are acyclic at file granularity (0 nontrivial SCCs). The
flatten's invariant (deps point `prolog -> prolog/compile`, not back) holds, so
a linear read-before-use order exists. The cross-package picture: prolog and
tsv2 are **disconnected** at file granularity (94 core edges, none cross
package), so they meet only through out-of-scope bridge/atlas facts; the global
order concatenates the two paragraphs in any order.

LouVain (undirected, networkx, seed 1):
- combined core: 4 communities, `Q=0.5134`, sizes [20,19,6,1] = prolog body /
  all-of-tsv2 / prolog sugar / `oracle_dump`.
- prolog alone: 5 communities, `Q=0.2967`.
- tsv2 core alone: 4 communities, `Q=0.2236`.

Reading: Louvain does not split tsv2 past its existing folders (it is one
cohesive package, `Q` there is low because every folder already references
`runtime/types.ts`). It does split prolog into a "sugar expansion" tier and the
compiler body, which matches the phase chain below.

---

## 3. Reading-order proposal

The order is a **valid topological order with folders as contiguous blocks**:
folders are ordered by a Kahn pass over the folder-quotient DAG, then files
within each folder by a Kahn topo (read-before-use), then blocks concatenated.

Tie-break choice. The prior lab ranked by depth then in-degree. Because folders
are contiguous blocks of one global topo, the ordering tie-break **cannot change
the cohesion metric at all** — cross-folder edges are fixed by the group
assignment, not the order. So the tie-break's only job is intra-block read
order. I used: among simultaneously-available nodes, **most-depended-on first
(highest in-degree), then name ascending**. Rationale: read the base a file
imports before the file, and within one tier put the orphan seeds first. I
verified all alternatives (depth+indegree, DFS post-order) also give 0
violations; none is objectively better on cohesion because cohesion is set by
grouping, not ordering.

**Invariant verified: 0 violations over all 169 edges.** Reading folder-by-folder
in the order below, then files by prefix within a folder, every dependency (its
head) is already read. This is the owner's goal 1, and it holds by construction
and by check, not by eye.

Folder order (quotient, forced by the graph):

```
prolog/  substrate -> surface -> expand -> host -> analyze -> gate -> pipeline
tsv2/    runtime -> serve -> cli -> tests
```

The prolog order is forced, not chosen: `surface` before `expand` (expand
imports the surface), `expand` before `analyze` (`analyze` imports
`1_expansion`), `analyze` before `gate` (`3_clock_check` imports `analyze`),
`gate`+`analyze` before `pipeline` (`lower`/`emit_ts`/`compile` import them).

---

## 4. Grouping / nesting proposal

Prolog: 7 named phase folders (from Louvain, relabeled; `registry`/`parse_dl`
moved to a substrate tier — the prior verdict's relocation finding). tsv2:
reuse the existing `runtime`/`serve`/`cli` folders unchanged (Louvain says they
are already right), tests stay a final leaf folder.

Internal / crossing per folder (crossing = directed edges with exactly one
endpoint in the folder; tests included):

| proposed folder | files | internal | crossing | internal frac |
|---|---:|---:|---:|---:|
| prolog/substrate | 4 | 1 | 15 | 0.062 |
| prolog/surface | 5 | 6 | 13 | 0.316 |
| prolog/expand | 5 | 5 | 3 | 0.625 |
| prolog/host | 1 | 0 | 4 | 0.0 |
| prolog/analyze | 3 | 2 | 11 | 0.154 |
| prolog/gate | 4 | 1 | 9 | 0.10 |
| prolog/pipeline | 5 | 9 | 17 | 0.346 |
| tsv2/runtime | 10 | 10 | 10 | 0.50 |
| tsv2/serve | 7 | 9 | 9 | 0.50 |
| tsv2/cli | 2 | 1 | 1 | 0.50 |
| tsv2/tests | 41 | 0 | 0 | - |

Modularity (undirected, core 46 files): current folders `Q=0.3614`; proposed
`Q=0.3341`; proposed over all 87 (tests as one group) `Q=0.0878` (dragged down
by the test leaf-bag). The coarse current partition is slightly more modular
than the fine one; the fine one trades `+0.027` of `Q` for named phase folders.
That tradeoff is the honest price of goal 2's "folders must not contain
decohesive things."

Tree distance between any two referencing groups: **2 before and 2 after**
(sibling groups under the package root; packages never reference each other, so
no referencing pair spans `prolog`/`tsv2`). The hop goal was already met and
stays met.

---

## 5. THE TABLE

Read `#` as the global reading index (1..87); read folders in the order above,
then files by `new` prefix. `old` is the numeric prefix present today (`-` =
none). `new` is the per-folder dense prefix. Numbers strictly increase along
the reading order; prefixes are unique within each folder.

| # | current path | folder | old | new | proposed path |
|--:|---|:--|:--:|:--:|---|
| 1 | v6/prolog/0_graph.pl | substrate | 0 | 00 | v6/prolog/substrate/00_graph.pl |
| 2 | v6/prolog/compile/oracle_dump.pl | substrate | - | 01 | v6/prolog/substrate/01_oracle_dump.pl |
| 3 | v6/prolog/compile/registry.pl | substrate | - | 02 | v6/prolog/substrate/02_registry.pl |
| 4 | v6/prolog/compile/parse_dl.pl | substrate | - | 03 | v6/prolog/substrate/03_parse_dl.pl |
| 5 | v6/prolog/0_body_walk.pl | surface | 0 | 00 | v6/prolog/surface/00_body_walk.pl |
| 6 | v6/prolog/0_type_plane.pl | surface | 0 | 01 | v6/prolog/surface/01_type_plane.pl |
| 7 | v6/prolog/0_program_check.pl | surface | 0 | 02 | v6/prolog/surface/02_program_check.pl |
| 8 | v6/prolog/0_relation_edge_expand.pl | surface | 0 | 03 | v6/prolog/surface/03_relation_edge_expand.pl |
| 9 | v6/prolog/0_relation_pattern.pl | surface | 0 | 04 | v6/prolog/surface/04_relation_pattern.pl |
| 10 | v6/prolog/0_coalesce_expand.pl | expand | 0 | 00 | v6/prolog/expand/00_coalesce_expand.pl |
| 11 | v6/prolog/0_enum_expand.pl | expand | 0 | 01 | v6/prolog/expand/01_enum_expand.pl |
| 12 | v6/prolog/0_match_expand.pl | expand | 0 | 02 | v6/prolog/expand/02_match_expand.pl |
| 13 | v6/prolog/0_seq_expand.pl | expand | 0 | 03 | v6/prolog/expand/03_seq_expand.pl |
| 14 | v6/prolog/1_expansion.pl | expand | 1 | 04 | v6/prolog/expand/04_expansion.pl |
| 15 | v6/prolog/1_host_expand.pl | host | 1 | 00 | v6/prolog/host/00_host_expand.pl |
| 16 | v6/prolog/analyze.pl | analyze | - | 00 | v6/prolog/analyze/00_analyze.pl |
| 17 | v6/prolog/print_dl.pl | analyze | - | 01 | v6/prolog/analyze/01_print_dl.pl |
| 18 | v6/prolog/strat.pl | analyze | - | 02 | v6/prolog/analyze/02_strat.pl |
| 19 | v6/prolog/3_clock_check.pl | gate | 3 | 00 | v6/prolog/gate/00_clock_check.pl |
| 20 | v6/prolog/0_refusal_messages.pl | gate | 0 | 01 | v6/prolog/gate/01_refusal_messages.pl |
| 21 | v6/prolog/compile/1_emit_registry_docs.pl | gate | 1 | 02 | v6/prolog/gate/02_emit_registry_docs.pl |
| 22 | v6/prolog/compile/2_emit_cli_inventory.pl | gate | 2 | 03 | v6/prolog/gate/03_emit_cli_inventory.pl |
| 23 | v6/prolog/lower.pl | pipeline | - | 00 | v6/prolog/pipeline/00_lower.pl |
| 24 | v6/prolog/emit_ts.pl | pipeline | - | 01 | v6/prolog/pipeline/01_emit_ts.pl |
| 25 | v6/prolog/compile.pl | pipeline | - | 02 | v6/prolog/pipeline/02_compile.pl |
| 26 | v6/prolog/6_profile.pl | pipeline | 6 | 03 | v6/prolog/pipeline/03_profile.pl |
| 27 | v6/prolog/sweep.pl | pipeline | - | 04 | v6/prolog/pipeline/04_sweep.pl |
| 28 | v6/runtime/types.ts | runtime | - | 00 | v6/tsv2/runtime/00_types.ts |
| 29 | v6/runtime/1_incremental.ts | runtime | 1 | 01 | v6/tsv2/runtime/01_incremental.ts |
| 30 | v6/runtime/2_boot.ts | runtime | 2 | 02 | v6/tsv2/runtime/02_boot.ts |
| 31 | v6/runtime/diff.ts | runtime | - | 03 | v6/tsv2/runtime/03_diff.ts |
| 32 | v6/runtime/rows.ts | runtime | - | 04 | v6/tsv2/runtime/04_rows.ts |
| 33 | v6/runtime/scratchStore.ts | runtime | - | 05 | v6/tsv2/runtime/05_scratchStore.ts |
| 34 | v6/runtime/serveStats.ts | runtime | - | 06 | v6/tsv2/runtime/06_serveStats.ts |
| 35 | v6/runtime/structPlane.ts | runtime | - | 07 | v6/tsv2/runtime/07_structPlane.ts |
| 36 | v6/runtime/ticklog.ts | runtime | - | 08 | v6/tsv2/runtime/08_ticklog.ts |
| 37 | v6/runtime/tickLoop.ts | runtime | - | 09 | v6/tsv2/runtime/09_tickLoop.ts |
| 38 | v6/serve/0_compile.ts | serve | 0 | 00 | v6/tsv2/serve/00_compile.ts |
| 39 | v6/serve/0_trace.ts | serve | 0 | 01 | v6/tsv2/serve/01_trace.ts |
| 40 | v6/serve/1_hosts.ts | serve | 1 | 02 | v6/tsv2/serve/02_hosts.ts |
| 41 | v6/serve/3_engine.ts | serve | 3 | 03 | v6/tsv2/serve/03_engine.ts |
| 42 | v6/serve/2_binds.ts | serve | 2 | 04 | v6/tsv2/serve/04_binds.ts |
| 43 | v6/serve/4_http.ts | serve | 4 | 05 | v6/tsv2/serve/05_http.ts |
| 44 | v6/serve/main.ts | serve | - | 06 | v6/tsv2/serve/06_main.ts |
| 45 | v6/cli/0_inventory.ts | cli | 0 | 00 | v6/tsv2/cli/00_inventory.ts |
| 46 | v6/cli/bop.ts | cli | - | 01 | v6/tsv2/cli/01_bop.ts |
| 47 | v6/tests/departureFrontier.test.ts | tests | - | 00 | v6/tsv2/tests/00_departureFrontier.test.ts |
| 48 | v6/tests/structPlane.test.ts | tests | - | 01 | v6/tsv2/tests/01_structPlane.test.ts |
| 49 | v6/tests/9_ordered_aggregate.test.ts | tests | 9 | 02 | v6/tsv2/tests/02_ordered_aggregate.test.ts |
| 50 | v6/tests/extraDrainTick.test.ts | tests | - | 03 | v6/tsv2/tests/03_extraDrainTick.test.ts |
| 51 | v6/tests/relationDepth.test.ts | tests | - | 04 | v6/tsv2/tests/04_relationDepth.test.ts |
| 52 | v6/tests/1_incremental_affinity_drop.test.ts | tests | 1 | 05 | v6/tsv2/tests/05_incremental_affinity_drop.test.ts |
| 53 | v6/tests/6_host-extraction-batching.test.ts | tests | 6 | 06 | v6/tsv2/tests/06_host-extraction-batching.test.ts |
| 54 | v6/tests/7_value-plane.test.ts | tests | 7 | 07 | v6/tsv2/tests/07_value-plane.test.ts |
| 55 | v6/tests/bootBind.test.ts | tests | - | 08 | v6/tsv2/tests/08_bootBind.test.ts |
| 56 | v6/tests/engineFault.test.ts | tests | - | 09 | v6/tsv2/tests/09_engineFault.test.ts |
| 57 | v6/tests/levelFreeze.test.ts | tests | - | 10 | v6/tsv2/tests/10_levelFreeze.test.ts |
| 58 | v6/tests/normRuntime.test.ts | tests | - | 11 | v6/tsv2/tests/11_normRuntime.test.ts |
| 59 | v6/tests/orderedPre.test.ts | tests | - | 12 | v6/tsv2/tests/12_orderedPre.test.ts |
| 60 | v6/tests/serveDrain.test.ts | tests | - | 13 | v6/tsv2/tests/13_serveDrain.test.ts |
| 61 | v6/tests/tickLoop.test.ts | tests | - | 14 | v6/tsv2/tests/14_tickLoop.test.ts |
| 62 | v6/tests/aggregateScope.test.ts | tests | - | 15 | v6/tsv2/tests/15_aggregateScope.test.ts |
| 63 | v6/tests/coalesceCounts.test.ts | tests | - | 16 | v6/tsv2/tests/16_coalesceCounts.test.ts |
| 64 | v6/tests/edgeGuard.test.ts | tests | - | 17 | v6/tsv2/tests/17_edgeGuard.test.ts |
| 65 | v6/tests/serveWatch.test.ts | tests | - | 18 | v6/tsv2/tests/18_serveWatch.test.ts |
| 66 | v6/tests/tickCounter.test.ts | tests | - | 19 | v6/tsv2/tests/19_tickCounter.test.ts |
| 67 | v6/tests/watchBootReconcile.test.ts | tests | - | 20 | v6/tsv2/tests/20_watchBootReconcile.test.ts |
| 68 | v6/tests/watchCounts.test.ts | tests | - | 21 | v6/tsv2/tests/21_watchCounts.test.ts |
| 69 | v6/tests/watchGlobDialect.test.ts | tests | - | 22 | v6/tsv2/tests/22_watchGlobDialect.test.ts |
| 70 | v6/tests/diff.test.ts | tests | - | 23 | v6/tsv2/tests/23_diff.test.ts |
| 71 | v6/tests/goldenFlexServed.test.ts | tests | - | 24 | v6/tsv2/tests/24_goldenFlexServed.test.ts |
| 72 | v6/tests/hostDecode.test.ts | tests | - | 25 | v6/tsv2/tests/25_hostDecode.test.ts |
| 73 | v6/tests/retentionCount.test.ts | tests | - | 26 | v6/tsv2/tests/26_retentionCount.test.ts |
| 74 | v6/tests/serveDoor.test.ts | tests | - | 27 | v6/tsv2/tests/27_serveDoor.test.ts |
| 75 | v6/tests/serveHost.test.ts | tests | - | 28 | v6/tsv2/tests/28_serveHost.test.ts |
| 76 | v6/tests/serveStats.test.ts | tests | - | 29 | v6/tsv2/tests/29_serveStats.test.ts |
| 77 | v6/tests/5_file-watch-scale.test.ts | tests | 5 | 30 | v6/tsv2/tests/30_file-watch-scale.test.ts |
| 78 | v6/tests/bopCheck.test.ts | tests | - | 31 | v6/tsv2/tests/31_bopCheck.test.ts |
| 79 | v6/tests/bopCommandInventory.test.ts | tests | - | 32 | v6/tsv2/tests/32_bopCommandInventory.test.ts |
| 80 | v6/tests/bopLoadQuery.test.ts | tests | - | 33 | v6/tsv2/tests/33_bopLoadQuery.test.ts |
| 81 | v6/tests/bopRun.test.ts | tests | - | 34 | v6/tsv2/tests/34_bopRun.test.ts |
| 82 | v6/tests/crawlOrg.test.ts | tests | - | 35 | v6/tsv2/tests/35_crawlOrg.test.ts |
| 83 | v6/tests/hostTemplateQuoting.test.ts | tests | - | 36 | v6/tsv2/tests/36_hostTemplateQuoting.test.ts |
| 84 | v6/tests/serveArrivalValidation.test.ts | tests | - | 37 | v6/tsv2/tests/37_serveArrivalValidation.test.ts |
| 85 | v6/tests/serveCompileBudget.test.ts | tests | - | 38 | v6/tsv2/tests/38_serveCompileBudget.test.ts |
| 86 | v6/tests/serveLeak.test.ts | tests | - | 39 | v6/tsv2/tests/39_serveLeak.test.ts |
| 87 | v6/tests/serveLifecycle.test.ts | tests | - | 40 | v6/tsv2/tests/40_serveLifecycle.test.ts |

Not in the node set, read once before the compiler front: `v6/prolog/ARCH.pl`
(204 KB, a self-checking architecture database, not a run-loadable module) and
`v6/prolog/compile/PIPELINE.md`/`SYNTAX.md`/`TICK-MODEL.md` (prose).

---

## 6. Self-grade

| metric | before | after |
|---|---:|---:|
| directed cross-folder edges (all 87, tests incl.) | 101 | 125 |
| directed cross-folder edges (core 46 only) | 26 | 50 |
| decohesive files (majority of edges leave folder, core) | 5 | 16 |
| modularity (core undirected) | 0.3614 | 0.3341 |
| max hops in folder tree, any two referencing groups | 2 | 2 |
| reading order valid (dep head before tail over 169 edges) | n/a (no order) | **0 violations** |

The reading-order deliverable (goal 1) is met and verified. Max-hop between
referencing groups (goal 3) is already minimal and stays minimal. What the
proposal trades away: cross-folder edges and naive decohesion rise because
prolog goes from a coarse 2-folder split to 7 named tiers. Two caveats make
this rise less bad than it looks, stated rather than glossed:

- The naive "majority of edges leave its folder" metric misfires on foundation
  and boundary files by construction: `substrate` is 6% internal only because
  everything else in the package imports into it. A base tier is "decohesive"
  by that metric and is exactly what you want as a base. The metric is reported
  and not overweighted.
- The added crossings are between adjacent tiers in the reading order (the
  phases a reader legitimately reads back against); they replace the
  `prolog root -> compile/` reach-in that the 9-move flatten left behind.

Goal 2's "folders must not contain decohesive things" and goal 3's "min
cross-group edges" pull in opposite directions on this codebase: finer folders
kill the `compile/` junk-drawer but raise crossings. The table above is the
finer option. The coarse alternative (keep prolog as a single root plus base
`compile/` with only `registry`/`parse_dl` moved out) holds cross edges at ~the
current number at the cost of an unlabeled 27-file flat root. The owner's goal
1 is served identically by either; the report prefers the finer tiers to give a
first-time reader phase labels.

---

## 7. Disagreements with the prior verdict, with receipts

1. **Scope.** The verdict's rename table covered 62 files across `dl`(16) /
   `prolog`(27) / `tsv2`(19). This brief excludes `v6/dl` and adds the 41 tsv2
   tests, so the counts here (46 core + 41 tests = 87) are not comparable
   column-for-column. `dl` is a separate connected component; the verdict's
   "three packages are disconnected" claim holds unchanged at this scope.

2. **The prolog folder as a single weak bag.** The verdict graded the pre-flatten
   `v6/prolog` at ratio 0.243 and the whole thing as "not one thing." Agreed.
   Where I extend it: after the flatten, the residual defect is not a cycle (0
   SCCs confirmed) but the unlabeled flat root plus the `registry`/`parse_dl`
   reach-in. The verdict's own preferred fix ("flatten `compile/` into
   `v6/prolog`", 9 moves, crossing 44->27) leaves those two files unattached in
   a stub `compile/`. This report finishes that by placing them into a first-read
   `substrate/` tier rather than leaving them as reach-ins. That is a
   disagreement in degree, not direction.

3. **Depth+indegree as the tie-break.** The verdict picked depth then in-degree
   for prefix order, and its prefix was a dense rank of depth within a folder.
   This report drops depth as a driver: once folders are contiguous blocks of
   one valid topo, in-degree (read the most-depended-on first) is the only
   tie-break that matters for intra-block order, and depth adds nothing to the
   read-before-use guarantee. The verdict's metric-blind finding ("prefixes
   encode reading order, depth is only one input") supports dropping depth from
   the driver; this report makes that consequential.

4. **Numbering span.** The verdict renumbered within the existing 2-folder
   prolog (prefix reused across `prolog/` and `prolog/compile/`). This report
   prefixes per-folder (unique within folder) over a 7-folder prolog, which
   keeps prefixes short and folders unambiguous. No duplicate prefix within any
   proposed folder; the global index 1..87 is the single linear reading order.

Agreement (no receipts needed, restated): 0 file-granularity cycles; `registry`
is the top fan-in file (13 dependents, confirmed); the `analyze.pl`/`lower.pl`
fan (Louvain puts `analyze` with its readers, and `analyze`/`lower` still sit at
the highest crossing point of the pipeline) is the same "highest value facade"
spot the verdict named.
