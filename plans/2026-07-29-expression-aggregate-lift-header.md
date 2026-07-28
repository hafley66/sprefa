# EXPRESSION + AGGREGATE LIFT (planner contract; user go 2026-07-29 "agg'ing etc, sqlite lowering down pat")

Lift the phase-C refusals into the incremental emitter under
expression_residency (fuse to SQL deltas, TS deopt last) and
host_residency (rows stay in sqlite). Target buckets (sweep names):
comparison-in-level-body 14, comparisons 12, aggregate_head 9,
arithmetic bind 5, pre 8 stays OUT of scope (its own construct).
Success = those fixtures leave unsupported, compile, and grade
byte-identical; compiled coverage rises from 34/115 accordingly.

## Scope

1. COMPARISONS + ARITHMETIC + := BINDS: fuse into the emitted
   statements (WHERE for guards, SELECT expressions for binds and
   head arithmetic). The phase-C miscompile classes get FAIL-FIRST
   checks written BEFORE the lift: (a) TEXT-collapse "1" vs 1 across
   a typed column boundary; (b) @libsql number->REAL/bigint bind
   corruption. Write both as plunit/fixture checks that fail on the
   naive translation, then lift until green. (The sqlite_udf lab is
   independently drafting a Q4 assertion set; on its landing the
   coordinator reconciles, do not wait for it.)
2. AGGREGATE HEADS, incremental story PER CLASS, named honestly:
   - count/sum: decomposable; per-group accumulator maintained by
     delta joins (+= on add, -= on retract), the count-IVM shape the
     repo already trusts.
   - min/max: NOT decomposable under retraction (the match-frontier
     rx table called incremental min/max over a retractable set
     impossible; respect that finding). Strategy: delta-compare on
     inserts, GROUP-SCOPED recompute on deletes (never whole-table).
     Statement receipts must show the recompute is scoped to affected
     groups.
   - json_array/json_object: sqlite-native json_group_array/
     json_group_object. ORDERING is the hazard: the tick log is
     byte-graded, so the emitted group expression must pin a
     deterministic ORDER BY matching the oracle's ordering rule; read
     conformance/engine.pl's aggregate evaluation FIRST and state the
     ordering rule in the summary.
3. The naive referee stays available (SPREFA_TSV2_EMITTER_MODE=naive)
   and every lifted fixture is graded in BOTH modes.

## Grades

sweep: lifted buckets leave unsupported, RUN identical grows by
exactly the lifted fixtures, zero movement elsewhere, both modes;
conformance (max 3 full runs) + roundtrip + plunit + tsv2 + import
gate; EXPLAIN receipts on aggregate delta paths (SEARCH on delta
side, group-scoped recompute receipts for min/max); one scale spot
cell (s2/10k) unchanged within noise.

## Laws

Worktree agent: FIRST ACTION `git merge --ff-only <base sha stated at
dispatch>`; if it fails or v6/ is missing, STOP AND REPORT (dispatch
law). Descriptive identifiers; no em dashes; banned words provenance,
substrate, load-bearing, regime; N+1 law; hermetic scratch dbs only;
interface-bound functions in the header types file for any new runtime
helper. Commit per logical step with git commit -n. Do NOT merge; the
coordinator review-gates. If a class cannot reach byte identity,
keep its refusal and NAME the crack.

## Final summary shape

Per-bucket lift table (fixture -> compiled/identical/mode), the two
fail-first receipts red->green, the min/max group-scope receipts, the
json ordering rule, all grades, cracks.
