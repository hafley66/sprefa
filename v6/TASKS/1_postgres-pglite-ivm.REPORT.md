# PostgreSQL and PGlite IVM shootout

Run date: 2026-09-07. Branch: `feature/postgres-pglite-ivm`.

## Result

Four arms completed the smoke and bounded scale profiles:

| arm | evaluator | incremental SQL maintenance | storage and transport |
|---|---|---|---|
| `native-query` | native PostgreSQL 18.6 ordinary views | none; each snapshot executes the full view query | disposable cluster, dedicated databases, private Unix socket |
| `native-pg_ivm` | native PostgreSQL 18.6 | pg_ivm 1.15 immediate trigger maintenance | disposable cluster, dedicated databases, private Unix socket |
| `pglite-query` | PGlite 0.5.8, embedded PostgreSQL 18.3 ordinary views | none; each snapshot executes the full view query | disposable NodeFS directory, in-process WASM |
| `pglite-pg_ivm` | PGlite 0.5.8, embedded PostgreSQL 18.3 | `@electric-sql/pglite-pg_ivm` 0.0.9 containing pg_ivm 1.13 | disposable NodeFS directory, in-process WASM |

The full run ended with `run-done: ok`. It checked 54 distinct measured output keys across arms. Its 672 mutation records are `ok`; 96 recursive full-query records are `ok`; and 96 recursive pg_ivm records are explicitly `unsupported`. The smoke run ended with `run-done: ok` and checked 18 output keys.

These measurements do not establish a performance ranking. Native PostgreSQL and PGlite use different PostgreSQL and pg_ivm versions, durability settings, process boundaries, and memory accounting scopes.

## Existing harnesses and reuse

The recursive [exec shootout contract](../v6/labs/exec_shootout/CONTRACT.md) requires semi-naive evaluation and disqualifies naive re-derivation. Ordinary recursive SQL is therefore recorded in the separate `recursive-full-query` category. No PostgreSQL arm was registered as an exec shootout engine and its contract was not weakened.

The old [tick benchmark](../v6/labs/exec_shootout/tick_bench/TICKS.md) rewrites the cumulative input and starts a new engine process for every tick. Its published numbers are process-per-tick full recomputation. They are retained as such and were not combined with the SQL IVM results.

The broader [sprefa-store scale runner](../v6/sprefa-store/bench/run.sh) provides the default `2x200`, `6x2000`, `8x20000`, `10x50000`, and `14x80000` ladder. This lab reuses its first three total row scales: 400, 12,000, and 160,000. Each row scale is paired with two update-batch sizes. The 500,000 and 1,120,000 row cases are explicit skipped receipt rows in the bounded laptop profile.

The [v7 runtime shootout](../v7/labs/18_runtime_shootout/0_README.md) supplies chain and ring definitions and the N=48 full-run size. This lab uses the same exact derived counts for a separately labeled ordinary recursive SQL check: chain has 1,128 pairs and ring has 2,304 pairs. Smoke uses N=16.

The new nonrecursive fixture uses the existing receipt conventions of deterministic generation, warmup and measured repetitions, exact oracles, phase timing, bounded execution, JSONL output, and explicit status rows. It is shared unchanged by all four SQL arms.

## Workload and correctness

The base tables are `dimension(group_id, factor)` and `fact(id, group_id, amount)`. Each arm maintains or executes both of these queries:

1. A join from fact to dimension, grouped by `group_id`, with `count(*)` and `sum(amount * factor)`.
2. `SELECT DISTINCT group_id, amount FROM fact`.

The ordered state sequence is:

1. Initial state.
2. Batched fact inserts.
3. Batched fact deletes.
4. Batched fact updates that change the join key and amount.
5. One dimension update with fact-table fanout.
6. Deletion of one copy of a duplicated `(group_id, amount)` pair.
7. Deletion of the second copy.

After every state, the runner materializes both SQL results into temporary snapshots, transfers every ordered row, computes a canonical SHA-256 over the complete rows, and compares the checksum and both result cardinalities with an independent JavaScript `Map` and `Set` oracle. The fixture label records seed `0x6d65726375727901`; row generation and update selection are deterministic formulas.

