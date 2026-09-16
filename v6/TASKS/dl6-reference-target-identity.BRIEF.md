# DL6 relation-reference target identity discriminator

Work in the Boop-provided worktree. Read these exact absolute files first:

- `/Users/chrishafley/projects/sprefa/TASKS/dl6-reference-target-identity.BRIEF.md`
- `/Users/chrishafley/projects/sprefa/v6/plans/2026-08-17-relation-value-identity-access.md`

Then inspect the corresponding tracked files in your worktree:

- `v6/prolog/0_type_plane.pl`
- `v6/prolog/lower.pl`
- `v6/prolog/compile.pl`
- relation-value and list conformance fixtures

The language rule is fixed:

```text
a relation used as a relation-valued column has a stored integer reference
```

Do not propose or add `entity`, `ref(T)`, `embed(T)`, log, or keep surface
syntax. A relation remains a relation. Identity behavior is derived from the
existing type graph.

Task:

1. Trace `reference_target_ref` and every equivalent predicate/IR fact.
2. Determine whether relation targets reached through `ref(T)`, `list(T)`,
   option companions, and enum payloads can be marked as identity targets from
   existing declarations.
3. Define the smallest compiler IR fact, preferably derived rather than
   authored, that distinguishes these targets from unrelated keyed arrivals and
   keyed edges.
4. If current metadata is sufficient, implement only that derived IR marker and
   focused tests proving:
   - direct relation-valued columns mark the target
   - unrelated keyed arrivals and keyed edges do not
   - list(relation) marks the element target and list container separately
   - imported relation types resolve to the correct target
5. Stop before persistent storage or `_id` accessor changes.

Use exact paths. If tool context drifts to `/home`, another repository, 2023
plans, or `.boo-worktrees`, stop immediately and report failure.

Run focused tests and `git diff --check`. Commit with
`Refs-Issue: @relation-reference-identity` only if the derived marker is proven.
Report changed files, exact IR fact, tests, commit, and remaining wrapper gaps.
