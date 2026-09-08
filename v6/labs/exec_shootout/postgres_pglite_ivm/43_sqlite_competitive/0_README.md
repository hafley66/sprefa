# Competitive SQLite-native IVM

Take 1 and Take 2 remain immutable comparison arms. This directory copies the
Take 2 C extension as the starting point for separately built variants.

SELECT compilation reading order: `6_sql_compile/0_README.md`, pinned Cargo
manifest/lock, `6_sql_compile/1_main.rs`, then `7_compile_test.py`. The offline
compiler accepts two scalar projections over up to three INNER/theta/self-join
occurrences. It emits a versioned JSON plan for `take2_lazy(plan,'...')`.
SQLite privately binds k/v scalar expressions. The C delta executor maps each
occurrence to its source side and applies every nonempty inclusion-exclusion
mask against current source images. No full-query recomputation is used.
Projection types must be INTEGER/NULL before affinity; accumulator overflow
fails atomically. Other SELECT shapes remain explicit compiler errors.

Lazy source-view layout setup now requires `SELECT take2_prepare('result')`
after all `take2_attach` calls and before source writes. The function is
`SQLITE_DIRECTONLY`; its internal op 13 rejects ordinary caller INSERTs.
Missing setup rejects the first write. This fixes observed index omissions and
malformed-index errors when the old lazy path created source indexes inside the
first multi-row source INSERT. Setup, rollback, reopen and integrity checks are
covered. Existing receipts remain preserved, including the newly found failure.

Counter variants use the same SQL and lifecycle contracts: `take2_fused(mode)`
updates `_stats` through SQL triggers on delta writes; `take2_counted(mode)`
stores a separate `events` count in each delta row. In counted mode `_counter(n)`
holds flushed events and `_stats(n)` is a view of saturated `_counter.n +
sum(_delta.events)`. Reads remain exact before flush. Flush folds events into
`_counter` before deleting the queue, within the same SQLite transaction. Event
counts survive signed-delta cancellation and roll back with source changes.
The optional shared arms are `sqlite-competitive-fused` and
`sqlite-competitive-counted`; existing arm names retain their storage layouts.

Event-time window API: `CREATE VIRTUAL TABLE result USING
take2_counted(window,'3')`, attach one source with timestamp in k and payload in
v, then prepare its source-view layout. `_clock(epoch)` persists a watermark
initially zero. `INSERT INTO result(op,id) VALUES(12, next_watermark)` strictly
advances it within 0..1000000. Width is fixed at creation, 1..1000000. NEW event
timestamps must lie in `[watermark-width+1, watermark]`; NULL/future/expired NEW
timestamps fail. Deleting expired sources is legal. Updating an expired source
into the admitted interval adds a new active support. Pending data deltas flush
before watermark advancement; an indexed result-range deletion retracts expired
supports. Source rows remain persistent. `9_window_test.py` proves actual
expiration, lateness, rollback, conflict, reopen and second-writer behavior.
This mode uses scalar event time; it does not implement partial-order antichains
or arbitrary DD timestamp/frontier programs. It has no shared cross-engine
window performance claim.

Current SQL boundary:

```sql
SELECT take2_control('cache_on'); -- bounded 32-statement cache per vtab
BEGIN;
INSERT INTO result(op) VALUES(10); -- open batch
-- ordinary INSERT/UPDATE/DELETE source statements
INSERT INTO result(op) VALUES(11); -- consolidate and flush
SELECT * FROM result;             -- exact before COMMIT
COMMIT;
```

Open-batch reads fail. An unflushed COMMIT fails in xSync and rolls back the
source and maintained state. Opening outside an explicit transaction, nesting
batches, and flushing without an open batch fail. SQLite stores batch flags and
signed delta support rows; savepoint rollback restores them. A failed flush
remains unreadable until repaired or rolled back. Read APIs never flush silently.

`_delta(key,side,k,v,w)` consolidates bag changes; `_state` keeps source images;
`_result` is the support accumulator. Flush uses the current-base inclusion-
exclusion formula, with sign (-1)^(mask-size-1), inspected in OpenIVM join.cpp.
Self joins repeat the same delta relation for each affected occurrence.