| family | native query | native pg_ivm | PGlite query | PGlite pg_ivm |
|---|---:|---:|---:|---:|
| joined grouped count and sum | full query | supported | full query | supported |
| fact insert batch | supported | supported | supported | supported |
| fact delete batch | supported | supported | supported | supported |
| fact update batch | supported | supported | supported | supported |
| dimension fanout update | supported | supported | supported | supported |
| duplicate support through DISTINCT | full query | supported | full query | supported |
| recursive chain and ring | separate full query | unsupported | separate full query | unsupported |

The pg_ivm arms create automatic unique indexes on the grouped and distinct immediate materialized views. The ordinary-query arms have only the fact primary key plus the shared `fact(group_id)` and `fact(group_id, amount)` indexes. Every setup receipt records the observed index definitions.

## Timing boundaries

All values use `process.hrtime.bigint()` in the Node process. The full profile runs one discarded warmup and three measured repetitions. Each case and repetition uses a fresh Node process. Each PGlite process uses a fresh directory. Native cases reset the schema in dedicated databases inside one disposable cluster.

| field | boundary |
|---|---|
| setup | schema creation, initial load, shared indexes, and view or IMMV construction, recorded as separate subfields and summed below |
| update transaction | `BEGIN` through `COMMIT`; pg_ivm trigger maintenance occurs here |
| query/readback | materialize both current views into temporary snapshot tables; ordinary-view full recomputation occurs here |
| client transfer | ordered `SELECT` of every snapshot row into Node |
| checksum | canonical row serialization and SHA-256 in Node |
| case wall | all seven validations and six updates after setup, including snapshot cleanup and memory sampling |
| process wall | Node process start through exit, including setup, the nonrecursive case, recursive category, database open, and database close |

The table gives medians across three measured repetitions. The update, query, transfer, and checksum columns sum the six post-initial states. `RSS KiB` is the maximum observed across those measured repetitions. PGlite RSS covers the Node process containing JavaScript and PostgreSQL WASM. Native RSS is shown as `backend/client`; backend RSS can include shared pages.

| rows | batch | arm | setup ms | update ms | query ms | transfer ms | checksum ms | case wall ms | process wall ms | RSS KiB |
|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 400 | 1 | native-pg_ivm | 8.906 | 4.700 | 4.890 | 4.394 | 0.439 | 35.287 | 88.700 | 20,208 / 60,224 |
| 400 | 1 | native-query | 5.297 | 1.464 | 7.290 | 4.157 | 0.472 | 36.526 | 96.058 | 19,424 / 60,112 |
| 400 | 1 | pglite-pg_ivm | 27.883 | 11.447 | 9.811 | 9.687 | 0.441 | 47.139 | 1,104.609 | 1,333,968 |
| 400 | 1 | pglite-query | 13.936 | 3.349 | 11.796 | 9.360 | 0.454 | 42.278 | 1,095.517 | 1,362,384 |
| 400 | 10 | native-pg_ivm | 9.637 | 7.048 | 6.471 | 3.487 | 0.465 | 40.752 | 97.246 | 23,216 / 62,208 |
| 400 | 10 | native-query | 5.945 | 2.040 | 7.126 | 3.486 | 0.492 | 37.149 | 97.491 | 19,744 / 61,344 |
| 400 | 10 | pglite-pg_ivm | 27.563 | 14.483 | 12.593 | 9.522 | 0.465 | 54.609 | 1,270.166 | 1,332,960 |
| 400 | 10 | pglite-query | 13.393 | 4.261 | 13.161 | 10.329 | 0.497 | 48.426 | 1,152.052 | 1,369,584 |
| 12,000 | 10 | native-pg_ivm | 56.466 | 7.089 | 13.503 | 35.090 | 7.807 | 160.361 | 262.876 | 33,232 / 93,040 |
| 12,000 | 10 | native-query | 42.369 | 2.636 | 33.739 | 34.704 | 8.355 | 180.781 | 277.654 | 31,152 / 95,968 |
| 12,000 | 10 | pglite-pg_ivm | 122.169 | 17.442 | 26.239 | 133.703 | 8.125 | 288.625 | 1,467.104 | 1,349,744 |
| 12,000 | 10 | pglite-query | 87.075 | 4.302 | 65.847 | 133.345 | 7.416 | 325.215 | 1,434.982 | 1,339,584 |
| 12,000 | 100 | native-pg_ivm | 55.467 | 17.811 | 14.602 | 34.962 | 8.627 | 175.242 | 275.348 | 33,920 / 96,768 |
| 12,000 | 100 | native-query | 45.317 | 3.453 | 32.755 | 35.288 | 8.280 | 180.821 | 289.827 | 30,016 / 98,432 |
| 12,000 | 100 | pglite-pg_ivm | 120.022 | 35.603 | 27.286 | 131.762 | 8.455 | 307.523 | 1,495.113 | 1,358,112 |
| 12,000 | 100 | pglite-query | 88.234 | 5.997 | 66.609 | 131.391 | 7.320 | 325.103 | 1,472.736 | 1,358,560 |
| 160,000 | 100 | native-pg_ivm | 690.051 | 31.591 | 25.520 | 55.207 | 12.484 | 444.753 | 1,202.912 | 71,152 / 122,864 |
| 160,000 | 100 | native-query | 578.893 | 3.158 | 231.778 | 49.738 | 11.124 | 599.294 | 1,267.922 | 70,176 / 120,880 |
| 160,000 | 100 | pglite-pg_ivm | 1,125.498 | 44.386 | 36.921 | 197.909 | 14.541 | 592.093 | 2,857.615 | 1,345,184 |
| 160,000 | 100 | pglite-query | 966.142 | 7.871 | 494.851 | 194.523 | 14.957 | 1,106.965 | 3,263.491 | 1,366,864 |
| 160,000 | 1,000 | native-pg_ivm | 846.587 | 142.908 | 24.753 | 62.664 | 14.184 | 551.695 | 1,480.294 | 72,352 / 145,216 |
| 160,000 | 1,000 | native-query | 622.258 | 12.696 | 246.861 | 58.725 | 13.166 | 643.269 | 1,409.451 | 66,624 / 145,872 |
| 160,000 | 1,000 | pglite-pg_ivm | 1,216.893 | 284.618 | 63.482 | 236.609 | 19.004 | 926.386 | 3,397.885 | 1,376,672 |
| 160,000 | 1,000 | pglite-query | 957.717 | 23.143 | 500.927 | 207.124 | 16.409 | 1,139.428 | 3,258.109 | 1,329,424 |

