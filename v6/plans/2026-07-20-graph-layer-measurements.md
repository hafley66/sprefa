# v6 graph layer: measurements

Date: 2026-07-20
Labs: `~/projects/claude-research/labs/graph-*` (8 crates)
Writeups: `~/projects/claude-research/commands/graph-libs/`

**This file contains no recommendations.** An earlier version
(`2026-07-20-graph-layer-decision.md`) carried six decisions written by an
agent. Those were removed. The architecture call is the user's; this is input.

## What the benchmarks actually compared

Stated up front because it bounds everything below. Each library was used by an
agent for hours, on first contact. sprefa's own code has been iterated in
production. Four headline results turned out to be misuse rather than library
behavior:

| reported | actual cause |
|---|---|
| petgraph 1.47x faster, then 1.16-1.43x slower | benchmarked `tarjan(&Vec<Vec<u32>>)` against `kosaraju(&DualCsr)`; different storage |
| GraphBLAS transpose 12-23x | missing `GrB_Matrix_wait` in the harness; real 1.21x, 0.99x stored `BY_COL` |
| GraphBLAS 90x cliff at 8+ sources | descriptor form re-transposes every step; pre-materializing is 3.9x |
| LadybugDB `scc` 65% wrong | run on the effective default `maxIterations` of 20; the documented 100 returns correct |

Each was found by one later round happening to dig there. Treat every remaining
library number as possibly the same shape.

## sprefa's existing code

`src/graph/scc.rs` 180 lines, `src/graph/walk.rs` 251 lines.

| property | verification |
|---|---|
| SCC partition correct | exact set equality against an independently written Tarjan, 6/6 real relations, 9 adversarial graphs |
| walk semantics correct | 12/12 `walk.rs` tests, 5 edge cases, 0/400 differential fuzz against CTE reproductions |
| self-loop handling | `scc.rs:83-84` sets `cyclic` for a self-edge |

`tarjan` callers, all on rule graphs: `typecheck.rs:1174`, `typed_plan.rs:402`,
`strata.rs:499`, `strata.rs:570`. `build_condensed` callers: `derive.rs:2084`,
`derive.rs:2187`.

## The graph data

18 non-empty edge relations. `rel_df_edge` is the friendliest and was the only
one measured in the first three rounds.

| relation | edges | max SCC | mean reach | max eccentricity |
|---|---|---|---|---|
| `rel_df_edge` | 261,704 | 6 | 5.35 | 34 |
| `rel_flow_edge` | 371,404 | 22,257 | 15,653 | >=112, p99 79 |
| `rel_map_edge` | 139,709 | | | |
| `rel_bom_edge` | 25,314 | | | |
| `rel_port_edge` | 22,154 | | | |

Max DFS depth measured: 690. Node ids are `i64` symbol hashes, density ~3e-14.

## Index state, verified against the live DB 2026-07-20

| relation | indexes |
|---|---|
| `rel_df_edge` | `idx_df_edge_from`, `idx_df_edge_to`, PK autoindex |
| `rel_map_edge` | PK autoindex only |
| `rel_bom_edge` | PK autoindex only |
| `rel_port_edge` | PK autoindex only |

Reverse traversal with no index on the reverse column: 162.4ms, plan shows
`SEARCH e USING AUTOMATIC COVERING INDEX (to=?)`. With the index: 0.04ms.
Reproduced in every round.

`idx_df_edge_from` duplicates the PK prefix; the forward plan uses the PK.

## SQL recursive CTE numbers

All against real relations unless marked synthetic.

