# Finite semantic coverage, 2026-09-08

This inventory is the enumerated coverage target for this lab. It does not
certify arbitrary SQL, arbitrary DD user operators, or all combinations of cells.

Evidence labels: **E** executed in this lane; **R** executable plugin rejection;
**D** documented library capability, not an executed plugin/parity claim;
**C** composition inferred from available library operators, not executed here;
**N** no admitted API/contract in the compared arm. Plugin means the actual loaded
25_sqlite_ivm extension. The preserved template arm has no general SQL installer;
its E cells refer only to its emitted fixture. It retains affected-group refresh.

Sources inspected:

- **DD-C**: pinned differential-dataflow 0.25.1
  [Collection source](https://docs.rs/crate/differential-dataflow/0.25.1/source/src/collection.rs):
  map/filter/concat/negate/consolidate/distinct/reduce/join/semijoin/antijoin.
  `semijoin` multiplies RHS weights; relational presence needs RHS distinct.
  `antijoin` subtracts semijoin, so the same RHS presence restriction applies.
- **DD-W**: [difference.rs](https://docs.rs/crate/differential-dataflow/0.25.1/source/src/difference.rs)
  Semigroup/Monoid/Abelian/Multiply. Addition/negation and zero detection are
  explicit requirements; raw signed input can represent more than SQL row bags.
- **DD-I**: [iterate.rs](https://docs.rs/crate/differential-dataflow/0.25.1/source/src/operators/iterate.rs)
  Iterate/Variable and nested timestamps. Its documentation requires
  consolidation on iterative paths to prevent circulating cancelling updates.
- **DD-T**: [multitemporal example](https://docs.rs/crate/differential-dataflow/0.25.1/source/examples/multitemporal.rs),
  unordered input capabilities, paired timestamp frontier and trace compaction.
  Source inspected; this example was not executed. The benchmark uses one
  monotone u64 sequence and waits on a probe, with no partially ordered input API.
- **PG**: pg_ivm [v1.15 README](https://raw.githubusercontent.com/sraoss/pg_ivm/v1.15/README.md),
  supported view definitions/restrictions and concurrency sections. D below
  records documented support only; the shared join is the executed PG query.
- **SQL**: SQLite [trigger](https://sqlite.org/lang_createtrigger.html),
  [conflict](https://sqlite.org/lang_conflict.html),
  [loadable extension](https://sqlite.org/loadext.html) public contracts.
- **LOCAL**: sprefa-store/src/engine.rs SQL refcount/frontier routines and
  prolog/lower.pl avg_delta_rows_sql / avg_accumulator_update_sql, source inspected.
  The existing signed AVG path has one positive source; the join extension uses
  a lab-local SQL lowering. No kernel/compiler changes or dd-runner design input.

Executable identifiers:

- **S**: 9_crossover_workload.mjs `makeCrossoverFixture(400,10,10,true)`;
  171 states in `results/plugin-20260908/semantic.jsonl` (final binary rerun recorded separately), five arms including
  both logging modes, 855 exact input/output validations. Base rows, summary
  multiplicities, hashes and independent recomputation are checked each state.
- **P01..P23**: `27_sqlite_ivm.test.py`, method test_01 through test_23.
  P02 checks 240 seeded two-sided bag transitions (seeds 7,42,2026).
  P08 has 32 named query rejection subcases; rejection is never counted as parity.
- **T**: `20_sqlite_template.test.py` six tests of existing emitted maintenance.
- **M**: `17_sqlite_trigger_capabilities.py` ten stock SQLite mechanism tests.
- **J**: `23_crossover.test.mjs` five shared fixture/engine/report tests.
  Includes removed-DD-write and removed-plugin-SQL injected failures.
- **O**: `sprefa-store/tests/oracle_dd.rs`: test_dd_simple_reach,
  test_dd_add_edge, test_dd_del_edge, test_dd_batch_updates.
- **L**: `sprefa-store/tests/datalog_ops.rs`: two_way_join, union_distinct,
  antijoin_negation, aggregation_group_count, three_way_join,
  recursion_transitive_closure. These execute store SQL reference behavior;
  they are not SQLite extension acceptance or cross-engine parity.
- Upstream inventory, source only: DD tests/join.rs (join_scale_1 onward),
  tests/reduce.rs, tests/reduce_reference.rs, tests/scc.rs, tests/bfs.rs,
  tests/trace.rs. They were not run wholesale, including their large scales.

| ID / semantic family | DD library and actual DD arm | pg_ivm | Template arm | Loaded plugin / concrete evidence |
|---|---|---|---|---|
| 01 Projection / map | DD-C D; fixture map E S | D; E S expression | E S expression | E SUM column/product P11; standalone projection R P08/projection |
| 02 Filter | DD-C D | D | N general | R P08/filter |
| 03 Multiset duplicate supports | DD-W/joins D; equal projected facts E S | E S | E S | E two-sided duplicates P01/P02/P11; S semantic_duplicate_supports/retract |
| 04 Set / DISTINCT | DD-C distinct D | D | keyed input set E S | keyed rows E S; DISTINCT R P08/distinct/count_distinct |
| 05 Signed multiplicity / consolidation | DD-W/C D; +/- keyed old/new E S | internal E S transitions | staged signs E T/S | arithmetic +/- contributions E P01/P02; no arbitrary signed-row SQL input API |
| 06 Inner equijoin | DD-C D; E S | E S | E S | E P01/P11/S, both ON and USING bound to catalog |
| 07 Self join | DD-C C (reuse collection) | D | N | R P08/self |
| 08 Multiway join | DD-C C (composed joins) | D | N | R P08/multiway; L/three_way_join is reference-only |
| 09 Left/right/full outer join | DD-C C (matched + presence/absence branches) | D restricted | N | R P08/left/right/full |
| 10 Semijoin / EXISTS | DD-C D with RHS presence restriction | D restricted | N | R P08/semi |
| 11 Antijoin / negation | DD-C D with RHS presence restriction | N not exercised | N | R P08/anti; L/antijoin_negation reference-only |
| 12 UNION ALL / UNION | concat / concat+distinct D/C | N | N | R P08/union_all/union; L/union_distinct reference-only |
| 13 EXCEPT / INTERSECT | DD-C C presence/threshold composition | N | N | R P08/except/intersect |
| 14 COUNT | CountTotal tuple count E S | E S | E S | E COUNT(*) P01/S; COUNT DISTINCT R P08/count_distinct |
| 15 SUM | DD-W explode tuple E S | E S | E S | E signed integer SUM P01/P07/P17/S |
| 16 AVG | DD-W count+sum C | D | N this plan | R P08/avg |
| 17 MIN / MAX | DD-C reduce D/C | D | N this plan | R P08/min/max |
| 18 Grouped / empty grouping | DD count E S | E S | E S | E P01/P17/S empty_facts/empty_dimensions/reseed |
| 19 Global / empty global aggregate | unit-key reduction + explicit empty row C | D | N this plan | R P08/global; no synthetic empty aggregate row lowering |
| 20 NULL keys/values/all-NULL SUM | Option-valued SQL semantics need explicit graph C | SQL semantics D | R non-null schema T | R NULL source writes P07/P21; no SQL NULL algebra admitted |
| 21 Subqueries | operator graph composition C | D restricted | N | R P08/subquery |
| 22 CTEs | reusable graph composition C | D restricted | N | R P08/cte |
| 23 Recursion / fixpoints | DD-I D; O executes reach changes | N | N | R P08/recursive; L recursion is store reference-only |
| 24 Cyclic deletion | DD-I algorithm/consolidation dependent; upstream scc/bfs inventory | N | N | R recursive query P08; no recursive lowering or cycle deletion claim |
| 25 Ordering | DD-C reduce sees ordered value supports D; SQL row order not collection order | N | N | R P08/order |
| 26 Top-k | DD-C per-key ordered reduction C | N | N | R P08/topk |
| 27 Windows | DD-C custom operator composition C, no SQL window frontend | N | N | R P08/window |
| 28 DD time/frontiers/iteration | DD-T/I D; one u64/probe regime E S | N DD API | N DD API | N DD API; SQL transactions are not arbitrary DD time parity |
| 29 Transaction / savepoint | no SQL rollback in DD arm | E S transactions | E T/M | E P03/P04/P09/P10/P13; base/support/counters rollback together |
| 30 Durability / reopen | volatile DD arm, N persistence | durable configured E S | E T/S reopen | E P05/S reopen; crash/power-loss injection untested |
| 31 Concurrent connections | one DD worker E S | server E S; competing-writer test not run | E T/M | E P05 lock contention/WAL visibility; P06 required writer pragmas |
| 32 Types / bounds / collations | fixture bounded integers E S; generic type depends on operator | bounded fixture E S | non-null integers E T/S | E affinity-converted integers P07; R float/text/blob/NULL/bounds P07/P17, collation/generated catalog P12 |
| 33 Schema / install/drop ownership | N SQL installer | E shared create | host installer E T | E P09/P10/P11/P22; R foreign-key cascades P20; R collisions/shadows/unmanaged triggers P09/P18; public result edits R P16 |
| 34 Fault detection | E J removed keyed write | exact state S | exact state T/S | E J removed SQL write; P15 deliberate accumulator corruption is detected |
| 35 Telemetry boundaries | current arm JSONL stage/probe receipts | existing case/server receipts | stage receipts | E P13 counters/rollback/second writer; P14 CLI scoped logging/rejection/off; P19 closed sink; P23 host trace retained |

Missing lowering algorithms, corresponding to R/N rows:

- General projection/filter/map needs bound expression typing, predicate evaluation
  on both row images, and output support multiplicities. Only a column or product
  of two columns is admitted inside SUM. Parenthesized/general expressions,
  aggregate ordering, HAVING, casts and nondeterministic functions have no lowering.
- DISTINCT/count-distinct require keyed value support counts and zero-crossing
  output updates. UNION/EXCEPT/INTERSECT need branch-specific weighted support
  combination with SQL set/bag rules.
- Self/multiway joins need alias-specific change scheduling and cross-term
  accounting when one base write appears in several logical inputs. Outer joins
  need match counts and NULL-padded rows at support zero crossings. Semi/antijoin
  need right-key presence thresholds; arbitrary signed weights alone are insufficient.
- AVG needs separate sum/non-NULL count and output division. MIN/MAX need ordered
  value support maintenance and replacement extrema on deletion. Global aggregates
  need an explicit empty-input output row. NULL support needs nullable join/group
  equivalence, aggregate non-NULL support counts and all-NULL SUM restoration.
- Subqueries/CTEs need scoped relation binding and dependency scheduling.
  Recursive SQL needs iteration/fixpoint scheduling plus a reviewed deletion
  algorithm for cycles. No new language/kernel semantics were added for these.
- Ordering/top-k/windows need ordered partitions, ties/frame rules and retractions.
  Arbitrary DD times need a timestamp/frontier API and time-indexed state, absent
  from the SQL plugin. The u64 fixture does not establish that contract.
- Administrative schema changes, reserved internal-table edits, C APIs disabling
  triggers, function replacement and blob writes require an ownership/authorizer
  contract beyond ordinary persistent SQL triggers. Public result writes fail;
  deliberate internal corruption is an injected detector test, not supported DML.

Every engine comparison here is restricted to the common S fixture. Library D/C
cells and successful rejection tests are reported separately from the 855 shared
semantic validations. Build/test receipts are under results/plugin-20260908/;
final-gate-2/status.txt is written only after the complete gate succeeds.
