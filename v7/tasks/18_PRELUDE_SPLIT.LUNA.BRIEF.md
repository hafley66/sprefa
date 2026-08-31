# Split the ordered DL7 prelude

## Goal

Implement issue `dl7-prelude-files` on the branch supplied by Boop. Commit the
result with subject `Split the ordered DL7 prelude`.

## Read first

- `AGENTS.md`
- `plans/2026-08-31-dl7-type-algebra.md`
- `v7/src/2_comptime/2_compiler.pl`
- `v7/prelude/0_types.dl7`
- `v7/test/1_entrypoints.test.pl`

## Required change

1. Replace the single hard-coded `0_types.dl7` loader with a deterministic,
   lexical load of every numbered `.dl7` file directly under `v7/prelude/`.
2. Split the existing prelude into a small ordered file set by dependency and
   reading order. Use numeric prefixes beginning at 0 and underscores.
3. Preserve the exact existing declarations and rules. This task introduces
   no type-algebra relation and no syntax.
4. Extend the consolidated test surface enough to prove ordering and complete
   loading. Do not create one test file per prelude file.
5. Update `v7/tasks/18_TYPE_ALGEBRA_PROGRESS.md` with the commit and test
   receipt.

## Constraints

- Use `apply_patch` for edits.
- Keep compiler paths independent of the caller working directory.
- Lexical order must be explicit in the implementation, not inherited from an
  unspecified filesystem enumeration.
- Preserve source-origin data enough for diagnostics to identify the combined
  prelude and program.
- Do not edit issue files. Issuectl runs on main.
- Do not touch type conformance, intersection, impl, or HistoryV1 behavior.
- Run focused V7 SWI and Tree-sitter tests. Report current results only.

## Completion

- Commit the implementation.
- Hail the parent through Boop with the commit hash, changed files, and test
  counts.