The complete phase table is [results/scale-summary.tsv](../v6/labs/exec_shootout/postgres_pglite_ivm/results/scale-summary.tsv). Raw records are [results/scale.jsonl](../v6/labs/exec_shootout/postgres_pglite_ivm/results/scale.jsonl) and [results/smoke.jsonl](../v6/labs/exec_shootout/postgres_pglite_ivm/results/smoke.jsonl).

## Recursive full-query category

The following medians each aggregate 18 measured records: three repetitions repeated alongside six nonrecursive size and batch cases. Both ordinary-query arms returned the exact oracle checksum on every record.

| arm | family | N | derived | query/readback median ms | SHA-256 |
|---|---|---:|---:|---:|---|
| `native-query` | chain | 48 | 1,128 | 0.730 | `c457f22deded5af96c08f6fe5d3c60882da7bf7fa004a3ebda14f8d2c303b875` |
| `native-query` | ring | 48 | 2,304 | 1.049 | `280398239665ca49cd9c6a227a0c02308ec409a0af1983c6697e7420d28e8394` |
| `pglite-query` | chain | 48 | 1,128 | 2.067 | `c457f22deded5af96c08f6fe5d3c60882da7bf7fa004a3ebda14f8d2c303b875` |
| `pglite-query` | ring | 48 | 2,304 | 2.082 | `280398239665ca49cd9c6a227a0c02308ec409a0af1983c6697e7420d28e8394` |

pg_ivm rejects these `WITH RECURSIVE` definitions, so both pg_ivm arms emit `unsupported` records rather than timing values.

## Environment, durability, and bounds

