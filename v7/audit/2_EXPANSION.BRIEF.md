# Slice 2: AST and relation normalization

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/0_ast_expand.pl`
- `v6/prolog/0_body_walk.pl`
- `v6/prolog/0_relation_pattern.pl`
- `v6/prolog/0_relation_edge_expand.pl`
- `v6/prolog/0_rel_record.pl`
- `v6/prolog/0_seq_expand.pl`
- `v6/prolog/1_expansion.pl`

Identify the earliest stable IR after surface expansion. Mark predicates whose
only purpose is DL6 syntax recovery separately from predicates preserving rule,
body, occurrence, edge, or relation semantics.

Write `v7/1_AUDIT/results/2_EXPANSION.md`.
