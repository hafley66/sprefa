# Slice 8: validation, stratification, and clock checking

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/0_program_check.pl`
- `v6/prolog/strat.pl`
- `v6/prolog/3_clock_check.pl`
- `v6/prolog/0_negated_guard_expand.pl`
- `v6/prolog/0_unsupported_messages.pl`
- direct tests

Trace the dependency graph, SCC, stratification, recursive-construction,
determinism/cardinality, temporal-delay, and diagnostic laws. Separate source
syntax checks from canonical semantic checks.

Write `v7/1_AUDIT/results/8_CHECKS.md`.