| measurement | value |
|---|---|
| 300-node frontier, 1M-edge synthetic | 0.28ms |
| 300-node frontier, 10M-edge synthetic | 0.29ms |
| 3,000-node frontier, 1M / 10M | 4.15ms / 4.71ms |
| same 300-node reach, out-degree 3 vs 299 | 0.31ms vs 5.99ms, identical plan |
| 40 random `rel_flow_edge` seeds | 18 reach 85,766 nodes at 105-193ms; bimodal, nothing between 0.6ms and 100ms |
| seed from a bound literal vs a seed TABLE | 0.24ms vs 7.02ms; the table form plans as a scan |
| cold cache | 2.5-5.2x |
| under a concurrent writer, WAL | 3.1x median |
| `walk.rs` reproduction | 12/12 + 5 edge cases + 0/400 fuzz |
| forward-backward SCC | quadratic in peel-core size, fit across 40x with <4% residuals; `rel_flow_edge` did not complete in 25 min |
| `count_pairs`, `rel_df_edge` | ~4s |
| `count_pairs`, `rel_flow_edge` | extrapolates to 4.3 hours |

`PRAGMA cache_size=-64000` removes an apparent table-size penalty. sprefa caps
`cache_size` at 16MB, `temp_store=FILE`, asserted at `src/db.rs:1540`.

## Library numbers

Read with the misuse warning above.

| library | measurement |
|---|---|
| petgraph | 58 lines of trait impls compile `algo` against a custom structure |
| petgraph | `kosaraju_scc` partition identical to `tarjan`, 6/6 real relations |
| petgraph | SCC 14.2ms vs `tarjan` 9.9ms on the same `DualCsr` |
| petgraph | `tarjan_scc`, `depth_first_search`, `is_cyclic_directed` are recursive; overflow ~65,420 nodes |
| petgraph | `Dfs` / `DfsPostOrder` walkers iterative, 1M-deep path 6ms / 11ms |
| petgraph | `EdgeFiltered::from_fn(g, \|e\| !halt[e.source()])` matches `walk.rs` halt semantics |
| petgraph | memory `16V + 8E` on v5 ids; 3.44GB at 130M edges |
| petgraph | `kosaraju_scc` state 40.00 B/node vs `tarjan` 4.00 |
| petgraph | ships `dominators`, `all_simple_paths`, `feedback_arc_set`, `bridges` |
| petgraph | `condensation` takes a concrete `Graph` by value, not generic |
| GraphBLAS | `LAGraph_scc` correct; 0.12s at depth 250, 3.28s at 1,000, 158.4s at 4,000 |
| GraphBLAS | 283k-node random graph 0.15s |
| GraphBLAS | `LAGr_ConnectedComponents` depth-insensitive, 1M-deep path 0.265s |
| GraphBLAS | own shim 443 lines, 56 declarations; build 4.16s on system libs |
| ultragraph | `freeze()` peaks at 1.85x final size, stable across 10x |
| LadybugDB | algorithm state from the buffer pool, cannot spill; 109-164 B/vertex |
| LadybugDB | `scc_ko` hash-exact vs `tarjan`, 6.3x slower than in-memory tarjan at 10M edges |
| LadybugDB | ~200 edges/sec incremental vs 2.9M/sec bulk; full rebuild wins past 43 edges |
| build cost | resident snapshot pays for itself between 1.5 and 297 queries, tracking mean reach |

## Prior art

46 logged queries. No published SQL SCC found: zero on Stack Overflow,
dba.stackexchange, GitHub repo search, DuckDB/DuckPGQ docs, or the recursive-CTE
literature. Vertica's survey (arXiv:1412.5263) omits SCC. DuckDB `USING KEY`
SIGMOD 2025 and Hirn & Grust CIDR 2023 score 0 on "SCC" / "Tarjan" / "strongly
connected". Soufflé runs Gabow on the rule graph in C++ for stratification,
never exposed. Zero petgraph-over-disk prior art.

Both prior-art passes ran after the session's WebSearch budget was exhausted,
falling back to `lite.duckduckgo.com`, the Stack Exchange API, and the GitHub
API. 3 CAPTCHA, 2 empty.

## Open, unmeasured

- `rel_flow_edge`'s 22,257-node SCC rests on one Tarjan implementation
- no SCC timing for `rel_flow_edge` in SQL
- reach estimation mechanism for tier selection
- `v6-deps` claims about sea-query, rmcp, tower-lsp-server, tracing
- 4 unrun labs: neo4j-graph, rustworkx, igraph, cpp trio