The prepared statement cache holds at most 32 SQLite statements and their SQL
text per vtab, never source relations. Bindings are cleared after execution.
Profile counters report attempted work, not committed domain events; `_stats`
retains transaction-safe event counts. Instrumented wrappers count prepare,
step, VM and full-scan work for writes and cached per-event invariant reads.
`scalar_steps` distinguishes those reads. Batch-wide validation, scalar-domain
checks, frontier checks and the direct PRAGMA read remain outside the counters.
`delta_sql_build_ns` measures contribute() term construction, and
`automatic_reprepares` counts SQLite reprepare events on wrapped writes. The
cache/schema tests in `3c` cover index rollback and a second connection's schema
cookie. `2a_plan_audit.py` records indexed key probes, the scans after removing
the k index, and indexed-plan restoration after rollback. The bounded ANALYZE
comparison reduced VM steps but increased measured elapsed time; shared defaults
remain unchanged. Exact commands and counts are in the task report.

Optional `SELECT take2_control('source_views_on')` before opening a batch selects
indexed per-source views instead of duplicate shadow images. Persistent layout
metadata then requires this option and an open batch on every writer; missing
configuration or writes outside a batch fail. The unused image table is emptied
transactionally. Result/support/delta storage remains in SQLite. Each source-side
view is separate so SQLite can push join keys to the source index.

The initial attempt to DROP the image table in xUpdate failed with SQLITE_LOCKED.
The working path creates separate source views and uses ordinary DELETE on the
unused image table. A UNION ALL source view caused 12108 full-scan steps on the
dimension mutation; per-source views reduced that to the 109 support-validation
steps. The repaired profiling run totals 23.867 ms over the four batch1000
mutations, with exact oracle checks. Paired runner measurements are separate.

## Finite option ledger

| Option | Current evidence / disposition |
|---|---|
| Prepared statement reuse | Implemented 32-entry cache. Same batch1000 fixture: 191.8 ms uncached vs 79.7 ms cached; 15007 vs 4 measured write prepares, same VM work. Single profiling run, not a throughput conclusion. |
| SQL/index tuning | Indexed per-source views remove duplicate image writes and UNION ALL materialization. Profiling dimension update: 12108 to 109 full-scan steps; 8.034 to 0.331 ms. |
| Counter SQL tuning | Profile attributed about 6.6 ms per batch1000 case to per-event `_stats` UPDATEs. Trigger fusion measured slower; counted delta metadata removes those UPDATEs while `_stats` remains exact during batches. Both variants have executable lifecycle/count tests and separate paired arms. |
| Bounded session-local cache | Implemented 32 prepared statements per vtab. SQLite pager targets of 1/8/32 MiB measured 28.440/29.046/26.834 ms median over two batch1000 profiles each. Pager size is a target, not a hard memory ceiling; existing runner RSS guards remain. No host source-relation copy. |
| Scheduler metadata cache candidate | Component profile measured only about 0.7 ms total in persistent batch-state reads across batch1000 mutations. No metadata cache added in this pass; measured counter SQL cost was about 6.6 ms. This does not establish a row-trigger floor. |
| Delta consolidation | Implemented persistent signed support queue and set-based flush; nine batch tests per layout pass. |
| Explicit SQL batch/flush | Implemented above, with read/missing-flush misuse rejection and savepoint tests. |
| Public vtab lifecycle batching | `take2_lazy` queues source deltas, flushes on xFilter and xSync, and passes completed-statement reads, conflicts, savepoints and failed-read/source rollback tests. It does not treat xSync as statement end. Explicit batch and epoch variants remain. |
| Preupdate/session capture + drain | `8_session_probe.c` owns an exclusive connection, uses the upstream session API without replacing hooks, and incrementally maintains a persistent same-DB mirror from changed primary keys. Naive drain/reset loses pending prefix maintenance on rollback-to; the probe reproduces that gap. Flush-before-savepoint/reset-after-rollback ordering passes fixed conflicts, nested rollback, cancellation and reopen cases under ASan/debug SQLite. Shared-module adoption remains rejected until that scheduler and a missing-drain guard are integrated. The probe has a fixed bounded fixture and a 64 KiB drain-buffer admission check, not a general capture-memory quota. |
| Custom statement-boundary callback | The tested read/xSync contract works on public ABI. `2b_sqlite_build.py` builds pinned SQLite with debug assertions, FTS5 and session support; 640 upstream tests and the competitive gate pass. No callback patch is needed for this contract. Materializing immediately at statement end without a read remains a distinct contract. |

