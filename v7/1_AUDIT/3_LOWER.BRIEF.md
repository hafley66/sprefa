# Slice 3: canonical lowering

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/lower.pl`
- `v6/prolog/0_graph.pl`
- direct tests and callers of exported lowering predicates

Build an entry-point and predicate-family map for `lower.pl`. Identify which
families consume surface-shaped terms and which emit the canonical program
contract used by compilation and engines. Report dynamic state and ordering
dependencies explicitly.

Write `v7/1_AUDIT/results/3_LOWER.md`.
