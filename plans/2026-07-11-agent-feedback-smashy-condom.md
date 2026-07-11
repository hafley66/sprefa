# Agent-session feedback: smashy condom-metrics dogfood (2026-07-11)

External Sonnet session built wrapper/"condom" metrics over ~/projects/smashy with dl
(artifacts there, uncommitted: `.dl/proposed-condom-metrics.dl`,
`.dl/proposed-condom-rail.dl`, `docs/research/dl-condom-metrics.md`). Untriaged
against sprefa code; capture only (same pattern as the S1-S6 batches).

## Gotchas hit (docs/ergonomics candidates)

- rels need explicit `rel` declarations even inline (surprised the agent).
- Negation only wraps atoms, not raw comparisons — worked around via a helper rel
  (`is_test_file`). Candidate: allow `!` on comparisons, or document the helper-rel
  idiom where the error fires.
- sg source rules need their own bare `scan(...)` call, not a derived rel.
- Aggregation goes inside the atom's parens (`rel(count(path), _)`), not as a prefix.
- Consumer repos have no `docs/reference/*.md`; `dl docs <topic>` is the equivalent —
  discoverability gap for agents that grep for docs first.

## Capability gaps (feature candidates)

- No "body is a single forwarding call" relation — hollow-delegation approximated by
  span ≤3 lines + ≤2 downstream calls.
- `type_sig` has NO field slot for Rust, confirmed empirically (0 field rows vs 1,929
  params / 1,135 rets). Possible bug vs design split: `type_edge` does have a `field`
  kind. TRIAGE FIRST.
- No method-body-to-field-access join.
- No visibility/pub column on `call_def` or `type_entity`.

## Their verdict

Only the zero-existing-callers hollow-wrapper rail is tight enough to gate commits;
fan-out and condom-score queries are dashboard-only (noisy/retroactive).