This is a finite experiment ledger, not an exhaustion claim. Read order:
types `0a`, scalar binding `0ab`, SQL lowering `0b`, nonmonotone lowering `0bc`,
batch scheduling `0c`, ABI `1`, profile transport `2`, batch/circuit/epoch tests `3/3a/3b`,
shared transport `4`, gate `5`. `0_circuit.py` adapts the shared circuit oracle.

## Added circuit contracts

All new modes require the explicit batch boundary. `SELECT id,k FROM result`
returns the two projected values with bag multiplicity, using a cursor support
counter without expanding a relation in host memory. Physical column names are
retained for ABI compatibility. `SELECT id FROM result` reads reachability nodes.

| Mode | Source sides | Query semantics |
|---|---|---|
| project | a | `SELECT k,v*2 FROM a WHERE v>=0` by default |
| inner | a,b | `SELECT a.k,a.v*b.v FROM a JOIN b ON a.k=b.k` |
| self_chain | a | `SELECT x.k,y.v FROM a x JOIN a y ON x.v=y.k` |
| chain | a,b,c | `SELECT a.k,c.v FROM a JOIN b ON a.v=b.k JOIN c ON b.v=c.k` |
| semi / anti | a,b | Left bag rows with / without equal-key right support |
| reach | edges a(k,v), roots b(k) | Set reachability, including cyclic retraction |
| distinct | a | Set projection with signed bag support counts |
| fanout | a | Bag union of `v>=0` and `v%2=0` filtered projections |
| diamond | a,b,c | Bag union of a-to-b and a-to-c chain joins |

Scalar predicates and value projections use SQLite's parser and binder:

```sql
CREATE VIRTUAL TABLE result USING take2(project,
 'b0.k BETWEEN -2 AND 2 AND (b0.v IS NULL OR b0.v<>1)',
 'CASE WHEN b0.v IS NULL THEN NULL ELSE abs(b0.v)+b0.k END');
SELECT take2_attach('result','a',0,'id','k','v');
```

The private plan-only SQLite connection permits k/v references, scalar operators,
CASE and abs/coalesce/ifnull/nullif. It rejects subqueries, aggregate functions,
unknown columns, bind parameters and unapproved functions. The caller's hooks and
authorizer remain untouched. Inputs retain the integer/NULL domain; accumulator
overflow and noninteger projections fail the flush before storage affinity can
coerce numeric text or real values. This is scalar SQL
binding plus fixed relational templates; arbitrary SELECT compilation is absent.

Semijoin/antijoin lower `dLeft * oldMembership + currentLeft * dMembership`.
Right-side support counts determine zero crossings; NULL equality matches no
witness. Self/multiway joins enumerate affected occurrences with inclusion-
exclusion against current sources, including cross terms in one batch.

Reachability uses DRed: negative edge/root support seeds an overdelete cone;
current roots and surviving incoming edges rederive that cone, followed by new
support propagation. `_cone(k INTEGER PRIMARY KEY)` is transactional shadow
storage. Recursive UNION deduplicates cycles. The CTE technique was inspected in
`v6/sprefa-store/src/engine.rs` retract_dred_cte; no store/kernel code changed.
Reach nodes must be non-NULL integers. Rootless cycles retract in the shared
oracle. This mode maintains set reachability, not path counts or arbitrary rules.

The circuit suite covers savepoints across flush, rollback, injected xSync
failure, ABORT/FAIL/IGNORE/REPLACE, UPSERT, duplicate supports, NULL joins and
reopen in both layouts. The shared suite adds 143 circuit states per layout,
covering all 11 families in `30_circuit_workload.mjs`. Additional semantic catalog
families, arbitrary recursive programs and partial-order time are unsupported.
DROP removes extension-owned source triggers, views,
indexes and DRed cone storage. A teardown test proves rollback restores the
schema and exact output, and a completed DROP permits ordinary source writes.

