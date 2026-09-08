# Existing compiler SQL on stock SQLite

2026-09-08. Worktree: `/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/postgres-ivm-crossover`.

Executed result: the existing Prolog compiler's join/count/sum maintenance SQL runs in persistent stock-SQLite triggers. Ordinary INSERT, UPDATE and DELETE automatically maintain an ordinary result table, including after reopening and from a second configured connection. No TS or Rust runtime executes during these writes. The installer is a lab Python consumer of the existing emitted plan. A SQL-facing `install_view(name, SELECT...)` function, SQL parser binding, and loadable extension binary remain unimplemented.

Maintenance class: affected-group recomputation, once per changed source row. It deletes and rebuilds affected groups from current sources. This differs in work performed from arithmetic aggregate deltas. No compiler or production runtime algorithm was changed.

## Reading order and exact reuse boundary

Paths below are relative to this worktree. The source audit inspected the listed symbols and surrounding implementations, not every line of each source file.

| Source | Input and output; executed consumer |
|---|---|
| `v6/prolog/compile.pl:660`, `compile_dl6/3`; `:839`, `compile_program_phases` | DL6 source, output path, emitter options -> expanded/checked program -> lowered plan -> output. The entrypoint consumes DL6. |
| `v6/prolog/lower.pl:529`, `compile_positive_uses`; `:771`, `compile_negative_uses` | Relation uses and bound variables -> SQL aliases, equality joins, filters; negation becomes NOT EXISTS. |
| `v6/prolog/lower.pl:4517`, `level_statement_groups`; `:4602`, `level_aggregate_sql/5` | Relation plans, head and rules -> `aggsql(scope_columns, scope_types, clear, seeds, delete, inserts, intern)`. This is the reused aggregate producer. |
| `v6/prolog/lower.pl:4940`, `aggregate_scope_seed_sql/6`; `:4998`, `aggregate_delete_scoped_sql/5`; `:5016`, `aggregate_insert_scoped_sql/7` | Source delta rows -> affected grouping keys -> scoped delete -> original grouped join constrained to those keys. |
| `v6/prolog/emit_ts.pl:1721`, `aggregate_sql_text`; `v6/prolog/emit_rust.pl:365`, `aggregate_field` | Same `aggsql` fields -> TS plan object or Rust `PROGRAM_JSON`; `delta_maintained=false`. Rust `emit_program` at `:598` serializes the full plan. |
| `v6/tsv2/runtime/1_incremental.ts:378`, `apply_aggregate_level_statement` | Executes scope, delete and insert; consumes RETURNING rows into signed downstream events and sequences. |
| `v6/sprefa-engine-rs/src/incremental.rs:1881`, `apply_aggregate_level_statement` | Executes the same phases, collects removed/inserted rows and stages downstream deltas, including its deferred aggregate path. |
| [`18_crossover.dl6`](18_crossover.dl6) -> [`results/template-reuse-20260908/18_crossover.rs`](results/template-reuse-20260908/18_crossover.rs) | This pass actually invoked the unchanged compiler and Rust emitter. No Rust executable was needed to consume the JSON. |
| [`19_sqlite_template_adapter.py`](19_sqlite_template_adapter.py), `trigger_sql` / `install` | Existing plan -> persistent schema and row triggers. Removes the exact terminal RETURNING suffix and changes TEMP support tables to persistent tables. Preserves the aggregate query text. |

`lowered_program_data/2,3` at `lower.pl:7756` also exposes rule-write verbs. Its aggregate fallback does not carry the complete aggregate maintenance plan. This probe therefore consumes the existing emitter's full plan rather than treating that verb map as a complete aggregate interface.

The previous shootout audit inspected specialized SQLite implementations in `v6/sprefa-store/examples/perf_report.rs`, `v6/tsv2/scripts/2_p3-retract-bench.ts`, and emitted-runtime adapters. This pass additionally traced the compiler SQL producer through both emitters and both runtime consumers into the persistent-trigger transport above. `v6/dd-runner` was excluded from design input.

## Concrete source, SQL and update timeline

