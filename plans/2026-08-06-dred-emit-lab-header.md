# DRED-EMIT LAB HEADER: incremental insert AND retraction for recursive heads

Seeded by coordinator 2026-08-06 on `lab/emit-reconcile`. Closes the card-5
design question: recursive heads stop full-recomputing. The algorithms are
NOT new; they are sprefa-store's, already labbed and priced 2026-07-22.

## TOC
1. Receipts (why nothing here is a guess)
2. Phase 0: probe, DONE, numbers
3. The policy the numbers force
4. Phase 1: emit it (lane spec)
5. Gates
6. Known risks

## 1. Receipts
| claim | where proven |
|---|---|
| insert path: delta-seeded frontier ping-pong, work ∝ new cone | `v6/sprefa-store/src/engine.rs:407-448` (`assert`) |
| delete path: over-delete cone + rederive, cycle-safe | `engine.rs:454-540` (`retract_dred`) |
| counting is WRONG on cycles (phantom rows) | chat_log/20260722.0 line 37; `tests/agreement.rs` |
| recursive CTE LOSES to temp-table loop (~20%) | chat_log/20260722.0 line 41; reproduced in phase 0 |
| DRed cost is algorithmic (2 passes), not sqlite misuse | chat_log/20260722.0 line 87 |
| oracle referee = checksum equality on a twin db | `tests/agreement.rs` pattern; phase-0 `referee()` |

## 2. Phase 0: DONE. `v6/labs/exec_shootout/dl6/dred.mjs`
Run: `node dred.mjs 30 100` and `node dred.mjs 50 100`. Every scenario
checksum-MATCHES the full-recompute oracle, including the dead cycle.

| scenario (grid 50x50, closure 1,625,625 rows) | incremental ms | full recompute ms | ratio |
|---|---|---|---|
| build from scratch (loop vs CTE) | 1,647 | 2,157 | loop wins 1.3x |
| insert one edge | 40 | 2,179 | 54x |
| insert 100 random long jumps | 1,084 | 3,517 | 3.2x |
| delete one structural edge (49 true retractions) | 60 | 2,195 | 37x |
| delete the 100 jumps (884,015 true retractions) | 7,153 | 2,218 | recompute wins 3.2x |
| cut a cycle's only anchor (cycle must die) | 0, MATCH | 0 | correctness fixture |

## 3. The policy the numbers force (PROFILED, dredprof.mjs + dredopt.mjs)
Worst-case profile (delete 100 scattered edges, head 2.5M, cone 2.17M = 87%):
hop_generate 2,411 ms / head delete 1,195 / rederive+revive 2,621 / cone copy
690. Variants raced, all checksum-MATCH:
| variant | ms | verdict |
|---|---|---|
| A shipped two-pass | 8,179 | baseline |
| B drop head probe in hop (tautology: old head is a fixpoint) | 7,440 | TAKE |
| D fused round-tagged cone, OR-IGNORE dedup | 36,209 | REJECT: PK-constraint rejection >> NOT EXISTS at this duplication |
| BC defer head delete to the end | 36,663 | REJECT: rederive must probe the SHRUNKEN head; the up-front delete is what makes pass 2 cheap (= store's weight=0 mark) |
| E = B + mid-walk bail at cone > head/4 | 3,190 | TAKE |
| full rebuild | 2,190 | the floor when cone ~ table |
Two-pass cost is algorithmic (store lab G2 said so; reproduced), so the guard
is NOT a pre-guess on seed count (seeds were 2.2% of head and still exploded
to 87%): the driver already holds the cone count for free (sum of changes)
and BAILS MID-WALK to one full rebuild past threshold. Worst case is now
rebuild + bounded walk (~1s at 25%); common case never pays anything.
- additive-only delta -> assert path (always; never worse than recompute)
- retraction delta -> DRed with mid-walk bail at cone > head/4
`retractionGuardSql` already discriminates the tick kinds; the guard extends
it instead of adding a new mechanism.

## 4. Phase 1: emit it (flash4 lane spec, worktree off 39ae7072)
Generalization rule, mechanical: the store's `(parent_key, child_key)` dep
table IS the recursive arm's body join, evaluated at walk time.

| store concept | emitted concept |
|---|---|
| `cx_row.weight` 0/1 | head-row presence (`INSERT OR IGNORE` / `DELETE`) |
| `cx_dep` hop | recursive arm body join, non-recursive atoms at current state |
| seeds | one statement per arm with the delta atom substituted at each position |
| frontier/next ping-pong | `__ping_<rel>` / `__pong_<rel>` TEMP tables, role swap in the driver |
| cone | `__cone_<rel>` TEMP table |
| rederive anchor check | one EXISTS per arm, every atom read from surviving state |

File ownership (disjoint):
- lane A: `v6/prolog/lower.pl` (`level_ref_count_sql` region, new
  `assert_sql`/`dred_sql` emission) + `v6/prolog/emit_ts.pl` templates.
- lane B: `v6/tsv2/runtime/1_incremental.ts` (`applyLevelsBeforeEdges` picks
  assert/dred/full per tick via the guard) + tests.
- `dred.mjs` is FROZEN as the statement-shape spec; lanes copy shapes from it,
  never edit it.
Naming law: refCount vocabulary only; `support*` identifiers are the known
violation awaiting the separate rename commit; introduce zero new ones.
Single-recursive-read is already enforced (`check_single_recursive_read`);
multi-arm heads emit one seed statement per arm.

Validation (every lane, every commit):
- `node v6/labs/exec_shootout/dl6/dred.mjs 30 100` exits 0, all MATCH
- tsv2 battery, conformance 302, sweep 420 (from v6/tsv2: `just` targets as in
  `bench.sh`)
- `just dl6-bench` gate 2 must not regress past 2,200 ms

## 5. Gates
- Sweep stays byte-identical on tick logs for all 420 fixtures.
- Conformance 302 PASS / 0 FAIL.
- NEW rail (COUNT law): extend `recursiveClosureCounts.test.ts` with a
  retraction slope: rows touched on a delete tick ∝ cone, never ∝ |head|.
  Fail-first receipt required against the shipped full-recompute path.
- NEW agreement test: checksum referee vs full recompute across an
  insert/delete tick script, cyclic fixture included (dead cycle dies).

## 6. Known risks
- Delta `_sequence` ordering: staged rows must keep the arrival order the 420
  logs pin. Phase-0 rowid trick from `arrival_scratch_table_name` applies.
- Same-tick mixed delta (inserts + deletes): order = DRed first, then assert
  (store runs retract before assert for the same reason; a rederived row must
  not double-stage).
- TEMP-table lifetime: ping/pong/cone are per-connection; serve mode reuses
  connections, so `DELETE FROM` at statement head, never `CREATE`.
- G2 (store lab, open): over-delete+cone-record fusion could shave the 2-pass
  cost; out of scope for phase 1, noted for phase 2.
