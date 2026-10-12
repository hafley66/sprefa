# Zone card

Updated every turn. Top section is the only thing to read when lost.

## Now

**Thread:** value type checking, done through memory layout, rules in userland.

**Waiting on you:** OK to build steps 1-2 below.

1. `raw_columns` (`src/_6_eval/_9a_ir_lower.rs:1241`) keeps the witness when a
   declared-int column falls back to term ids, instead of a bare `false`.
2. One `column_layout(Relation, Column, Declared, Stored, Witness)` row per
   column goes into the next comptime round; a prelude rule turns
   `declared int, stored term` into `column_type_mismatch`.
3. Probes become tests: `plans/v8/probes/prover/7_string_in_int_column.dl7`,
   `8_int_flows_into_str_column.dl7`.

Why this shape: one row per column, never per seed row; the check is a fact
the storage layout already computes; the rule that judges it is dl7, per
language.

## Facts this rests on

- Generics substitute at check: `plans/v8/2026-10-11-generics-trace.md`.
- Type rows live in `compiler_rows`; value rows in `program.seeds`; nothing
  joins them today.
- Prover catches 7 of 9 probe cases; misses both value-vs-type cases
  (`plans/v8/probes/prover/`).

## Parked, in order

1. Read phase hotspot, 4 passes over 1M where 1 is needed:
   `plans/v8/probes/read_expand/README.md`. Fix: shallow colon test.
2. Doc rail: a test that fails when a doc path stops resolving (v5 had
   `stale-doc.dl`).
3. `Result` in `prelude/` (decided, not built).
4. Storage classes beyond int: text dictionary, term of type node.
5. Unaudited: `.agents/`, `.claude/`, `issues/`;
   `plans/v8/probes/2026-09-28-compile-gate.py` still forces `ir:sqlite`.
6. Probe `2_seed_arity_mismatch` reports `variable_in_seed`, a misleading name.

## Done today (main, local, 6 commits ahead of origin)

- comptime 14.6s -> 9.9s, RSS 4.3G -> 3.2G at 1M (spark-2, paired runs)
- docs: archive split, README/env/AGENTS/CLAUDE match code, refreeze on `ir:dd`
