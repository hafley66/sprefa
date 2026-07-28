# SQLITE RETRACTION LAB (planner contract, user go 2026-07-28 night)

User word: "try reimplementing the retraction algos we found for sqlite
using queries but with deletion and fks and the cascade ... a prototype
standalone swi prolog script, it gets a special space."

The types lab ASSERTED (round 1, check 3) that SQL FK ON DELETE CASCADE
gives wrong semantics for shared children while support counting is
right and complete on value DAGs, and that counts fail on cycles. Those
claims were graded in a PROLOG MODEL of the store. This lab re-proves
them in REAL SQLITE with real DDL, real DELETEs, real foreign keys --
deletes have never been labbed against the actual database and the user
finds them scary; this makes them mechanical.

## Special space + shape

v6/prolog/labs/sqlite_retraction/ (lab protocol: dies on landing, last
copy recorded by hash). ONE standalone entry:
`swipl -q -l v6/prolog/labs/sqlite_retraction/lab.pl -g go -g halt`
exits 0, prints ONLY PASS lines. Drive a REAL sqlite database: shell to
the sqlite3 CLI via process_create with a scratch db file under /tmp
(dependency-free; no ODBC, no packs). Every scenario builds a fresh db.

## The three strategies, all as real SQL against the same schema

The schema: the types lab's route/view value graph (parent tables, ref
columns, a two-parent shared child) plus a cyclic entity pair.
1. FK_CASCADE: child ref columns declared FOREIGN KEY ... ON DELETE
   CASCADE (PRAGMA foreign_keys=ON); delete a root with a plain DELETE
   and let sqlite cascade.
2. SUPPORT_COUNT: a support column (or count table) maintained by
   queries; delete = decrement via UPDATE joins, collect zero-support
   rows with DELETE ... WHERE, iterate to quiescence (each iteration =
   plain statements, loop in prolog until no rows change).
3. FIXPOINT_RECOMPUTE (the referee): live(id) as a recursive CTE from
   roots over refs; delete = DELETE everything not in the CTE result.

## Graded scenarios (hand-computed expected survivor sets IN the lab)

 a. straight chain, delete root: all three agree, full cascade.
 b. SHARED CHILD (two roots, one child), delete root 1 then root 2:
    expected = child survives step 1, dies step 2. Prediction to prove:
    FK_CASCADE kills the child at step 1 (wrong) OR dangles -- record
    what sqlite ACTUALLY does, including whether the second parent's
    ref row blocks or cascades, with the real error text if any.
 c. CYCLE (a<->b reachable from root), delete root: expected = both
    die. Prediction: SUPPORT_COUNT leaves both alive (counts lie on
    cycles); FIXPOINT_RECOMPUTE gets it right; FK_CASCADE -- record
    what sqlite really does with circular FKs (deferred constraints?).
 d. DIAMOND (root -> a,b -> shared c), delete root: all-die case where
    naive per-edge cascade double-visits; check each strategy visits/
    deletes exactly once (count affected rows).
 e. CRASH MID-CASCADE: run SUPPORT_COUNT inside an explicit
    transaction, kill the sqlite3 process between iterations (or
    ROLLBACK to simulate), reopen, assert the db is the PRE-delete
    state (atomicity receipt, the endurance law's shape).

## Deliverables

- The lab (PASS-only, scenarios a-e x strategies where applicable).
- plans/2026-07-28-sqlite-retraction-verdict.md: the strategy x
  scenario matrix (survivor sets, affected-row counts, timings at 10k
  rows for each strategy on scenario a), what sqlite FK cascade
  ACTUALLY did with receipts, and the one-paragraph implication for the
  engine (when is FK cascade ever safe = only single-parent trees;
  support = DAGs; CTE recompute = cycles/referee).

## Laws

Lab protocol + style laws as standing (descriptive vars, PASS-only, no
em dashes, banned words, rx/prolog/sql vocabulary). No edits outside
the lab dir + the verdict doc. Conformance/roundtrip re-run untouched
as the no-drift proof.