The existing crossover query joins keyed `fact(id, group_id, amount)` and `dimension(group_id, factor)`, grouped by group_id, computing count and sum(amount * factor). [`9_crossover_workload.mjs`](9_crossover_workload.mjs) supplies the same deterministic rows, mutations and exact summary hash used by the native PG/pg_ivm crossover. The DL6 input is:

```text
rel fact(id: int, group_id: int, amount: int) key(1).
rel dimension(group_id: int, factor: int) key(1).
summary(Group, count(Id), sum(Amount * Factor)) <-
    fact(Id, Group, Amount), dimension(Group, Factor).
? summary(Group, Count, WeightedSum).
```

Example compiler-emitted delete and insert, formatted for reading; the trigger transport only strips RETURNING:

```sql
DELETE FROM "18_crossover_summary"
WHERE ("group") IN (SELECT "group" FROM "__agg_scope_18_crossover_summary")
RETURNING "group", "col2", "col3";

INSERT OR IGNORE INTO "18_crossover_summary" ("group", "col2", "col3")
SELECT b0."group_id", count(*), sum((b0."amount" * b1."factor"))
FROM "18_crossover_fact_332750f6839d" b0,
     "18_crossover_dimension_7021c74db03d" b1
WHERE b1."group_id" = b0."group_id"
  AND (b0."group_id") IN (SELECT "group" FROM "__agg_scope_18_crossover_summary")
GROUP BY b0."group_id" HAVING count(*) > 0
RETURNING "group", "col2", "col3";
```

Each source has BEFORE guards and AFTER maintenance for INSERT, DELETE and UPDATE. An UPDATE stages OLD with sign -1 and NEW with sign +1. The scope seeds copy grouping keys from those rows. A group-key change refreshes both old and new groups. The trigger clears support rows before and after maintenance, inside the source statement's transaction.

Executed 400-row, fanout-200 shared case, group 0:

| State | Source change | Maintained `(group, count, weighted_sum)` |
|---|---|---|
| initial | 200 facts join dimension `(0,1)` | `(0,200,-300)` |
| insert_batch, delete_batch, update_batch | Existing fixture changes other groups | `(0,200,-300)` |
| dimension_fanout | factor 1 -> 4 | `(0,200,-1200)` |

No connection-local callback, explicit flush, or application maintenance loop is called on these writes. SQLite runs the trigger statements. Batches can rebuild the same group repeatedly because SQLite triggers fire per row; the existing host aggregate consumer can scope an entire staged batch.

## Contracts and limits

| Area | Source fact / executed evidence | Boundary |
|---|---|---|
| Query declaration | DL6 -> unchanged compiler -> emitted plan -> Python install | Public SQL SELECT declaration and extension install function remain missing. No SQL parser was added. |
| Tested algebra | One terminal inner equijoin, grouped COUNT/SUM of integer expression, keyed source sets | This adapter is fixture-specific; its structural checks do not certify arbitrary emitted aggregates. |
| Duplicates | Different fact IDs in a group contribute separately; keyed REPLACE/UPSERT checked | General SQL bag relation input is unimplemented. No arbitrary duplicate-row contract claimed. |
| NULL | Generated `INTEGER NOT NULL`; NULL write rejected with exact unchanged state | Compiler `count(Expr)` emits count(*) at `lower.pl:6557`; nullable SQL COUNT(expr) cannot be mapped without checking semantics. |
| Numeric types | Tested bounded integers, negative amounts/factors and zero factors | SQLite schema uses affinity, not STRICT or typeof checks. General numeric overflow, floating values and arbitrary text writes are unvalidated, not fail-closed. |
| Transactions | Ordinary mutation and maintenance share commit/rollback; nested savepoint and statement ABORT tested | Crash recovery and concurrent competing writers were not exercised. |
| Writer setup | `recursive_triggers=ON` and `trusted_schema=ON`; missing setting fails before mutation | Every connection must configure these. Trusting schema is a public-contract/security choice. This probe does not establish safe execution of untrusted database files. |
| Persistence | Persistent triggers/support and results reopen correctly; second configured connection updates correctly | Direct writes to result/support tables, dropping triggers, writable_schema and blob writes are outside the lab contract. |
| Conflicts | REPLACE, UPSERT, IGNORE and uniqueness ABORT tested | REPLACE relies on recursive delete-trigger execution; the connection guard is necessary. |
| AVG | `lower.pl:4628` emits signed sum/count state (`delta_maintained=true`); `:4740` requires exactly one positive body atom and no negation | Adapter explicitly rejects this maintenance class. Join AVG was not implemented by the probe. |
| Other aggregates | The existing affected-group SQL generator also has MIN/MAX and further aggregate expressions | This pass tested COUNT/SUM only. No blanket SQL aggregate acceptance claim. |
| Negation | Scope seeding has distinct positive and negative clauses; positive non-local grouping raises `aggregate_group_not_delta_local` | Negative scope seeding catches failures; do not infer general negation support from presence of NOT EXISTS SQL. |
| Recursion | `level_ref_count_sql` (`:5110`) and `level_dred_plan` (`:5391`) emit additional support/expansion plans | Existing runtime round/event loops remain required by those paths. This adapter rejects recursion, downstream edges, hosts, ticks and interning. Recursive reachability was not used to judge nonrecursive IVM. |