| field | recorded value |
|---|---|
| machine | Mac14,10, Apple M2 Pro, arm64, 16 GiB |
| OS | macOS 14.6.1, build 23G93 |
| Node | 24.15.0; npm and npx 11.12.1 |
| initial native inventory | no `postgres`, `pg_config`, or pg_ivm on PATH |
| task-local native build | PostgreSQL 18.6; pg_ivm 1.15 at commit `377a37dc72a922486d9d3d0c8caf2f7e91900c93` |
| task-local JS packages | PGlite 0.5.8; PGlite pg_ivm package 0.0.9; `pg` 8.16.3 |
| PGlite runtime probe | embedded PostgreSQL 18.3; `CREATE EXTENSION pg_ivm` succeeds; pg_ivm 1.13 |
| native durability | `fsync=on`, `synchronous_commit=on`, `full_page_writes=on` |
| PGlite durability | NodeFS; runtime reports `fsync=off`, `synchronous_commit=on`, `full_page_writes=on` |
| timeout | 120 seconds per case process and native `statement_timeout=120000` |
| Node bounds | 2,048 MiB old-space limit; 3,072 MiB process RSS watchdog |
| native bounds | one client per case; 128 MiB shared buffers; 16 MiB work memory; 2,048 MiB temporary file limit |

PostgreSQL 18.6 came from the [official source directory](https://www.postgresql.org/ftp/source/v18.6/) with the published SHA-256 verified before extraction. The release is listed in the [PostgreSQL 18.6 announcement](https://www.postgresql.org/about/news/postgresql-186-1711-1615-1519-1424-and-19-beta-3-released-3365/). pg_ivm 1.15 came from its [official release tag](https://github.com/sraoss/pg_ivm/releases/tag/v1.15).

The [pg_ivm README](https://github.com/sraoss/pg_ivm/blob/main/README.md) documents immediate trigger maintenance, supported joins and aggregates, DISTINCT multiplicity handling, automatic indexes, and unsupported query forms. The [PGlite extension catalog](https://pglite.dev/extensions/#pg_ivm) identifies `@electric-sql/pglite-pg_ivm` and its load procedure. The [PGlite filesystem documentation](https://pglite.dev/docs/filesystems) documents NodeFS directory persistence.

## Skipped and unsupported cases

| case | status | reason |
|---|---|---|
| 500,000 rows, batch 1,000 | skipped | outside the bounded laptop profile after the first three shared store-rig row scales |
| 1,120,000 rows, batch 1,000 | skipped | outside the bounded laptop profile after the first three shared store-rig row scales |
| recursive pg_ivm chain and ring | unsupported | pg_ivm rejects `WITH RECURSIVE` view definitions |
| PGlite `live.changes` and `live.incrementalQuery` | skipped | optional category; the [live query documentation](https://pglite.dev/docs/live-queries) states that queries rerun and incrementalQuery diffs results through a temporary table, so this is not an incremental SQL evaluation arm |

No case recorded timeout, memory-limit, error, or mismatch status. The full profile stopped at 160,000 rows under its declared bounded ladder. The observed PGlite process peak at executed sizes was 1,376,672 KiB.

## Receipts and rerun commands

SHA-256 receipt hashes:

```text
27ad9db78ee9fad82971967dadbf830a6878235298cd48e5f0d7d06f5f2a4486  results/scale.jsonl
5b5fde64ec37c992629fb8323ecd584be9e120efdb54b4ffe32eed75f2b15c15  results/smoke.jsonl
d435ab1e38f99a269cdfd2d7879eced3e26686cc72f30edb16767a2f058ba32a  results/scale-summary.tsv
```

From the repository root:

```bash
cd v6/labs/exec_shootout/postgres_pglite_ivm
npm ci
node 0_probe_pglite.mjs
./1_prepare_native.sh
./7_run.sh smoke results/smoke.jsonl
./7_run.sh full results/scale.jsonl
node 8_summarize.mjs results/scale.jsonl results/scale-summary.tsv
```

After task-local dependencies are present, the opt-in entrypoints are:

```bash
just -f v6/justfile postgres-pglite-ivm-smoke
just -f v6/justfile postgres-pglite-ivm
```

Normal recipes remain dependency-free. The smoke recipe is a deterministic correctness gate. Existing CI workflows were not changed and no CI coverage was removed. Current local execution passed the load probe, JavaScript syntax checks, shell syntax checks, dependency inventory, opt-in four-arm smoke, and full profile.

## Commits

| commit | content |
|---|---|
| `063d5f813` | isolated provisioner, shared workload and oracle, four execution arms, bounded runner, summarizer, and opt-in just recipes |
| `eda98b99c` | raw smoke and scale receipts, generated phase summary, and this report |

No compiler kernel, compiler semantics, call-site migration, production database configuration, shared daemon, system service, or primary checkout was changed.
