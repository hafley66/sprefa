# MAP backlinks — source sessions for major v6 decisions

Every major decision in `v6/MAP.md` and `v6/DECISIONS.md`, traced to its source session(s).
All citations are reproducible rg hits.

| decision | source path:line | snippet |
|---|---|---|
| **The unification: salsa/dd/graph-algos = one semi-naive cascade** | `v6/DECISIONS.md:19` | `ONE semi-naive cascade:  frontier → one hop → prune → fixpoint` |
| **Unification prune test varies by role (digest/weight/reached)** | `v6/DECISIONS.md:21–24` | `The ONLY thing that varies is the prune predicate ... A · control (salsa) → prune by **digest** ... B · facts (dd/feldera) → prune by **weight ≠ 0** ... C · reach (SCC/blast) → prune by **reached**` |
| **Unification lab-verified** | `v6/plans/2026-07-21-v6-lab-arc-oracles-and-measured-perf.md:15–28` | `Layer 2 CONTROL = salsa (red-green). Layer 1 FACTS = append-only bitemporal table on SQLite. Retract = close interval. One monotonic `revision` counter serves dirty-check (salsa), retraction (weight→0), and temporal as-of` |
| **Counting-not-DRed: retraction = weight-based, no separate code path** | `v6/plans/2026-07-19-v6-table-design.md:344–352` | `Retraction stops being a separate code path. A delta is a set of `(row, ±weight)` pairs... Recursion is handled without DRed. A tuple derived by three rules carries weight 3; killing one derivation leaves weight 2` |
| **SCC handles cycles via nested fixpoint, not DRed** | `v6/plans/2026-07-19-v6-table-design.md:356–363` | `a cyclic derivation (a tuple that participates in deriving itself) makes naive weights diverge. The fix is... run the recursive group's fixpoint as a nested loop that reaches a least fixed point before its deltas are published outward` |
| **DD/Salsa are teachers (oracles), not shipped** | `v6/DECISIONS.md:26–32` | `They are the **yardstick** (dd: proves the O(Δ) floor) and the **blueprint** (salsa: the red-green mechanism we re-express in SQL). Shipping either means shipping their resident-RAM model — the exact v5 36 GB-swap death v6 exists to kill` |
| **Boolean-bit weight rejected; integer count chosen** | `v6/DECISIONS.md:49–50` | `Weight is INTEGER support-count; `weight>0` = alive. Boolean-bit REJECTED` |
| **The 7-function covering set** | `v6/MAP.md:48–81` | `reaches_from · reached_by · multi_source_walk · multi_source_halt_bfs · tarjan · build_condensed · count_pairs` |
| **Coverage mapped to SQLite shipping vehicles** | `v6/MAP.md:67–71` | `ships in SQLite: recursive CTE (6/7 reach+walk) · counting Z-set retract (weight = #supports, delete-at-0) · SCC nested fixpoint` |
| **Recursive CTE win** | `v6/MAP.md:156–162` | `recursive CTE reachability **WIN** — 6/7 functions, 0 deps` |
| **Counting Z-set win** | `v6/MAP.md:158–159` | `counting Z-set (weight=support) **WIN** — retract w/o DRed` |
| **SCC nested fixpoint win** | `v6/MAP.md:159–160` | `SCC nested fixpoint in SQL **WIN** — retract_scc beats DRed 6%` |
| **DRed as recursive CTE: loss** | `v6/MAP.md:163` | `DRed as recursive CTE **LOSS** — 20% slower than the loop` |
| **Boolean-bit weight: loss** | `v6/MAP.md:164` | `boolean-bit weight **LOSS** — rejected, integer count wins` |
| **Broad low-selectivity autoindex: loss** | `v6/MAP.md:165` | `broad low-selectivity autoindex **LOSS** — loses to value skew` |
| **SCC DAG early-out: open** | `v6/MAP.md:166` | `SCC DAG early-out **OPEN** — 4.4x counting on acyclic cuts` |
| **petgraph as teacher** | `v6/MAP.md:172` | `petgraph **TEACHER** — Csr good, algos need own-storage; 112 B/node resident` |
| **Salsa and dd roles in the unification** | `chat_log/20260722.1.v6-store-hermetic-harness-counting-decision-pin-session-digest.md` | `It is ALL counting. One semi-naive cascade, prune = digest(salsa)/weight(dd,Z-set)/reached(SCC)` |

## How to extend this table

For any new major decision added to v6/MAP.md or v6/DECISIONS.md, run:

```bash
# Find the decision's source session(s)
rg -i "DECISION TEXT HERE" chat_log/*.md plans/ v6/plans/ v6/*.md

# Then extract and cite the exact path:line and snippet
rg -A3 -B1 "matching line" <source-file>
```

All citations must be real rg hits. Do not invent paths or snippets.
