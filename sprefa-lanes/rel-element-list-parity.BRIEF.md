# fix/rel-element-list-parity: door parity + roundtrip for rel-element list spellings

## Ruled by user 2026-08-11: fix these bugs. Scope is parity and printing;
## the rel-element ENGINE path stays TODO (list_of_relation_refs_still_refused).

## The failures, measured on main d60d9990's ancestors
```bash
cd <worktree>/v6 && just roundtrip   # 3 FAIL fail(not_variant) in conformance/fixtures/10_list_elements.pl:
#   rel_element_list_round_trips, nested_rel_element_list_round_trips,
#   list_interned_set_relation_element_refused
cd <worktree>/v6 && just text-door   # column_type_unknown(fighter_summary) failures in the same family
```
Re-run both FIRST in your worktree to get the exact post-#149 counts (#149
added an enum-field walk to declared_column_type_name and already flipped one
of the text-door failures green).

## Diagnosis so far (verify, then fix)
1. text-door: a rel name as a list ELEMENT (`list(fighter_summary)`) reaches
   0_type_plane.pl column_storage via a path where no type_def exists for
   fighter_summary, so the error comes out column_type_unknown. The DELIBERATE
   error for this shape is list_of_relation_refs (0_type_plane.pl:119-121
   keeps it distinct on purpose; 0_program_check.pl:338-341 documents why).
   Term door and text door must throw the SAME named error; the fixtures pin
   it. Follow the #149 pattern (git show 4d5cf473, a 7-line
   declared_column_type_name clause) if the fix is collection; keep the
   list_of_relation_refs reason intact either way.
2. roundtrip: print_dl does not round-trip the rel-element list spellings
   byte-identically (fail(not_variant)). Find the print_dl.pl clause for
   generic list types and make the rel-element case print the exact source
   spelling.

## Files you own
- v6/prolog/compile/parse_dl.pl and print_dl.pl (the failing seams only)
- v6/prolog/conformance/fixtures/10_list_elements.pl (only if a pinned
  expectation is provably wrong; say so in the commit message)
- v6/prolog/compile/out/* regeneration artifacts
Nothing else. 0_type_plane.pl's distinct-reason design stays.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
text-door and roundtrip must go FULLY green; that is the whole point.
Manifest: zero bucket flips outside this family.

## Commit rail (commit-or-report)
Up to 2 commits, prefix `prolog:`. Blocked -> FAILURE-REPORT-LIST-PARITY.md,
exact command + output, exit nonzero. NEVER --no-verify. Pre-commit hook
fails >2 consecutive comment lines in any touched hunk; one-line comment
edits only.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
dl variable names descriptive, never single-letter. Follow each file's
existing style.
