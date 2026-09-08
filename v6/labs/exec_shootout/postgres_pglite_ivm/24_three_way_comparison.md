# Shared pg_ivm / SQLite / native DD crossover

2026-09-08. Integration commit `13044d4c3`, on `feature/postgres-ivm-crossover` in `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-ivm-crossover`.

All three arms now execute in the existing [`12_crossover_runner.mjs`](12_crossover_runner.mjs), on the same nonrecursive fact/dimension inner join with grouped COUNT and SUM(amount * factor). This pass measured 45 matched triples: nine fixture cells, five measured repetitions per arm, plus one discarded warmup per cell/arm. Every input and output state matched exactly.

[Three-way chart](results/three-way-20260908/three-way.svg) · [Exact denominators, ranges and hashes](results/three-way-20260908/three-way.tsv) · [Per-mutation timing](results/three-way-20260908/families.tsv) · [Raw repeated receipts](results/three-way-20260908/repeated.jsonl) · [Semantic receipts](results/three-way-20260908/semantic.jsonl)

## Measured rows

Values are median milliseconds for four mutation transactions plus result materialization and count. Each cell has five successful, exact-state-matched triples. Min/max values and paired ratios are in the TSV and chart. Warmups and initial setup are excluded.

| Rows | Batch | Dimension fanout | pg_ivm | SQLite affected-group | Native DD |
|---:|---:|---:|---:|---:|---:|
| 400 | 10 | 10 | 7.021 | 1.963 | 0.132 |
| 12,000 | 10 | 10 | 9.796 | 4.100 | 0.298 |
| 400 | 10 | 200 | 6.269 | 1.906 | 0.142 |
| 1,200 | 10 | 200 | 7.265 | 2.626 | 0.155 |
| 4,000 | 10 | 200 | 8.599 | 3.399 | 0.207 |
| 12,000 | 10 | 200 | 9.576 | 4.431 | 0.274 |
| 12,000 | 1,000 | 200 | 23.434 | 177.213 | 1.140 |
| 12,000 | 1 | 200 | 9.815 | 2.660 | 0.274 |
| 12,000 | 100 | 200 | 13.119 | 21.441 | 0.431 |

At 12,000 rows and fanout 200, the measured SQLite total rises from 4.431 ms for batch 10 to 177.213 ms for batch 1,000; pg_ivm rises from 9.576 to 23.434 ms. SQLite's transport refreshes affected groups per source row. DD's benchmark API accepts in-process keyed writes and provides no durable commit. These measurements do not establish an equal-durability engine ranking or general-query performance.

## Executed implementations and timer boundary

| Arm | Actual implementation | Measured work and scope |
|---|---|---|
| pg_ivm | Native PostgreSQL 18.6, pg_ivm 1.15; `pgivm.create_immv` in [`10_crossover_case.mjs`](10_crossover_case.mjs) | Node sends BEGIN, ordinary DML and COMMIT over a private Unix socket; extension triggers maintain the result; CREATE TEMP TABLE materializes it and SELECT count(*) counts it. Includes command/client-server latency. fsync, synchronous_commit and full_page_writes enabled. |
| sqlite-template-group | Existing Prolog-emitted SQL, installed by [`19_sqlite_template_adapter.py`](19_sqlite_template_adapter.py) as persistent stock-SQLite 3.53.2 row triggers | Python submits BEGIN IMMEDIATE, ordinary DML and COMMIT; SQLite executes affected-group recomputation; temporary snapshot materialization and count. On-disk WAL, synchronous FULL. Every benchmark case also passes fresh-reopen validation. |
| native-dd | [`22_crossover_dd.rs`](22_crossover_dd.rs), built using existing sprefa-store DD 0.25.1 / timely 0.31 dependencies | One worker; keyed-map old-row lookup, signed InputSession updates, arranged join, tuple-weight CountTotal, probe/frontier completion, copying maintained output into a snapshot and counting it. Volatile memory, no WAL or reopen guarantee. |

