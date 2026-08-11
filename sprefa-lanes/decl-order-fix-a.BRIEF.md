# fix/decl-order-msort: stop sorting author decls in generic expansion (fix A)

## Ruled by user 2026-08-10: fix A. Column order in a rel declaration is the
## author's order, end to end. Generic expansion must stop scrambling it.

Read FIRST: FAILURE-REPORT.md at repo root (the full repro + root cause).

## The defect
`v6/prolog/0_generic_expand.pl:195` (`generic_artifact_order/3`) runs
`msort(Decls, Sorted)` when any generic instance is minted. msort reorders
`col_type/3` entries alphabetically within each rel. `0_type_plane.pl:
relation_columns_and_types/5` reads column order from decl-list position, so
every rel's columns scramble and `0_program_check.pl:
program_violation(relation_column_type_conflict,...)` fires a FALSE conflict
on unrelated columns.

Repro (exact, from FAILURE-REPORT.md):
```
rel plot(row: int, col: int).
rel patch(label: text, at: plot).
rel tree(tree_id: int, species: text, site: patch).
rel tree_label(tree_id: int, label: text).
tree_label(TreeId, Label) <- tree(TreeId, _Species, Site), decode(Site, {label: Label}).
rel box_list(tree_id: int, items: list(text)).
```
compile_dl6 output today:
`unsupported_construct(at('<prog>',5,relation_column_type_conflict(tree/3,site,patch,tree_label/2,label,text)))`

## The fix (A, exactly this)
1. Replace the msort at 0_generic_expand.pl:195 with an order-preserving
   arrangement: author decls keep their original relative order; minted
   generic-instance decls land in a DETERMINISTIC position derived from
   content (e.g. sorted among themselves, appended after the author decls or
   after the decl that minted them). Determinism must come from content,
   never from Prolog term order over author decls.
2. KNOWN LANDMINE (FAILURE-REPORT.md "Notes for the fix"): plunit test
   `generic_e2e_declaration_permutation_is_byte_deterministic` in
   v6/prolog/compile/test/plunit_tests.pl asserts expansion output is
   invariant under decl permutation INCLUDING within-rel col_type order.
   Under fix A that assertion is wrong by design: within-rel column order IS
   the program. Rewrite the test to assert (a) same input text -> same
   output bytes, (b) permuting WHOLE decls across rels is invariant,
   (c) permuting columns WITHIN a rel is a different program and the test
   must not require identical output for it.
3. Fail-first fixture: the repro above compiles green and `tree_label` rows
   come out correct (decode reads site by name). Add as a conformance
   fixture; also a TEXT_DOOR twin if the fixture set there covers generics.
4. golden-flex GENERICS section: add to v6/dl/fixtures/golden-flex.dl6 the
   section the text-door lane could not (FAILURE-REPORT.md lines 3-5): one
   decl per list constructor (list(T), list_entity_dense_sequence(T),
   list_interned_set(T), list_entity_linked_sequence(T)), arrivals + a
   retraction. Skip list(some_rel) elements (engine path still TODO,
   `list_of_relation_refs_still_refused`); nested list(list(text)) is fine.
5. Decision row in v6/prolog/conformance/rulings.pl (follow the file's row
   format): decl_order_fix_a, author column order is data; user 2026-08-10.
6. Delete FAILURE-REPORT.md at repo root once its content is superseded by
   the green fixture (the repro lives on as the fixture).

## Files you own (nothing else)
- v6/prolog/0_generic_expand.pl
- v6/prolog/compile/test/plunit_tests.pl (the one test + any new ones)
- v6/dl/fixtures/golden-flex.dl6 (GENERICS section only)
- new conformance/TEXT_DOOR fixture files
- v6/prolog/conformance/rulings.pl (one appended row)
- FAILURE-REPORT.md (delete, step 6)
Do NOT touch 0_type_plane.pl or 0_program_check.pl: fix A lives in the
expansion, the type plane keeps trusting decl order because fix A makes decl
order true again.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip && just golden-flex
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
Manifest: new fixtures compiled, zero bucket flips elsewhere.

## Commit rail (commit-or-report)
Up to 3 commits, prefix `prolog:`. Blocked -> FAILURE-REPORT-DECL-ORDER.md,
exact command + output, exit nonzero. NEVER --no-verify. Pre-commit hook
fails >2 consecutive comment lines in any touched hunk, including legacy
blocks: one-line comment edits only.

## Style
Comments state only constraints the code cannot show, max 2 consecutive
lines. Banned words, prose and identifiers: provenance, substrate,
load-bearing, regime, refusal. dl variable names descriptive, never
single-letter. Follow each file's existing style.
