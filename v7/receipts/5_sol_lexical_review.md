# Sol lexical core review

Status: core accepted with one known alias gap; alias proposal absent.
SHA: `52c8b2a7b563585145a29640505df83ed85acd55` against `5dc1e9360`.
Files reviewed: `v7/src/2_comptime/0_lowerer.pl`,
`v7/src/2_comptime/1_checker.pl`, `v7/test/19_lexical_binding.test.pl`,
and `v7/test/fixtures/lexical_binding/*`.

Evidence:
- `lower_expression/7` and `expression_callable/4` call
  `scoped_reservation/5` before kind classification.
- `scoped_reservation/5` retains same-owner product preference and then
  walks enclosing owners with cycle protection.
- Callable classification retains `product`, `derived_callable`, then
  kernel fallback only when no lexical reservation exists.
- Checker visitation now keys `Owner-Name`; direct aliases and multi-name
  chains can revisit one owner under different names, while self and
  two-name cycles terminate through existing unresolved-name diagnostics.
- `resolve_edges/6` deliberately skips `deferred_expression` targets.
  Therefore pair-keyed visitation cannot resolve `(: Name (Option text))`
  followed by `(: field Name)`.
- Existing `lower_derived_bind_rules/5` emits
  `:(BindOwner, Name, BindValue, Index)` from the deferred expression, and
  `lexical_atom_value/7` already emits a scoped colon lookup for expression
  reads. This is the existing seam for expression aliases and chains.
- Compound targets already pass deferred expressions through
  `compound_edge_target/5`, preserving field owner and position in the
  generated colon head.

Validation: inspected reported test19 source and fixture assertions; parent
reported 8/8. No suite rerun. The eighth test still asserts the known
`unresolved_name(field)` result and must become an exact success assertion.

Next: implementation lane should send the promised <=15-line proposal.
Review it for direct aliases, chains, compound labels, lexical shadowing,
referenced-name scope, cycles, and owner/index preservation before edits.
