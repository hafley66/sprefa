# shared sqlite frontier: per-relation vs shared transient tables

Measurement lab. No compiler, emitter, or runtime file changed. Every number
below comes from a command spelled in this document.

## TOC

1. [Rig](#rig)
2. [lower.pl: the per-relation transient tables today](#lowerpl-the-per-relation-transient-tables-today)
3. [Q1. Today's table bill](#q1-todays-table-bill)
4. [Q2. Tick cost, arm A vs arm B](#q2-tick-cost-arm-a-vs-arm-b)
5. [Q3. Boot cost](#q3-boot-cost)
6. [Q4. Contention, one writer, 1024 relations](#q4-contention-one-writer-1024-relations)
7. [Q5. Where time goes](#q5-where-time-goes)
8. [Plan claim vs measured](#plan-claim-vs-measured)
9. [Findings](#findings)

## Rig

| item | value |
| --- | --- |
| machine | Apple M2 Pro, `sysctl -n machdep.cpu.brand_string` |
| node | v24.15.0, `node --version` |
| SQLite driver | `@libsql/client` 0.17.4, url `:memory:`, default intMode |
| driver citation | `v6/tsv2/runtime/scratchStore.ts:14` imports `open_db`; `v6/sprefa-store/js/src/engine/lib.ts:54` is `createClient({ url })` |
| swipl | 10.0.2 arm64-darwin |
| worktree HEAD | `b7fbbf6c9cf17af1aa9759a937d1cbe817766aef` |
| rig | `v6/labs/shared_frontier/rig/*.ts`, one command `bash v6/labs/shared_frontier/run.sh` |

Arm A DDL text is copied from `lower.pl` (citations below). Arm B DDL text is
copied verbatim from the Storage section of
`plans/2026-08-19-shared-sqlite-frontier.md`. Both arms carry the same durable
typed tables and the same integer surrogate keys; only the transient state
differs.

```bash
bash v6/labs/shared_frontier/run.sh            # every table, pokeapi already compiled
COMPILE=1 bash v6/labs/shared_frontier/run.sh  # recompile pokeapi first
```

## lower.pl: the per-relation transient tables today

Every row is minted once per lowered relation. Name predicate and DDL site are
separate; both are cited.

| table | name predicate | DDL site | shape |
| --- | --- | --- | --- |
| `__delta_<rel>` | `lower.pl:216-218` | `lower.pl:6331-6333` | `CREATE TEMP TABLE ... ("_sign" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, <cols>)` |
| `__delta_<rel>` `_sign` index | | `lower.pl:6336-6338` | `CREATE INDEX ... ("_sign")` |
| `__delta_<rel>` group index | | `lower.pl:6342-6344` | `CREATE INDEX ... (<all cols>)` |
| `__frontier_<rel>` | `lower.pl:220-222` | `lower.pl:6347-6349` | `CREATE TEMP TABLE ... ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, <cols>)` |
| `__frontier_<rel>` `_phase` index | | `lower.pl:6352-6354` | `CREATE INDEX ... ("_phase")` |
| `__next_frontier_<rel>` | `lower.pl:224-226` | `lower.pl:6357-6359` | same columns as the frontier, no index (`lower.pl:6321-6322` says 0 of 747 emitted modules chose one) |
| `__departure_frontier_<rel>` | `lower.pl:239-241` | `lower.pl:6293-6302` | same columns as the frontier, emitted only for rels some rule binds with `finalize/1` |
| `__support_next_<rel>` | `lower.pl:291-293` | `lower.pl:6428-6430` | `CREATE TEMP TABLE ... (<cols>, "__refcount" INTEGER NOT NULL, PRIMARY KEY (<cols>)) WITHOUT ROWID` |
| `__new_<rel>` (arrival scratch) | `lower.pl:4711` | `lower.pl:6435-6437` | `CREATE TEMP TABLE ... (<cols>, "__refcount" INTEGER NOT NULL)` |
| `__pre_<rel>` | `lower.pl:228-230` | `lower.pl:6304-6320` | keyed `WITHOUT ROWID`, else plain |
| `__ping_/__pong_/__cone_<rel>` | `lower.pl:4826/4830/4834` | `lower.pl:6393-6397` | `CREATE TEMP TABLE ... PRIMARY KEY (<all cols>) WITHOUT ROWID` |
| `__expand_a_/__expand_b_<rel>` | `lower.pl:4788/4791` | `lower.pl:6404-6416` | same |
| durable set rel (unchanged in both arms) | | `lower.pl:992-999` | `CREATE TABLE ... ("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<key>))` |

Column defs are `column_def/4`, `lower.pl:2860-2906`; an `int` column is
`"<name>" INTEGER NOT NULL`.

## Q1. Today's table bill

Counts are read from a booted in-memory database, `sqlite_master` UNION
`sqlite_temp_master`; the transient tables are `CREATE TEMP TABLE`, so reading
`sqlite_master` alone reports zero of them.

```bash
bash v6/prolog/compile/scripts/compile_dl6.sh \
  v6/tsv2/gen/pokeapi_gen.dl6 v6/labs/shared_frontier/out/pokeapi_gen.ts
node --experimental-transform-types v6/labs/shared_frontier/rig/q1_table_bill.ts
```

### Q1a. Table bill per emitted program

| program | relations | durable tables | transient tables | transient/relation | indexes | views | CREATE statements | DDL bytes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| pokeapi_gen | 780 | 781 | 2348 | 3.01 | 2348 | 1370 | 6847 | 1682616 |
| key_last_write_wins (keyed) | 3 | 4 | 9 | 3.00 | 9 | 6 | 28 | 4977 |
| mutual_recursion_matches_oracle (recursive) | 4 | 5 | 16 | 4.00 | 14 | 2 | 37 | 5597 |
| bool_relation_negation_is_two_valued (negation) | 3 | 4 | 11 | 3.67 | 10 | 6 | 31 | 5723 |

### Q1b. Transient tables by family

| program | __delta_ | __frontier_ | __new_ | __next_frontier_ | __support_next_ |
| --- | --- | --- | --- | --- | --- |
| pokeapi_gen | 780 | 780 | 4 | 780 | 4 |
| key_last_write_wins (keyed) | 3 | 3 | 0 | 3 | 0 |
| mutual_recursion_matches_oracle (recursive) | 4 | 4 | 2 | 4 | 2 |
| bool_relation_negation_is_two_valued (negation) | 3 | 3 | 1 | 3 | 1 |

### Q1c. Emitted pokeapi_gen.ts, 6,077,435 bytes total, largest top-level sections

| section | line | bytes | share of module |
| --- | --- | --- | --- |
| `rel_catalog` | 11284 | 1,846,669 | 30.4% |
| `ddl` | 1301 | 1,723,741 | 28.4% |
| `INCREMENTAL_RELATIONS` | 21291 | 1,373,563 | 22.6% |
| `final_select` | 20508 | 411,596 | 6.8% |
| `STRUCT_TYPES` | 156 | 261,765 | 4.3% |
| `rel_physical_names` | 8935 | 91,531 | 1.5% |
| `rel_columns` | 8152 | 61,249 | 1.0% |
| `rel_declared_column_types` | 19708 | 55,308 | 0.9% |
| `rel_column_types` | 9718 | 54,766 | 0.9% |
| `rel_stored_column_types` | 10501 | 54,554 | 0.9% |
| `arrival_targets` | 20491 | 38,549 | 0.6% |
| `STRUCT_REF_COLUMNS` | 422 | 34,556 | 0.6% |

| measure | bytes |
| --- | --- |
| emitted module total | 6,077,435 |
| accounted for by top-level consts | 6,074,175 |
| dl6 source `v6/tsv2/gen/pokeapi_gen.dl6` | 42,992 |
| static runtime `v6/tsv2/runtime/*.ts` | 170,273 |
| emitted / source | 141.4x |
| emitted / static runtime | 35.7x |

### Q1d. pokeapi DDL split, and the arm-B projection

| group | statements | bytes | share |
| --- | --- | --- | --- |
| durable (typed tables, dictionaries, catalog, their indexes and views) | 2160 | 715,709 | 42.5% |
| per-relation transient (__delta_, __frontier_, __next_frontier_, __support_next_, __new_ and their indexes) | 4688 | 966,907 | 57.5% |
| total emitted DDL | 6848 | 1,682,616 | 100.0% |
| arm-B shared replacement | 2 | 416 | 0.0247% |
| arm-B projected DDL total | 2162 | 716,125 | 42.6% |

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q1_index_owner.ts
```

### Q1e. Index ownership in the pokeapi DDL, 2348 explicit CREATE INDEX statements

| index owner | statements |
| --- | --- |
| __delta_ | 1560 |
| __frontier_ | 780 |
| durable tables | 8 |

2,340 of 2,348 explicit indexes belong to a per-relation transient table. The 8
durable ones are the `<head>_zero` partial refcount index, `lower.pl:6440-6444`.

Compile wall for that first command, measured once on this worktree:

| leg | ms | note |
| --- | ---: | --- |
| parse | 6,528 | `COMPILE-TRACE program=pokeapi_gen` line |
| plan | 517,929 | |
| lower | 238 | |
| boot | 2 | |
| emit | 1,692 | |
| write | 42 | |
| total | 526,431 | `real 8m46.599s` under `time` |

## Q2. Tick cost, arm A vs arm B

200 ticks per run, 5 runs per cell, medians. One arrival per touched relation.
Per tick, per touched relation: insert the durable row, insert the frontier
row, run the frontier-to-durable join, then clear the tick's frontier. Arm A
issues one DELETE per touched relation because each relation owns a table; arm
B issues one DELETE for the tick. The statements/tick columns carry that.

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q2_tick_cost.ts
```

### Q2a. ms/tick, median of 5 runs of 200 ticks

| N | k | arm A ms/tick | arm B ms/tick | B/A | arm A stmts/tick | arm B stmts/tick | arm A worst run s | arm B worst run s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 16 | 1 (1) | 0.062 | 0.063 | 1.016 | 4 | 4 | 0.01 | 0.01 |
| 16 | N/8 (2) | 0.123 | 0.111 | 0.907 | 8 | 7 | 0.03 | 0.02 |
| 16 | N (16) | 0.909 | 0.783 | 0.861 | 64 | 49 | 0.19 | 0.16 |
| 64 | 1 (1) | 0.059 | 0.064 | 1.080 | 4 | 4 | 0.01 | 0.02 |
| 64 | N/8 (8) | 0.461 | 0.398 | 0.863 | 32 | 25 | 0.10 | 0.08 |
| 64 | N (64) | 3.635 | 3.079 | 0.847 | 256 | 193 | 0.73 | 0.64 |
| 256 | 1 (1) | 0.059 | 0.065 | 1.094 | 4 | 4 | 0.01 | 0.02 |
| 256 | N/8 (32) | 1.851 | 1.535 | 0.829 | 128 | 97 | 0.37 | 0.31 |
| 256 | N (256) | 14.604 | 12.290 | 0.842 | 1024 | 769 | 3.05 | 2.61 |
| 1024 | 1 (1) | 0.063 | 0.061 | 0.968 | 4 | 4 | 0.01 | 0.02 |
| 1024 | N/8 (128) | 7.710 | 6.274 | 0.814 | 512 | 385 | 1.56 | 1.28 |
| 1024 | N (1024) | 61.818 | 51.603 | 0.835 | 4096 | 3073 | 12.71 | 10.82 |

### Q2b. phase split of the same medians, ms/tick

| N | k | arm | insert | read | delete |
| --- | --- | --- | --- | --- | --- |
| 16 | 1 | A | 0.025 | 0.024 | 0.013 |
| 16 | 1 | B | 0.025 | 0.025 | 0.013 |
| 16 | N/8 | A | 0.053 | 0.045 | 0.025 |
| 16 | N/8 | B | 0.050 | 0.048 | 0.013 |
| 16 | N | A | 0.371 | 0.345 | 0.184 |
| 16 | N | B | 0.414 | 0.362 | 0.016 |
| 64 | 1 | A | 0.024 | 0.022 | 0.013 |
| 64 | 1 | B | 0.026 | 0.024 | 0.013 |
| 64 | N/8 | A | 0.190 | 0.168 | 0.095 |
| 64 | N/8 | B | 0.212 | 0.173 | 0.015 |
| 64 | N | A | 1.457 | 1.398 | 0.739 |
| 64 | N | B | 1.631 | 1.406 | 0.024 |
| 256 | 1 | A | 0.025 | 0.022 | 0.013 |
| 256 | 1 | B | 0.027 | 0.024 | 0.013 |
| 256 | N/8 | A | 0.790 | 0.699 | 0.367 |
| 256 | N/8 | B | 0.787 | 0.729 | 0.020 |
| 256 | N | A | 6.072 | 5.507 | 3.012 |
| 256 | N | B | 6.540 | 5.696 | 0.053 |
| 1024 | 1 | A | 0.028 | 0.022 | 0.013 |
| 1024 | 1 | B | 0.026 | 0.022 | 0.012 |
| 1024 | N/8 | A | 3.304 | 2.848 | 1.548 |
| 1024 | N/8 | B | 3.345 | 2.881 | 0.034 |
| 1024 | N | A | 25.846 | 22.976 | 12.918 |
| 1024 | N | B | 27.508 | 23.723 | 0.221 |

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q2_explain.ts
```

### Q2c. EXPLAIN QUERY PLAN, N=64, 4000 durable rows per relation, 1 frontier row per relation

#### arm A, no ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "__frontier_rel_7" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."_phase" = ?
```

```
SEARCH f USING INDEX __frontier_rel_7_phase (_phase=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### arm A, after ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "__frontier_rel_7" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."_phase" = ?
```

```
SEARCH f USING INDEX __frontier_rel_7_phase (_phase=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### arm B, no ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "frontier" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?
```

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### arm B, after ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "frontier" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?
```

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

Both arms SEARCH on both sides, with and without ANALYZE. Arm B's probe binds
`relation_id=?` only: under `PRIMARY KEY (relation_id, row_id, tick, sign)` the
`tick` predicate is not an index prefix, so it filters rows the index already
returned.

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q2d_keyorder.ts
```

### Q2d. Shared-frontier key order, 256 relations x 200 retained ticks, median of 5

| frontier PRIMARY KEY | frontier rows | reads per run | median ms | us per read |
| --- | --- | --- | --- | --- |
| B  (relation_id, row_id, tick, sign) | 51200 | 256 | 6.88 | 26.9 |
| B' (relation_id, tick, row_id, sign) | 51200 | 256 | 5.36 | 20.9 |

#### B  (relation_id, row_id, tick, sign)

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### B' (relation_id, tick, row_id, sign)

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=? AND tick=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

## Q3. Boot cost

Every CREATE the arm issues for N relations, timed, median of 5. `page_count`
is read for both databases: arm A's transient tables are TEMP, so
`PRAGMA main.page_count` shows none of them.

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q3_boot_cost.ts
```

### Q3. Boot cost, median of 5

| N | arm | CREATE statements | DDL bytes | sqlite_master + temp objects | boot ms | main page_count | temp page_count |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 16 | A | 64 | 8350 | 80 | 1.53 | 33 | 52 |
| 16 | B | 18 | 2870 | 36 | 0.46 | 33 | 5 |
| 64 | A | 256 | 33550 | 320 | 6.78 | 133 | 202 |
| 64 | B | 66 | 10262 | 132 | 1.79 | 133 | 5 |
| 256 | A | 1024 | 135130 | 1280 | 42.05 | 530 | 807 |
| 256 | B | 258 | 39986 | 516 | 9.58 | 530 | 5 |
| 1024 | A | 4096 | 542290 | 5120 | 388.87 | 2117 | 3226 |
| 1024 | B | 1026 | 159050 | 2052 | 70.05 | 2117 | 5 |

Main `page_count` is identical per N across arms: the durable tables are the
same in both, which is the control this table exists to show.

## Q4. Contention, one writer, 1024 relations

Frontier rows for 1024 relations per tick, 300 ticks, never deleted, so the
tree grows to 307,200 rows. Dispatch and btree work are separated by running
both arms at one row per statement, then letting arm B chunk 100 rows per
statement (`CHUNK_ROWS`, `v6/sprefa-store/js/src/engine/lib.ts:59`), which arm A
structurally cannot do across relations.

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q4_contention.ts
```

A, 1024 per-relation tables, 1 row/statement: 3339.7ms
B, one shared table, 1 row/statement: 3759.4ms
A, 1024 per-relation tables, chunked (cannot chunk across relations): 3332.4ms
B, one shared table, 100 rows/statement: 452.8ms
### Q4. One writer, 1024 relations x 300 ticks = 307,200 frontier rows, never deleted, median of 5

| arm | rows | statements | median ms | rows/s | worst run s |
| --- | --- | --- | --- | --- | --- |
| A, 1024 per-relation tables, 1 row/statement | 307200 | 307200 | 3339.7 | 91,986 | 3.37 |
| B, one shared table, 1 row/statement | 307200 | 307200 | 3759.4 | 81,715 | 3.84 |
| A, 1024 per-relation tables, chunked (cannot chunk across relations) | 307200 | 307200 | 3332.4 | 92,185 | 3.50 |
| B, one shared table, 100 rows/statement | 307200 | 3300 | 452.8 | 678,484 | 0.45 |

Reference ladder, `.claude/skills/sqlite-costs/SKILL.md`, in-process rusqlite on
this machine class: rowid table + UNIQUE index 1.34M rows/s; 4-col
`WITHOUT ROWID` INTEGER PK 2.9-3.3M rows/s. Every number in the table above is
below that band because the JS driver's per-statement work is in it.

## Q5. Where time goes

Worst arm-B cell (N=1024, k=N), 50 ticks, median of 5, then the same workload
under `--cpu-prof`.

```bash
node --experimental-transform-types v6/labs/shared_frontier/rig/q5_profile.ts
node --cpu-prof --cpu-prof-dir=v6/labs/shared_frontier/out/prof \
  --experimental-transform-types v6/labs/shared_frontier/rig/q5_profile.ts
node --experimental-transform-types v6/labs/shared_frontier/rig/q5_summarize.ts
```

### Q5a. Arm B, N=1024, k=N, 50 ticks, median of 5

| phase | statements | median ms | share of tick | us per statement |
| --- | --- | --- | --- | --- |
| insert (durable + frontier) | 102400 | 1294.4 | 52.3% | 12.64 |
| read (frontier join durable) | 51200 | 1171.1 | 47.3% | 22.87 |
| delete (one per tick) | 50 | 10.7 | 0.4% | 213.14 |
| total | 153650 | 2476.1 | 100.0% | 16.12 |

### Q5b. CPU profile self time, CPU.20260819.165324.63286.0.001.cpuprofile, 15076 ms sampled

| frame | self ms | share |
| --- | --- | --- |
| prepare in index.js | 3829.1 | 25.4% |
| raw in index.js | 2406.8 | 16.0% |
| run in index.js | 2285.2 | 15.2% |
| (program) in (native) | 2234.9 | 14.8% |
| columns in index.js | 1977.5 | 13.1% |
| iterate in index.js | 461.7 | 3.1% |
| (garbage collector) in (native) | 427.2 | 2.8% |
| rowFromSql in sqlite3.js | 397.0 | 2.6% |
| next in index.js | 372.0 | 2.5% |
| executeStmt in sqlite3.js | 273.6 | 1.8% |
| run in q5_profile.ts | 111.2 | 0.7% |
| safeIntegers in index.js | 84.0 | 0.6% |
| execute in sqlite3.js | 61.4 | 0.4% |
| runMicrotasks in (native) | 47.5 | 0.3% |
| valueFromSql in sqlite3.js | 20.1 | 0.1% |

## Plan claim vs measured

One row per claim in the Context and Storage sections of
`plans/2026-08-19-shared-sqlite-frontier.md`, plus the compile-time baseline the
Verification section states.

| # | plan claim | measured | verdict |
| --- | --- | --- | --- |
| C1 | the target seam is `compile.pl:701-721` | `compile_program_phases/8` is `compile.pl:671`; `emit_program/5` is `emit_ts.pl:2545` and `emit_rust.pl:519`; `compile.pl:701-721` is `write_compiled_output/2` plus `with_emit_context/3` | claim holds, citation drifted |
| C2 | emitted pokeapi module is 6,082,867 bytes from a 42,992-byte dl6 | 6,077,435 bytes from 42,992 bytes, 141.4x | -5,432 bytes, -0.09% |
| C3 | SQLite DDL section 1,725,668 bytes | `ddl` section 1,723,741 bytes; the statement text alone is 1,682,616 | -1,927 bytes |
| C4 | relation catalog 1,846,669 bytes | `rel_catalog` 1,846,669 bytes | exact |
| C5 | incremental relation plans 1,373,563 bytes | `INCREMENTAL_RELATIONS` 1,373,563 bytes | exact |
| C6 | final-select SQL 411,596 bytes | `final_select` 411,596 bytes | exact |
| C7 | static TSV2 runtime is 170,976 bytes | `v6/tsv2/runtime/*.ts` sums to 170,273 bytes | -703 bytes |
| C8 | the lowerer specializes frontier, delta, support, projection per relation | 780 `__frontier_`, 780 `__next_frontier_`, 780 `__delta_`, 4 `__support_next_`, 4 `__new_`; 2,348 transient tables and 2,348 indexes; 966,907 DDL bytes, 57.5% of emitted DDL | holds |
| V1 | baseline 4.14 s of compiler time | 526.4 s total on this worktree, 517.9 s of it the plan phase | 127x the stated baseline |
| S1 | one typed durable table per materialized relation stays | `PRAGMA main.page_count` identical arm A vs arm B at N = 16/64/256/1024 (33, 133, 530, 2117) | holds by construction |
| S2 | shared `frontier` TEMP table as written | compiles as written; 416 bytes with `support_count`, 5 temp pages at N=1024 against arm A's 3,226 | holds |
| S3 | shared `support_count` TEMP table as written | compiles as written; created at boot in every arm-B cell | holds; not written by Q2's workload, so its per-row cost is unmeasured here |
| S4 | frontier rows reference durable rows by `(relation_id, row_id)`, no JSON or BLOB payload | rig stores exactly that; the read still SEARCHes both sides | holds |
| S5 | the read is `SELECT typed.* FROM frontier f JOIN <typed> ON typed.__id = f.row_id WHERE f.relation_id=? AND f.tick=?` | SEARCH on both sides, with and without ANALYZE; `f` probes `relation_id=?` only | holds, with the key-order cost in S7 |
| S6 | durable identity remains the declared key or the all-column identity rule | not exercised: the rig writes arrivals only | unmeasured |
| S7 | frontier uniqueness is relation, row, tick, sign | that column ORDER leaves `tick` off the index prefix: 26.9 us/read against 20.9 us/read for `(relation_id, tick, row_id, sign)` at 200 retained ticks, 22% | holds as a uniqueness rule, costs 22% as a key order |
| S8 | support uniqueness is relation, row, rule | table created with that PK; no support writes in the Q2 workload | unmeasured |
| S9 | retractions address the same durable row identity as arrivals | not exercised: the rig writes `sign=1` only | unmeasured |

## Findings

| # | finding | number |
| --- | --- | --- |
| F1 | arm B is faster at every k >= N/8 | B/A 0.814-0.863; headline N=256, k=N/8: 1.851 ms/tick against 1.535 ms/tick |
| F2 | the whole arm-B win is the delete | N=1024, k=N: delete 12.918 ms/tick down to 0.221 ms/tick, 58x, k statements down to 1 |
| F3 | arm B's inserts and reads are slightly worse, not better | N=1024, k=N: insert 25.846 to 27.508 ms/tick (+6.4%), read 22.976 to 23.723 ms/tick (+3.3%) |
| F4 | at k=1 arm B loses | N=256, k=1: 0.059 ms/tick against 0.065 ms/tick, B/A 1.094 |
| F5 | boot is where the shared shape pays most | N=1024: 388.87 ms to 70.05 ms (5.6x), 542,290 DDL bytes to 159,050 (3.4x), 3,226 temp pages to 5 (645x) |
| F6 | most emitted DDL is per-relation transient | pokeapi: 966,907 of 1,682,616 DDL bytes, 57.5%, replaced by 416 bytes of shared DDL |
| F7 | the single shared btree costs a little on write, dispatch held equal | 307,200 rows: 91,986 rows/s per-relation against 81,715 rows/s shared, 11.2% |
| F8 | the shared table can chunk and the per-relation tables cannot | 92,185 rows/s against 678,484 rows/s, 7.4x, because one table takes 100 rows per statement |
| F9 | the plan's frontier PK column order is not sargable for the tick read | `(relation_id, row_id, tick, sign)` probes `relation_id=?` only; `(relation_id, tick, row_id, sign)` probes both and reads 22% faster |
| F10 | 10-second law, two cells | Q2 N=1024, k=N: worst run 12.71 s arm A, 10.82 s arm B. pokeapi compile: 526.4 s, of which 517.9 s is the plan phase |
| F11 | at this workload the JS driver, not SQLite, holds the largest single self-time frame | `prepare` 3,829 ms of 15,076 ms sampled, 25.4% |

Not measured by this lab, and named so nothing reads the tables as covering it:
retraction and `sign=-1`, support-count writes during a tick, recursion and
drain, the `__delta_`/`__next_frontier_`/`__pre_` families' own tick cost, and
Rust-side execution.