The DD dataflow is `facts.map(group, amount).join(dimensions).explode(group, (1, amount*factor)).count_total()`. The tuple is the additive difference: first component accumulates COUNT, second accumulates SUM. Both retractions and additions flow through DD operators. The operator's maintained count/sum output is captured as signed output rows; no expected summary is supplied to the graph. The independent recomputation is only used after the measured interval to check that output.

Fixture construction and payload parsing are outside the write timer for all arms. DD receives normalized keyed puts/deletes, looks up the existing row inside the timer and retracts it before inserting its replacement. SQL arms execute their row selection and expressions as DML. This API/work difference remains explicit. No-op keyed writes can be omitted during normalization; semantic tests validate them but the four timed crossover mutations actually change their target rows.

Actual input collections are captured in DD and compared row-for-row with the fixture after frontier completion. SQL source tables are fetched and compared outside the timer. The summary is also compared exactly, with canonical input and output SHA256 stored per phase. Sorting, full client result/source transfer, hashing and oracle checks are outside the measured write/materialize/count interval. Source capture instrumentation inside DD adds timed work and memory; its overhead has not been isolated.

The keyed inputs are bounded, non-null integers. PG inputs use 32-bit integer columns; all tested aggregate values fit signed 64-bit arithmetic. Nullable SQL values, unbounded integer/overflow behavior, arbitrary bag inputs, recursive queries and general subqueries are not certified by this fixture. PG has the fixture's source foreign key; extended semantic operations preserve it. SQLite's generated storage and DD have no general foreign-key enforcement API.

## Validation and failures

- Shared semantic run: 163 states per arm, 489 exact input/output checks. Includes the five crossover states, eight directed states (delete/reinsert, primary-key and group move, zero SUM with positive COUNT, last-group removal, dimension delete/reinsert), then 150 seeded updates across seeds 7, 42 and 2026. All three arms matched.
- Repeated run: 135 measured engine cases and 27 warmup cases, 810 total state checks; 675 are measured-state checks. Exactly 45 successful triples. Zero engine errors, mismatches, timeouts or resource-blocked cases.
- Current local tests: `23_crossover.test.mjs` 3/3, `20_sqlite_template.test.py` 6/6, `17_sqlite_trigger_capabilities.py` 10/10. Total 19/19 tests pass. The new DD test also deliberately removes one keyed input write and confirms nonzero exit on exact-input mismatch. This intentional fault is excluded from benchmark success counts.
- Initial DD compilation used `insert/remove` on `InputSession<..., i64>` and failed with six E0599 errors: those convenience methods are implemented for isize differences. The lab now uses `update(row, +1/-1)` for its i64 differences. Release compilation succeeded in 7.10 s. No dependency or production algorithm changed to resolve this.
- Existing compiler/SQLite tests and semantic receipts remain preserved. The new test is executable through Node's test runner; no CI workflow was changed to schedule it automatically.

Engine order within measured rounds rotates `[pg_ivm, SQLite, DD]`, `[SQLite, DD, pg_ivm]`, `[DD, pg_ivm, SQLite]`, then repeats. The five rounds are not perfectly position-balanced because five is not divisible by three. All three warmups finish before measured rounds for each cell. Each case uses a new DD process or SQLite database; the private PG server persists across the run while each case gets a fresh schema. Host/cache effects remain possible.

## Memory, storage and execution budget

One sequential repeated run took 21.899 s runner wall; the separate three-way semantic run took 1.240 s. Per-case timeout is 120 s; overall harness deadline is 20 minutes. No 160k case was run. Host free-memory percentage was checked between cases: 42–46% during repeats. The harness stops remaining cases below 15%. Swap was already allocated before the run; recorded usage changed from 4497.38 MiB to 4489.38 MiB. No memory-pressure stop occurred.

