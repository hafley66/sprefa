# Competitive SQLite-native IVM

Take 1 and Take 2 remain immutable comparison arms. This directory copies the
Take 2 C extension as the starting point for separately built variants.

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
retains transaction-safe event counts. Instrumented write wrappers count prepare,
step, VM and full-scan work. The separate read-only invariant probes are outside
those counters, so preparation attribution is a lower bound.

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
| Bounded session-local data cache | Candidate: SQLite session library, available in host compile options. No source-relation cache implemented. |
| Delta consolidation | Implemented persistent signed support queue and set-based flush; eight batch tests pass. |
| Explicit SQL batch/flush | Implemented above, with read/missing-flush misuse rejection and savepoint tests. |
| Public vtab lifecycle batching | xSync is commit preparation, not statement end; Take 2 exact traces prove this. Current contract uses explicit flush, preserving precommit reads after flush. Automatic statement batching remains unimplemented. |
| Preupdate/session capture + drain | SQLite exports both ENABLE_PREUPDATE_HOOK and ENABLE_SESSION. Its header explicitly declares session objects plus an independently registered preupdate hook undefined behavior. This is not admitted to the shared-module contract without an exclusive-ownership proof; no hook is replaced. An exclusive connection factory remains a separately testable configuration. |
| Custom statement-boundary callback | Not currently required by the explicit contract. A transparent statement-end API would need separate evidence and a minimal pinned variant; no patch has been made. |

This is a finite experiment ledger, not an exhaustion claim. Read order:
types `0a`, scalar binding `0ab`, SQL lowering `0b`, nonmonotone lowering `0bc`,
batch scheduling `0c`, ABI `1`, profile transport `2`, batch/circuit tests `3/3a`,
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
overflow and noninteger stored projections fail the flush. This is scalar SQL
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
reopen in both layouts. The shared suite adds 104 circuit states per layout.
Time/frontiers, arbitrary recursive programs, and remaining circuit catalog
families are unsupported. DROP removes extension-owned source triggers, views,
indexes and DRed cone storage. A teardown test proves rollback restores the
schema and exact output, and a completed DROP permits ordinary source writes.
