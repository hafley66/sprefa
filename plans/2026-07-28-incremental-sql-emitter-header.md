# INCREMENTAL SQL EMITTER ARC (planner contract, user directive 2026-07-28 night)

User words, condensed: the rust store -> TS port -> tsv1 langium -> v6.2
prolog -> tsv2 lineage was the lab sequence; take the good ideas from
the previous increments and merge them in BY GENERATION: "we could
generate efficient sql code for this stuff inline ... the prolog/souffle
way, where we just inline it, so the sqlite combos we found with high
efficiency can just be reused in prolog itself."

The move: lower.pl stops emitting only DELETE-all + rebuild. It gains an
INCREMENTAL statement family, specialized per program at compile time,
inlined into the generated module as visible SQL text. No runtime engine
library; the optimization lives in the emitted statements. Target-neutral
by construction (SQL text in the lowered plan; TS and future rust/bash
backends inherit identically).

## The good ideas to mine, with their sources

1. Semi-naive delta joins: new_head = delta(body_atom) JOIN rest-at-
   current-state, per rule, instead of full rebuild. Reference shapes:
   the rust store's frontier -> one hop -> prune -> fixpoint cascade
   (v6/sprefa-store/js/src/engine/engine.ts header documents the shape
   and notes the two cascade optimizations landed RUST-side only; the
   rust sources are the authority), and lowerSql.ts's recursive-stratum
   rounds (semi-naive within a stratum already; lift it across ticks).
2. Count-IVM support maintenance as SQL: per-row support counts
   maintained by UPDATE joins, retraction = decrement + collect-zero
   loops. The 4-5x-vs-DRed receipt is the rust store's. The REAL-SQLITE
   statement patterns arrive from the sqlite_retraction lab currently
   running (its verdict's support-count strategy IS the template).
3. DISTINCT placement: engine.ts's documented decision (DISTINCT in
   assert_body and the retract cascade, dropped elsewhere) becomes an
   emitter rule, not a runtime choice.
4. Index/storage policies with receipts: storage-diet direction 5
   (planner-honest demand filters, PK-prefix on rowid tables, tiny-rel
   floor, constant-column) and the WITHOUT ROWID decisions -- emit
   CREATE INDEX per program from the rule set, the way souffle picks
   per-relation data structures by analysis.
5. N+1 law: arrivals and deltas batched, never per-row statements
   (already house law; the incremental family must respect it).

## Grades

- Byte-identical tick logs across the WHOLE corpus vs the oracle (the
  grade already exists; incremental evaluation must be observationally
  identical to recompute, fixture by fixture). Any diff = the
  incremental lowering is wrong, not the fixture.
- Scale bench before/after: the same matrix rows (bench/engines/
  tsv2_gen.sh) re-run on the incremental emitter; the deliverable
  number is the curve bend. The naive emitter STAYS available behind a
  flag as the semantic referee (recompute = reference semantics, the
  Babel precedent in ARCH.pl).
- Per-statement receipts: EXPLAIN QUERY PLAN assertions on the emitted
  delta joins (SEARCH not SCAN on the delta-side index), same style as
  the rowsForPath count tests.

## Phases

P1 non-recursive level rules: delta-join statements per rule; boundary
   diff computed FROM the delta stream instead of full-table snapshots
   (kills the O(table)/tick snapshot reads).
P2 recursive strata: semi-naive across ticks in SQL (frontier tables).
P3 retraction: support-count SQL from the retraction lab's verdict;
   finalize arms ride the same delta stream.
P4 index emission per program (the souffle-style specialization).

## Sequencing (hard)

- BEHIND the sol registry merge (sol owns compile/*.pl right now; this
  arc lands in the registry-shaped lower.pl).
- BEHIND the scale bench landing (the baseline curve is the before).
- P3 BEHIND the sqlite_retraction lab verdict (its statement patterns).
- Runner: sol-class (open lowering decisions per rule shape); briefs per
  phase, each phase = its own worktree + full-corpus byte-identity gate.

## TEMPERING (user, same night): lab it, it aint godhood

Inline generation is NOT a dogma. P0 becomes a LAB before any phase:
prototype the incremental families on 3-4 fixtures and grade, per
statement family, INLINE-SPECIALIZED vs ONE SHARED FUNCTION CALL:
- inline when the SQL text genuinely differs per program (join shapes,
  column lists) so specialization carries information;
- a single runtime helper call when the code is common modulo
  parameters -- generated-file linecount is a REAL criterion, and the
  precedent already exists (selectRows, multisetDiff are helper calls
  in generated files today).
Lab deliverable: per-family verdict table (inline | helper | mixed) with
linecount, readability, and perf columns; tick-log identity as always.
The souffle framing survives only where the specialization pays.
