# Zone card

Updated every turn. Top section is the only thing to read when lost.

## Now

**Thread:** value type checking, done through memory layout, rules in userland.

**Waiting on you:** build caret lowering first? `(^proof: (*)): (...)` expands at macrotime but lowering reads each member target as a call (`undeclared_relation(*)`, throw `src/_2_lower/_8_express.rs:498`; probes `plans/v8/probes/namespace/4_*`, `7_*`). The repo's own caret oracle example fails the same way end to end. Before that: OK the namespace heads `proof.` / `edge.` / `value.` (plus `curry.`, `pick.`, `exclude.`, `intersection.`, `program.` for the existing prefix families). Probe: `plans/v8/probes/namespace/0_member_products.dl7` compiles and derives. Then build steps 1-2 below under those names.

1. `raw_columns` (`src/_6_eval/_9a_ir_lower.rs:1241`) keeps the value that
   knocked a declared-int edge to term storage, instead of a bare `false`.
2. One `edge.seen(Product, Name, Value)` row per edge goes into the next
   comptime round; a prelude rule asks the one prover:
   `proof.asked(ValueType, EdgeType)` from `edge.seen`, `value.type`, `edge.type`.
3. Probes become tests: `plans/v8/probes/prover/7_string_in_int_column.dl7`,
   `8_int_flows_into_str_column.dl7`.

Why this shape: one row per column, never per seed row; the check is a fact
the storage layout already computes; the rule that judges it is dl7, per
language.

**Next design thread (named 2026-10-12): pluggable types.** `Extends`
(`prelude/4_type_algebra.dl7`) split into rule packs, structural default;
scope option rows (`prover_mode`) inherited down modules; union contracts
proven per branch (today `undecided`, `:243-252`); variance once function
types exist. Sketch in chat, nothing built.

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
7. Served body goals emit `effect` rows (`served_effect`,
   `src/_6_eval/_9a_ir_lower.rs:963`, called per body goal `:2224`); the
   2026-09-18 decision (`AGENTS.md:120`) says requests are head-derived or
   seeded input rows. Code and decision disagree.

## Corrections logged

- `intern` in a body is two-way. Bound type and arguments: builds and inserts
  a row into `__ir_intern_requests_{level}` (`_9a_ir_lower.rs:3736`).
  Either unbound: reads those rows back, same run (`:1393-1410`).
  `intern_snapshot` is the previous round's frozen copy.
- Compound heads lower to body `tuple` + `intern` goals
  (`plans/v8/probes/prover/9_compound_head.dl7`).
- Design thread (named 2026-10-12): no implicit inserts in bodies. Compound
  heads lower to N rules, every insert a head; served requests are head-derived
  rows. Name for the per-column value feed: `column_value` is taken
  (`prelude/3_derived_rules.dl7:20`); pick pending.
- Value checking and `Extends` are one prover: a value enters as its type
  (`value_type`), the column check is one more `asked` row.

## Done today (main, local, 6 commits ahead of origin)

- comptime 14.6s -> 9.9s, RSS 4.3G -> 3.2G at 1M (spark-2, paired runs)
- docs: archive split, README/env/AGENTS/CLAUDE match code, refreeze on `ir:dd`
