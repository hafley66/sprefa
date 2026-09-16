# lab/list-flavors continuation: rebase + findings 1-2 (design already ruled)

## Ruled by user 2026-08-10 (implementing, not deciding)
- D4 = C: named constructors as the lab built them, NO options grammar now.
- Bare `list(T)` = relational dense+owned+sequence (the lab's default combo).
- `json_list(T)` is the inline-JSON spelling at every layer (landed PR #140,
  main 1d0e294a); `list(T)` no longer collides with anything.
- Ship the lab's four constructors: `list(T)`,
  `list_entity_dense_sequence(T)`, `list_interned_set(T)`,
  `list_entity_linked_sequence(T)`.

## The work, in order, on branch lab/list-flavors
Worktree already exists: <repo>/.boop-worktrees/lab/list-flavors (branch head
09da4004 carries the finding-3 dictionary fix; keep it).

1. REBASE lab/list-flavors onto origin/main (1d0e294a, the json_list term
   rename). Conflict rule, mechanical: main renamed the inline-JSON type term
   `list(X)` -> `json_list(X)` in prolog + fixtures + emitted catalog
   (`kind: "json_list"`); take main's spelling for every JSON-array site, keep
   every branch ADDITION (0_generic_expand.pl, new fixtures, plunit tests).
   Branch-side RELATIONAL generics keep the `list(` spelling; that is the
   point of the rename.
2. Finding 1 fix: `generic_type/1` in 0_generic_expand.pl must match bare
   `list(_)` so `col_type(box/1, items, list(text))` mints the relational
   dense+owned+sequence artifacts. Fail-first fixture: a bare list(text)
   column whose minted rel exists and round-trips arrivals + retractions.
3. Finding 2 fix: instance discovery must reach fixpoint over MINTED decls,
   never the author decls alone: `option(list(list(text)))` mints the outer
   list AND the inner list (member column type resolves to the inner minted
   rel). Fail-first plunit: nested case pre-fix leaves `value:list(text)`
   unminted; post-fix both minted, expansion byte-deterministic under decl
   permutation (the existing permutation test must stay green).
4. Decision rows in v6/prolog/conformance/rulings.pl, following the file's
   existing row format, all attributed user 2026-08-10: (a) list generic
   surface = named constructors for now, options grammar deferred; (b) bare
   list(T) default = dense+owned+sequence relational; (c) list flavor set v1 =
   the four lab constructors.

## Setup (REQUIRED first; cwd resets between calls, absolute cd every command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release
```

## Validation gate (all green before commit)
```bash
cd <worktree>/v6 && just conformance
cd <worktree>/v6 && just plunit
cd <worktree>/v6 && just text-door
cd <worktree>/v6 && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts   # sweep deletes it, known defect
cd <worktree>/v6 && just typecheck
cd <worktree>/v6 && just tsv2-test
```
Manifest check: `git diff v6/prolog/compile/out/manifest.json` shows new list
fixtures compiled + zero bucket flips on pre-existing fixtures.

## Commit rail (commit-or-report)
- Commit ON THE BRANCH before exiting, up to 3 commits, prefix `prolog:`.
  Force-push the rebased branch (`git push --force-with-lease`).
- If blocked (rebase conflict you cannot resolve under the rule above, red
  gate), write FAILURE-REPORT.md at the worktree root with the exact failing
  command + output, and exit nonzero. NEVER --no-verify.

## Style laws
- Comments state only constraints code cannot show; max 2 consecutive comment
  lines in new code (hook-enforced).
- Banned in prose and identifiers: provenance, substrate, load-bearing,
  regime, refusal (say TODO / not built yet).
- All minted tables key on INTEGER surrogate ids; interned values live once in
  the UNIQUE dictionary (finding-3 fix on the branch is the precedent).