| Arm | Measured setup range, ms | Observed memory | Recorded database bytes |
|---|---:|---|---:|
| pg_ivm | 7.107–63.929 | Maximum sampled postmaster-tree RSS 71,328 KiB; Node client RSS and backend memory contexts are recorded separately per state | 8,009,407–11,794,111; includes PG database overhead. Cluster WAL reported separately |
| SQLite | 2.384–22.102 | Process peak 49,283,072 bytes including Python, fixture/oracle and SQLite; sampled process maximum 47,616 KiB | 139,264–598,016; main database after close/checkpoint |
| DD | 0.264–3.351 | Process peak 26,443,776 bytes including fixture, keyed maps, observed input traces, DD and oracle; short process lifetimes yielded no successful periodic RSS sample, so samples remain null | 0; volatile state. Fixture and log storage excluded |

PG settings: shared_buffers 32 MiB, work_mem 1 MiB, effective_cache_size 64 MiB, maintenance_work_mem 32 MiB, temp_file_limit 2 GiB. These are tuning settings, not a total memory cap. No arm has enforced total-process memory limits. Process RSS scopes differ and must not be compared as engine-only heap sizes.

## Reproduction and artifacts

From the worktree, build the benchmark example with the existing locked dependencies:

```sh
CARGO_TARGET_DIR="$PWD/v6/sprefa-store/target" \
cargo build --offline --release --manifest-path v6/sprefa-store/Cargo.toml --example crossover_dd
```

From the lab directory, choose a fresh output filename and use the existing cluster runner:

```sh
IVM_BUDGETS=constrained bash 13_crossover_run.sh full results/three-way-new.jsonl \
  --arms pg_ivm,sqlite-template-group,dd \
  --sqlite-program "$PWD/results/template-reuse-20260908/18_crossover.rs" \
  --max-rows 12000 --warmups 1 --repetitions 5
```

Use profile `semantic` for the 163-state sequence. The script refuses an existing output receipt. The JSONL records its exact command and cluster settings. `14_crossover_summarize.mjs` accepts a fifth positional output for the three-way TSV; `15_crossover_heatmap.mjs --three-way` renders the matched median/range chart. The legacy full-query/pg_ivm pair table is intentionally unmeasured in this pass because the full-query arm was not requested.

All source and receipts are in the isolated worktree, not the main checkout. Numeric reading order is shared fixture `9`, SQL case/adapter `10–11`, runner `12–13`, reports `14–15`, SQLite compiled fixture/adapter/tests `18–20`, native DD example `22`, integration tests `23`, this report `24`.

The DD binary is `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-ivm-crossover/v6/sprefa-store/target/release/examples/crossover_dd`; it is a local benchmark executable, not a SQLite extension or installed application component. It reads a per-case fixture and writes JSONL. The SQLite adapter reads the retained compiler-emitted `18_crossover.rs` JSON and persists tables/triggers/results in each per-case `maintained.sqlite`. No main DL7 app emitter runs IVM maintenance in this integration.

`results/three-way-20260908/repeated.artifacts/` retains exact per-case fixtures, stdout/stderr, SQLite DDL/triggers and databases, the PG server log and the runner's budget-part JSONL. Its generated footprint is 127 MiB. Adjacent SQLite WAL/SHM may exist while a database is live; closed benchmark databases are retained for reopen. The private `/tmp/pgx.*` clusters were stopped and removed by the existing cleanup protocol; their logs and metric receipts persist. DD's in-memory state ends with each process. No source dependency was installed, no system service changed, and no repository was pushed or merged.

Execution provenance SHA256:

```text
DD executable: 051f41f2429ab69bb309d61258020b4b49629ab31c4696217d97c16c820f3adf
DD source:     256a2233d24d2512e4204a8637a1df3e81a0046be05f54ac7801aecc8c7857df
Cargo.lock:    61c7e422f506378114d861613d4ef5602cb35353da3356b0b7353463caa0d5d9
SQLite plan:   98f4b91f6da96dfa52b943baa3421d44d274375a300fbe4d20c280e9bab82825
```

Remaining SQLite interface work is unchanged: no public SQL SELECT installer, loadable extension binary or general SQL/subquery acceptance boundary exists yet. The stock-SQLite arm uses compiler-emitted affected-group maintenance for this declared fixture, with the writer settings and transaction limitations documented in [`21_sqlite_template_reuse.md`](21_sqlite_template_reuse.md). The comparison tests this executable slice without extending that claim.
