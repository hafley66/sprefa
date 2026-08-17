# DL6 `_id` access first slice

Implement the smallest sound slice of:

- `/Users/chrishafley/projects/sprefa/v6/plans/2026-08-17-relation-value-identity-access.md`

Read `AGENTS.md`, the plan, `v6/README.md`, the real DL6 golden syntax, dot/member
expansion, type plane, list expansion, and lowering before editing. Use the
existing authored syntax. Do not invent `file(File)` row binding if it does not
exist.

Scope:

1. Expose the stored typed integer for a direct relation-valued column through
   a collision-checked generated `<column>_id` accessor.
2. Expose the stored list-container integer for a direct `list(T)` column through
   the same convention if the current member syntax supports it cleanly.
3. Preserve the ordinary followed-value access path.
4. Keep IDs nominal in compiler IR. Do not serialize a bare integer without its
   target type.
5. Reject `_id` on scalar columns and reject authored-column collisions with a
   source-located compiler error.
6. Add generated SQL goldens proving identity-only access adds no target join,
   followed access adds one target join, and requesting both shares one join.
7. Add parser/resolver/compiler tests using actual authored `.dl6` syntax.

Boundaries:

- Do not implement persistent/tombstoned key maps.
- Do not change option representation.
- Do not add `ref(T)` or `embed(T)` surface syntax.
- Do not edit unrelated catalog counts or baseline failures.
- Run focused gates, `git diff --check`, and inspect every changed file.
- Commit one coherent change with `Refs-Issue: @relation-id-access` if all
  acceptance checks pass. Otherwise leave the worktree uncommitted and report
  the exact missing primitive with file and line evidence.

Report authored examples, compiler IR before/after, emitted SQL, tests, commit,
and remaining unsupported cases.
