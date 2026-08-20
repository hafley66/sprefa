---
created: 2026-08-20
updated: 2026-08-20
type: task
status: testing
priority: high
epic: write-verb-interface
labels: [compiler]
commits:
- hash: 845af307e
  summary: write verbs, the six-verb projection and the shared support ledger
- hash: a4b03b6ec
  summary: tsv2 write-verb interface, flag branches deleted
- hash: 4cf9ebaca
  summary: engine-rs WriteVerbs trait, flag branches deleted
- hash: 42673a70e
  summary: retraction battery, three arms, both doors
---

# step 5

Retraction and support parity on the shared frontier path
(plans/2026-08-19-shared-sqlite-frontier.md, sequence step 5).

## Description

The recount verb's shared arm publishes per-rule support to one shared ledger:

```sql
CREATE TEMP TABLE "__support_count" (
  "relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL,
  "rule_id" INTEGER NOT NULL, "count" INTEGER NOT NULL,
  PRIMARY KEY ("relation_id", "row_id", "rule_id")) WITHOUT ROWID
```

All-integer key, `row_id` the durable `__id` (surrogate-key law), hot lookup on
the `(relation_id, row_id)` prefix. The write runs AFTER the head insert, so
every row resident at the end of a recount round carries its counts. A head fed
by ONE rule reads the `__support_next_` staging table it already filled (zero
extra join work); a head fed by two or more re-reads its own arm per rule,
because the staging sum cannot be split back apart. Every write is one batched
INSERT, which the lab measured as mandatory (`v6/labs/shared_frontier/out/q4.md`:
row-at-a-time into a shared table is 11% slower than per-rel tables, a 100-row
multi-row INSERT 7.4x faster).

The recount verb runs only on a tick that carries a retraction
(`1_incremental.ts recompute_levels_after_edges` reads `retraction_guard_sql`
first), so the ledger's contract in this arc is: current for every head row at
the end of a recount round. The battery ends every schedule on a retraction
tick and asserts it there.

The per-rel typed `__support_next_<t>` staging survives this arc: it is the
DISCOVERY buffer, holding rows that are not in the head yet and therefore have
no `row_id` to key the ledger by. Deleting it is the default-flip card's work.

## Acceptance Criteria

- [x] Shared `support_count(relation_id, row_id, rule_id, count)` written by the recount verb
- [x] Keyed replacement, stale retraction, current retraction, negation support counts, restart
- [x] Both doors, per_rel vs shared vs oracle equal
- [x] COUNT tests: statements per run pinned for every case
- [x] EXPLAIN SEARCH on the support_count lookup
- [x] Batched inserts on the support writes

## Implementation Notes

- DDL `v6/prolog/lower.pl:262-270` (`shared_frontier_ddl/1`,
  `shared_support_table/1`); plan `v6/prolog/lower.pl:4796`
  (`support_count_plan/8`); verb row `v6/prolog/lower.pl:7025`
  (`rule_write_verbs/3`).
- Runtime arms: `v6/tsv2/runtime/writeVerbs.ts:267` `SharedWriteVerbs.recount`,
  `v6/sprefa-engine-rs/src/write_verbs.rs:262`. The statements close the recount
  batch after `insert_new`, so no result index shifts
  (`v6/tsv2/runtime/1_incremental.ts` `reconcile_ref_count_statement`).
- Fixtures are prolog TERMS (`v6/tsv2/tests/shared_frontier/retraction.fixtures.pl`)
  so the oracle and both compiled arms read ONE source: `sf_retract_current`,
  `sf_retract_stale` (keyed replacement then a delete of the replaced row),
  `sf_negation_support`, `sf_two_rule_support`.
- The gates compile them with `compile_fixture/5` and print the oracle log with
  `conformance/ticklog.pl` over the same terms; no conformance fixture file was
  touched.

## Tests Run

TS gate (`v6/tsv2/scripts/shared-frontier-gate.sh`), three arms plus the ledger
invariant, statements pinned over the whole run:

| fixture | per_rel | shared | oracle | ledger rows | restart |
| --- | ---: | ---: | --- | ---: | --- |
| sf_retract_current | 96 | 74 | equal | 2 | equal |
| sf_retract_stale | 73 | 63 | equal | 2 | equal |
| sf_negation_support | 142 | 118 | equal | 2 | equal |
| sf_two_rule_support | 105 | 87 | equal | 1 | equal |

Ledger invariant, per case: every head row's `sum("count")` over the ledger
equals its `__refcount`, and
`EXPLAIN QUERY PLAN SELECT "count" FROM "__support_count" WHERE "relation_id" = ? AND "row_id" = ?`
reports SEARCH with no SCAN.

Rust gate (`v6/sprefa-engine-rs/shared-frontier-gate.sh`): all four byte-diff
per_rel = shared = oracle.

Counts re-measured three times, stable.

## Decisions

### 2026-08-20T15:48:31Z · @write-verb-interface-lane

The shared ledger is written AFTER the head insert, so it is current for every resident head row at the end of a recount round; the per-rel __support_next_ staging survives as the DISCOVERY buffer, because a row not yet in the head has no row_id to key the ledger by. A single-rule head writes the ledger straight from that staging (no extra arm evaluation); a multi-rule head re-reads one arm per rule id.
