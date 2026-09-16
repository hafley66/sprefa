# feature/generic-text-door: parser spellings for the four list constructors

## Ruled by user 2026-08-10 (D4 = C, decision rows in conformance/rulings.pl:
## list_surface_named_constructors, list_bare_default_dense_owned_sequence,
## list_flavor_set_v1). This lane implements, it does not design.
The four constructors exist TERM-DOOR-ONLY (0_generic_expand.pl:41-42). Give
them the text door: `list(T)`, `list_entity_dense_sequence(T)`,
`list_interned_set(T)`, `list_entity_linked_sequence(T)` as column types in
.dl6 text, producing exactly the terms the engine already expands.

## Files owned by this lane
- v6/prolog/compile/parse_dl.pl: typed_column_type_base clauses (~:697-730).
  CAREFUL: `list` currently hits the removed_word(list) clause that points
  users at json_list — that clause DIES; bare `list(T)` now parses as the
  relational generic term list(T). json_list keeps its clause untouched.
  Element = any typed_column_type (primitives, idents/rel names, nested
  constructors); groundness/coherence stay the engine's job, the grammar
  stays permissive (the parse_dl.pl house rule).
- v6/prolog/print_dl.pl: print side round-trips the four spellings
  (roundtrip gate enforces).
- v6/dl/fixtures/golden-flex.dl6: a new GENERICS section following the
  file's existing style — one decl per constructor, arrivals + a
  retraction, and coverage-gate rows per the file's own conventions (read
  the header, :1-160, before writing anything; the file documents its own
  gate mechanism).
- New conformance/TEXT_DOOR fixtures for the four spellings incl one nested
  (list(list(text))) and one rel-element (list(some_rel)) IF the rel-element
  engine support has landed on main by the time you rebase; otherwise skip
  the rel-element fixture and note it in the commit body.

## Setup (REQUIRED; absolute cd every command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release
```

## Validation gate
```bash
cd <worktree>/v6 && just conformance
cd <worktree>/v6 && just plunit
cd <worktree>/v6 && just text-door
cd <worktree>/v6 && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck
cd <worktree>/v6 && just tsv2-test
cd <worktree>/v6 && just golden-flex
```
Manifest: new fixtures compiled, zero bucket flips on pre-existing ones.

## Commit rail (commit-or-report)
- Commit ON THE BRANCH, up to 3 commits, prefix `prolog:`.
- Blocked -> FAILURE-REPORT.md at worktree root with exact command + output,
  exit nonzero. NEVER --no-verify.

## Style laws
- Follow parse_dl.pl's existing DCG clause style exactly.
- Comments only constraints code cannot show; max 2 consecutive lines.
- Banned words prose+identifiers: provenance, substrate, load-bearing,
  regime, refusal.
