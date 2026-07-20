# v6 graph layer: measurements, and one agent's unreliable reading of them

**Read the health warning in
`~/projects/claude-research/commands/graph-libs/STATE.md` before the D-G
sections below.** The recommendation in this document changed five times in the
session that produced it. The numbers are reproducible; the D-G decisions are an
inference on top of them and carry no more weight than the reader's own.

The genuinely stable results, held across all four adversarial rounds:

| result | verification |
|---|---|
| `scc.rs` + `walk.rs` are correct | exact set equality against independently written Tarjan on 6/6 real relations, 9 adversarial graphs, 0/400 differential fuzz |
| CTEs reproduce `walk.rs` semantics | 12/12 tests plus 5 edge cases |
| reverse traversal needs a reverse index | 4,000x, `AUTOMATIC COVERING INDEX` in the plan, reproduced in every round |
| `rel_df_edge` is unrepresentative | 1 of 18 relations, friendliest by a wide margin |

Everything past that table moved between rounds.

---


Date: 2026-07-20
Research: `~/projects/claude-research/commands/graph-libs/STATE.md` (read-first),
plus per-library writeups and eight lab crates.

Three rounds ran: sonnet, then adversarial opus, then adversarial opus against
real data. Rounds two and three each overturned the round before them. Every
number below comes from an executed lab against the live sprefa database or
from reading vendored source.

## D-G1: adopt no graph library

petgraph, ultragraph, GraphBLAS, CozoDB all rejected. Reasons per library are in
the writeups; the decisive measurement is that with storage held constant
sprefa's own `tarjan` beats `petgraph::kosaraju_scc` by 1.16x to 1.43x, and the
apparent petgraph win in round two was a storage confound (`Vec<Vec<u32>>`
against `DualCsr`, difference credited to the library).

| graph | tarjan / `Vec<Vec<u32>>` | tarjan / `DualCsr` | kosaraju / `DualCsr` |
|---|---|---|---|
| `rel_df_edge` | 19.5ms | 9.9ms | 14.2ms |
| `rel_flow_edge` | 18.3ms | 9.9ms | 15.0ms |
| synth 1M/2M | 196.8ms | 102.7ms | 119.6ms |

LadybugDB, the last candidate and the only one advertising out-of-core graph
algorithms, was measured and rejected. Its algorithm state comes from the buffer
pool and cannot spill, so a 150M-node graph needs 16-24 GB of buffer pool on a
16 GB machine. Its fast `scc` is **silently wrong** (effective `maxIterations`
20 against a documented 100, non-convergence returns a partial partition with no
error). Its correct `scc_ko` runs 6.3x slower than plain in-memory Tarjan. It
ingests at ~200 edges/sec incrementally, so a full rebuild wins past 43 edges,
which ends any story for a reactive engine.

**No option exists that runs compiled graph algorithms over non-resident data.**
The search is closed rather than unfinished.

## D-G2: keep `scc.rs` and `walk.rs` exactly as they are

**The cheapest option is the one already merged.** 431 lines, correct, and its
SCC partition was verified by exact set equality against an independently
written Tarjan on 6 of 6 real relations plus 9 adversarial graphs. Every library
measured is slower, wrong, or both.

Nothing here is worth rewriting, and the earlier draft of this document framed
CSR adoption as a project. It is not one. Two receipts against doing it:

- `tarjan`'s six callers (`typecheck.rs:1174`, `typed_plan.rs:402`,
  `strata.rs:499`, `strata.rs:570`, `derive.rs:2084`, `derive.rs:2187`) all run
  on RULE graphs, which are small. The measured 2x was on data graphs that no
  caller passes.
- The self-loop defect reported by the red team is not in sprefa.
  `scc.rs:83-84` already sets `cyclic` for a self-edge. The SQL forward-backward
  implementation was the one that got it wrong, and D-G5 drops that.

CSR remains the correct storage IF a resident snapshot is ever built for a hot
saved rel. That is a D9 tier question with a measured decision rule (D-G3), and
it stays unbuilt until a relation actually earns it.

Sizing, corrected for v5 reality then for v6:

| | cost |
|---|---|
| v5 measured | `16V + 8E`, because node ids are `i64` symbol hashes at density ~3e-14 and need a dictionary |
| v5 corpus extrapolation | 3.44GB at 130M edges |
| **v6 with D1 surrogate keys** | `8E` plus dense offsets, dictionary term deleted by construction |

D1 (surrogate integer keys, no hashed ids) pays for itself here. Whoever costs
the v6 graph layer uses the v6 row, and the v5 numbers are the ceiling we are
leaving behind.

Build once per `(family, rev)` from the sorted stream. Never build-then-compress:
that pattern peaked at 1.85x final size in the ultragraph lab and is structural
to the shape rather than a defect in that crate.

## D-G3: tier selection keys on mean reachable-set size, not graph size

The build-vs-CTE crossover measured **1.5 to 297 queries**, and it tracks mean
reach. `rel_df_edge` and `rel_flow_edge` are near-identical in edge count with
crossovers 198x apart.

So the D9 tier decision (transient / saved / cold) cannot be made from table
statistics. It needs a reach estimate per relation.