## Optional finite scalar epochs

`take2_epoch` is a separate module name backed by the same maintenance code.
Its persisted schema declaration keeps the contract across reopen and writers.
It adds `_clock(epoch)` and `_frontier(side,t)`, exactly three scalar input
frontiers matching the shared DD host's a/b/c completion probe. Epochs begin at
1, advance once per opened batch, and stop with an error at 10^12.

```sql
CREATE VIRTUAL TABLE result USING take2_epoch(project);
SELECT take2_attach('result','a',0,'id','k','v');
BEGIN;
INSERT INTO result(op) VALUES(10); -- opens epoch 1
INSERT INTO a VALUES(1,1,2);
INSERT INTO result(op,id,side) VALUES(12,1,0),(12,1,1),(12,1,2);
INSERT INTO result(op) VALUES(11); -- requires all three seals
SELECT id,k FROM result;
COMMIT;
```

`op=12,id=epoch,side=0..2` seals that input. The epoch must equal the open epoch
and strictly advance its previous frontier. A sealed input rejects further
source writes in that batch. Holding c unsealed blocks flush and output reads,
even for a query that only reads a. Seals, clock, deltas and outputs roll back
together. A second writer loads the same module and obeys the persisted contract;
an absent module rejects source mutation. This is a sequential scalar completion
barrier. Per-record timestamps, retained historical versions, late data in closed
epochs and arbitrary partial-order antichains are unsupported.

The `sqlite-competitive-frontier` shared arm uses source views and explicit
sealing. Its finite held-c check corresponds to `34_circuit_dd.rs:132` and
`33a_dd_host.rs:83`. Those DD hosts already advance all inputs and wait on their
probe. No DD or kernel source changed. SQL frontier costs are reported separately.

## Public-ABI lazy read/commit flush

`CREATE VIRTUAL TABLE result USING take2_lazy(inner)` selects automatic delta
queuing. Ordinary source statements need no op10/op11 transport. Maintained reads
flush pending deltas before opening their shadow cursor. xSync flushes remaining
deltas during transaction preparation; source and maintained state commit or
roll back together. A failed flush returns an error. Source-view writers retain
the explicit connection configuration requirement.

The lazy tests cover completed-statement visibility, autocommit and explicit
transactions, ABORT/FAIL/IGNORE/REPLACE/UPSERT, nested source-read trigger abort,
failed SELECT after a flush, xSync failure, second writers, absent module and
reopen. Both storage layouts pass. `sqlite-competitive-lazy` is a separate shared
arm. Its three-repetition batch1000 total was 30.373 ms versus explicit source
views 29.285 ms and pg_ivm 20.000 ms. This removes caller flush scheduling under
the tested visibility contract; it does not establish a performance improvement.

## Reused sources

The ABI comes from installed `sqlite3ext.h`; SQLite parses/binds scalar SQL and
stores every relation, delta, support and epoch. OpenIVM's current-base delta
mask/sign technique is reused as a fixed SQL lowering; its DuckDB-bound compiler
is not ported. The smallest compiling target is this one C extension translation
unit. Pg_ivm transition logic and SQLite FTS5 lifecycle/storage tests informed
the transaction boundary. No source-relation engine runs outside SQLite.

Pinned inspection sources at `/tmp/sprefa-sqlite-extension-research.ZsALrZ/`:
SQLite `f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9` (public domain), OpenIVM
`3b3938f4f8293875b56157f563c6f8cb196a0b41` (MIT), pg_ivm
`dda7470e085822c215411c0b349f4aa4fbb9fdf4` (PostgreSQL license). The upstream
licenses and source receipts remain in the original research/Take 2 material.
Discussion 309's Feldera-generated Rust replies remain a proposal; they do not
prove SQLite-native maintenance. Take 2's executable FTS5 receipts are distinct
from the earlier source-inspection report whose testfixture build lacked Tcl.