Additional SQLite-only tests delete/reinsert a dimension row and remove a group's last fact. Generated DL6 storage has no fact-to-dimension foreign key. The native PG fixture has one. These extended inner-join tests are separate from the five shared crossover states, which preserve the key relationship.

## Current execution accounting

Stock Python SQLite 3.53.2:

- `20_sqlite_template.test.py`: 6/6 pass, including 150 deterministic mutations (seeds 7, 42, 2026) with exact source dictionaries and independently recomputed summary after every update. Transaction, second-writer, conflict and rejection cases included. Last run: 0.092 s.
- `17_sqlite_trigger_capabilities.py`: 10/10 pass, 0.028 s. These are SQLite mechanism probes, not an installed third-party IVM candidate.
- Repeated shared crossover: 25 measured cases plus 5 discarded warmups, 150 exact state checks, 30 successful fresh reopens, zero failures. Runner wall: 4.680 s.
- Final smoke after explicit trusted-schema setup and guard checks: 2/2 cases, 10 exact states and 2 fresh reopens; 0.194 s runner wall. Earlier smoke receipts remain retained separately.
- Unsupported plans/settings are asserted rejection tests, not skipped successful arms. SQL-only installation, general SQL breadth, AVG and recursive trigger integration remain unimplemented/unmeasured.

The tests add executable lab coverage. No CI workflow was changed to invoke the new test automatically. No production compilation target changed.

Repeated timing below sums each case's four mutation transactions (ordinary write, maintenance and COMMIT) plus result materialization. Exact source/summary validation and SHA256 are outside timing. Initial compilation/install/load, initial query, Python startup and fresh reopen are excluded from this total and recorded separately where applicable.

| Existing fixture: rows / batch / dimension fanout | Repeats | Median ms | Min..max ms |
|---|---:|---:|---:|
| 400 / 10 / 10 | 5 | 1.970 | 1.579..2.161 |
| 12,000 / 10 / 10 | 5 | 4.515 | 4.171..5.461 |
| 400 / 10 / 200 | 5 | 2.382 | 1.928..3.211 |
| 12,000 / 10 / 200 | 5 | 4.974 | 4.276..6.071 |
| 12,000 / 1,000 / 200 | 5 | 179.735 | 177.500..181.014 |

Durability: on-disk WAL, synchronous FULL. Process peak RSS includes Python, SQLite and loaded fixture/oracle data. Database bytes are a separate metric. No enforced total-memory cap. Sandboxed process/swap sampling was unavailable and remains null. Only this opt-in SQLite arm ran in this pass, so no cross-engine order rotation or paired ranking is claimed. Existing native PG/pg_ivm/DD/SWI receipts remain untouched. The existing summarizer produced [`repeat-family.tsv`](results/template-reuse-20260908/repeat-family.tsv); its PG pair table explicitly has zero measured pairs. The PG speedup heatmap is therefore not presented as a SQLite comparison chart.

Raw receipts: [`repeats.jsonl`](results/template-reuse-20260908/repeats.jsonl), [`final-smoke.jsonl`](results/template-reuse-20260908/final-smoke.jsonl), and per-case stdout/stderr, exact fixture states, installed DDL/triggers and retained databases under the same directory.