| relation shape | tier |
|---|---|
| mean reach small (`rel_df_edge`, 5.35) | CTE, build almost never pays |
| mean reach large (`rel_flow_edge`, 15,653) | snapshot after ~2 queries |
| queried once | CTE always |
| SCC / condensation / count_pairs | snapshot always |

Open: where the reach estimate comes from. A sampled probe at rel-creation time
is the cheap answer, and it is unbuilt.

## D-G4: SQL recursive CTEs cover traversal, with three named limits

Expressiveness is settled: 12 of 12 `walk.rs` cases reproduce, plus 5 edge
cases, plus 0/400 differential fuzz mismatches. The halt predicate is a `WHERE`
clause on the recursive term.

Limits that the red team established, each of which was hidden by measuring only
`rel_df_edge` (1 of 18 non-empty edge relations, and the friendliest):

1. **Multi-seed loses the index seek.** Seeding from a bound literal seeks;
   seeding from a seed TABLE, which is the only shape `multi_source_walk` has,
   plans as a full scan. 7.02ms against 0.24ms. Any CTE lowering has to handle
   the seed set shape deliberately.
2. **Cost tracks reached EDGES, not reached nodes.** Same 300-node reach costs
   0.31ms at out-degree 3 and 5.99ms at out-degree 299, identical plan. Bimodal
   in practice: 18 of 40 random `rel_flow_edge` seeds land at 105-193ms with
   nothing between 0.6ms and 100ms.
3. **A reverse-column index is mandatory**, 4,000x without one.

## D-G5: SCC stays in Rust, on CSR

SQL forward-backward SCC is correct and is **quadratic in peel-core size**, fit
holding across 40x with under 4% residuals. `rel_flow_edge` did not complete in
25 minutes. Dead at target scale.

`count_pairs` likewise: the published ~4s was a `rel_df_edge` number whose
stated precondition (mean reach 6.5) is violated by the largest relation in the
same database. `rel_flow_edge` extrapolates to 4.3 hours.

Both stay in `sprefa-graph` as Rust over CSR slices.

## D-G6: the depth premise was wrong, and stack safety is a non-issue here

The brief said "assume worst-case path depth 1,000,000". Measured max DFS depth
is **690**, against petgraph's recursive overflow threshold of 65,420. A 95x
margin.

| relation | max eccentricity | mean reach |
|---|---|---|
| `rel_df_edge` | 34 | 5.35 |
| `rel_flow_edge` | >=112, p99 79 | 15,653 |

Consequence for the engine: `rel_flow_edge`'s p99 eccentricity of 79 sits above
the depth cap of 64, so a capped walk silently truncates on the largest
relation. That is a correctness question, not a performance one.

## Immediate work: four DDL statements and one decision

**No Rust changes.** The entire actionable output of four research rounds is
index DDL plus one policy call. Verified against the live DB 2026-07-20: the
three relations below carry only their PK autoindex.

| # | change | receipt | size |
|---|---|---|---|
| 1 | reverse index on `rel_map_edge` | 139,709 rows, forward-only index, 4,000x cliff on reverse traversal | 1 DDL |
| 2 | reverse index on `rel_bom_edge` | 25,314 rows, same | 1 DDL |
| 3 | reverse index on `rel_port_edge` | 22,154 rows, same | 1 DDL |
| 4 | drop `idx_df_edge_from` | duplicates the PK prefix; the forward plan never uses it | 1 DDL |
| 5 | depth cap 64 against `rel_flow_edge` p99 eccentricity 79 | capped walks silently truncate on the largest relation. Correctness, not performance | POLICY |

Deliberately NOT doing, each with its reason:

| skipped | why |
|---|---|
| `tarjan` onto CSR | 2x on graphs no caller passes; all six callers use rule graphs |
| self-loop fix | already correct at `scc.rs:83-84` |
| adopt any graph library | all measured slower, wrong, or non-viable |
| SQL SCC | quadratic in peel-core size, did not finish in 25 min on `rel_flow_edge` |
| build a resident snapshot tier | no relation has earned it yet; D-G3 gives the rule when one does |

One follow-on worth a read rather than a change: dc9b67b1's index-demand policy
should keep reverse-column indexes on traversed relations and drop PK-prefix
duplicates. Both rules came out of this research and neither was known when that
policy merged.

## Method law earned by this arc

Three rounds, three reversals, same error each time:

| round | stopped at | cost |
|---|---|---|
| 1 | the first compile error (`E0277` against a concrete type) | missed that petgraph's algorithms are generic over public traits |
| 2 | the first warm-cache measurement | reported a size penalty that a pragma erases |
| 3 | one relation out of eighteen | every performance generalization broke |

**A sample that looks complete is the failure mode.** Adding to the standing
laws: a measurement over one instance of a family is a hypothesis about the
family. Name the sample in the writeup, or the number will be read as general.

## Still open

- Reach estimation for D-G3 tier selection.
- `rel_flow_edge`'s 22,257-node SCC rests on one Tarjan implementation; no SQL
  cross-check completed.
- `v6-deps` claims about sea-query, rmcp, tower-lsp-server, tracing, from the
  same pass that produced the wrong petgraph note. Never revisited.
- 4 queued labs: neo4j-graph, rustworkx, igraph (license-gated), cpp trio.
