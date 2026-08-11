# feature/list-of-rel-refs: rel types as the list element

## Ruled by user 2026-08-10 ("yes please reasonably handle rels in list 1st
## arg that way"): the relational list(T) generic accepts a rel type as its
## element. Member storage follows the ALREADY-SHIPPED shapes, no new design:
- The list side: the flavors machinery from PR #136 (0_generic_expand.pl):
  list entity + member table, INTEGER surrogate keys.
- The element side: exactly how a direct rel-typed column stores today
  (golden-flex.dl6:21-27, `patch(label: text, at: plot)`, `tree(site: patch)`;
  type plane relation_field_object). The member's value column is typed as
  the target rel, spelled and stored the same as `at: plot`.
- json_list(SomeRel) stays refused: the JSON carrier holds values, not rel
  identity. The throw at v6/prolog/0_type_plane.pl:115-120
  (`list_of_relation_refs`) stays for the json_list path; it must NOT fire
  for the relational list path.

## The work
1. 0_generic_expand.pl: element resolution accepts a rel type name (any
   declared rel usable as a column type today). Minted member table's value
   column carries that rel type. Applies to all four constructors from
   PR #136 (list, list_entity_dense_sequence, list_interned_set,
   list_entity_linked_sequence). For list_interned_set with a rel element,
   the dictionary indirection is NOT needed (the rel id IS the interned id);
   if that combination turns out incoherent, name it a checked error with a
   conformance fixture instead of forcing it.
2. Arrivals: an arriving list column whose elements are relation-shaped
   objects posts each element into its own rel and stores the id in the
   member row — same decomposition the runtime does for direct rel-typed
   columns (find the oracle-side equivalent in 0_type_plane.pl's
   canonicalize/field paths and follow it).
3. Fixtures (conformance, fail-first where possible):
   - a green fixture: `rel squad(members: list(fighter_summary))` with
     `rel fighter_summary(name: text, url: text)`; arrivals over 3 ticks
     including a retraction; oracle final state shows member rows holding
     fighter ids and fighter rows landed in their own rel.
   - nested: list(list(fighter_summary)) minted through the finding-2
     fixpoint.
   - the existing `list_of_relation_refs_still_refused` fixture stays RED
     for json_list (it guards the json path); add a comment-free sibling
     name if its current spelling now hits the relational path.
4. plunit: expansion byte-determinism under decl permutation for the
   rel-element case (extend the existing permutation test pattern).

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
```
Manifest: new fixtures compiled; zero bucket flips on pre-existing ones.

## Commit rail (commit-or-report)
- Commit ON THE BRANCH, up to 3 commits, prefix `prolog:`.
- Blocked -> FAILURE-REPORT.md at worktree root, exact command + output,
  exit nonzero. NEVER --no-verify.

## Style laws
- Comments only constraints code cannot show; max 2 consecutive lines.
- Banned words prose+identifiers: provenance, substrate, load-bearing,
  regime, refusal.
- INTEGER surrogate keys everywhere; no TEXT keys in minted DDL.