Plain system `sqlite3` CLI 3.43.2 also reopened the retained database, performed an ordinary UPDATE, compared the stored sum with a fresh join SUM, then rolled back. Result `3.43.2|1|0`: version, exact-equality true, zero source delta rows. Its first attempt with trusted_schema disabled failed with `unsafe use of virtual table "pragma_recursive_triggers"`. This observed failure establishes the additional writer setting; it is not omitted from the success claim.

## Reproduce without replacing receipts

From the worktree, compile to a new destination (the checked-in generated program is also available):

```sh
swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
  -g "compile:compile_dl6('v6/labs/exec_shootout/postgres_pglite_ivm/18_crossover.dl6','/private/tmp/crossover-reuse-new.rs',[intern(none),emitter(emit_rust:emit_program)]),halt"
```

From the lab directory, tests use checked-in immutable input receipts and create disposable databases:

```sh
SQLITE_TEMPLATE_PROGRAM=results/template-reuse-20260908/18_crossover.rs \
SQLITE_TEMPLATE_FIXTURE=results/template-reuse-20260908/smoke/constrained-sqlite-template-group-400:10:10-measured-1/fixture.json \
python3 20_sqlite_template.test.py -v
```

Run the existing `12_crossover_runner.mjs` with a fresh `IVM_RUN_ROOT`, fresh `--output`, `--arms sqlite-template-group`, `--sqlite-program <absolute emitted .rs path>`, `--profile full --budget focused --max-rows 12000 --warmups 1 --repetitions 5`. The adapter refuses an existing DB file. Exact commands and generated-program SHA256 are in the JSONL metadata.

## Filesystem handoff and remaining integration choice

All task files are in the isolated worktree above, under `v6/labs/exec_shootout/postgres_pglite_ivm/`. The main checkout was not edited.

1. Source: `18_crossover.dl6` is read by Prolog. `19_sqlite_template_adapter.py` reads emitted JSON and installs schema/triggers. `20_sqlite_template.test.py` exercises that installer. The existing runner and oracle produce fixtures.
2. Generated artifact: `results/template-reuse-20260908/18_crossover.rs` contains `PROGRAM_JSON`. This pass reads it as data and does not compile or install a Rust library.
3. Concrete retained DB: `results/template-reuse-20260908/final-smoke/constrained-sqlite-template-group-400:10:10-measured-1/maintained.sqlite`. SQLite owns base, result, support tables and persistent trigger definitions inside this file. These survive reopen. Each benchmark case has its own database.
4. Per-case `installed.sql` records generated DDL and triggers. It excludes initial row data, boot statements and the fixture's extra group index, so it is not a complete standalone database replay. `fixture.json` holds initial and subsequent exact input/output states. `stdout.log`, `stderr.log` and parent JSONL retain execution evidence.
5. SQLite creates adjacent `maintained.sqlite-wal` and `maintained.sqlite-shm` while needed. WAL can contain uncheckpointed committed pages; SHM coordinates WAL access and is rebuildable. The first smoke database has retained sidecars from the CLI probe. The fresh final-smoke database is the simpler reopen example. Do not copy a live database while ignoring its WAL.
6. Test `TemporaryDirectory` databases are removed at test end. Python may create an ignored `__pycache__` alongside the adapter. No installed extension binary or new dependency exists.

Smallest next integration boundary: bind a reusable SQLite-compatible SQL parser's AST and the existing database catalog to the compiler's checked relation/rule plan, then expose an SQL-callable installer that installs the validated persistent trigger plan transactionally. The current compiler starts from DL6; accepting SQL cannot be claimed by renaming that entrypoint. The prior-art/parser findings remain in [`16_sqlite_ivm_capabilities.md`](16_sqlite_ivm_capabilities.md).

Choices requiring an explicit public contract before extending this slice: whether affected-group recomputation is acceptable for COUNT/SUM, which SQL expressions/types/NULL/bag behavior the compiler binding must support or reject, how existing catalog names and constraints are preserved, and whether the writer's trusted-schema requirement is acceptable. Arithmetic delta maintenance, arbitrary SQL lowering and recursive host-loop replacement were not added by this probe.
