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
types `0a`, SQL lowering `0b`, batch scheduling `0c`, ABI `1`, profile transport
`2`, batch tests `3`, shared transport `4`, gate `5`. `0_circuit.py` adapts the
existing aggregate circuit for the preserved Take 2 baseline and batch variant.
