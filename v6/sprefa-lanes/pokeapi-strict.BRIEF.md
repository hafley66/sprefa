# feature/pokeapi-strict: converter strict mode — G1 rerun after fix A

## Context
The forward converter (PR #154, v6/tsv2/scripts/openapi_to_dl6.ts) fell back
to `json` carriers for 201 properties (gap G1 in
v6/dl/fixtures/POKEAPI_ROUNDTRIP_REPORT.md): list(rel_name) columns and
inline-object lifts hit column_type_unknown through generic expansion on the
dense ref web. THAT BUG WAS THE DECL-ORDER MSORT, fixed and merged as
PR #157 (fix A). Your base has the fix.

## The work
1. Rerun the converter in strict mode (the mapping's real spellings:
   list(rel_name) for arrays of refs, minted parent__prop rels for inline
   objects, per the mapping table in sprefa-lanes/openapi-to-dl6.BRIEF.md).
   If the script has no strict/safe flag, add one; safe mode stays the
   default only if strict still hits a live compiler error.
2. gen/pokeapi_gen.dl6 must compile (compile_dl6.sh exit 0). Each property
   that STILL cannot take its mapped spelling gets its exact compiler error
   and throw site in the report; zero silent fallbacks.
3. Rerun openapi_roundtrip_check; update POKEAPI_ROUNDTRIP_REPORT.md. The
   kind table's known-gap count must drop from 201; state the new number in
   the commit message. Nullable properties now count as matches (anyOf emit
   landed, #153). G2 (option(list(T)) nullable arrays) stays a named known
   gap: the user has not ruled a spelling; drop nullability on those, log
   per property.
4. Emit-back receipt: definitions count and a spot-check that a formerly
   json-carrier property (e.g. an array-of-refs column) now round-trips as
   an array of $ref items.

## Files you own
- v6/tsv2/scripts/openapi_to_dl6.ts, openapi_roundtrip_check.*
- v6/tsv2/gen/pokeapi_gen.dl6, v6/dl/fixtures/POKEAPI_ROUNDTRIP_REPORT.md
- the hand mapping fixture + its test if strict mode changes their shape
No .pl files. A compiler error is REPORTED with its throw site, never
worked around in .pl code.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6/tsv2 && bash scripts/openapi_roundtrip_check.* (exit code is the gate)
cd <worktree>/v6 && just typecheck && just tsv2-test
```

## Rails
- NEVER git merge / pull / rebase in the worktree.
- Blocked -> FAILURE-REPORT-STRICT.md, exact command + output, exit
  NONZERO. rc=0 with a dirty tree or red gates is a defect.
- NEVER --no-verify. Up to 2 commits, prefix `tsv2:`. Comment budget: max 2
  consecutive comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
