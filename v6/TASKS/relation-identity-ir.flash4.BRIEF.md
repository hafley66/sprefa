# 041a Derive relation identity targets

## Read first

- `/Users/chrishafley/projects/sprefa/v6/plans/2026-08-17-relation-value-identity-access.md`
- `/Users/chrishafley/projects/sprefa/v6/plans/2026-08-17-relation-value-identity-access.limitations.md`
- `/Users/chrishafley/projects/sprefa-v6/issues/relation-identity-ir/item.md`
- `v6/prolog/0_type_plane.pl`
- `v6/prolog/0_generic_expand.pl`
- `v6/prolog/0_option_expand.pl`
- `v6/prolog/0_enum_expand.pl`
- `v6/prolog/lower.pl`

## Scope

Implement the compiler IR fact that identifies every relation used as a stored relation-valued target after generic, option, enum, and import expansion.

The language derives identity from relation-valued type edges. Do not add authored `entity`, `ref(T)`, or `embed(T)` syntax.

Cover:

- direct relation-valued columns
- `list(Relation)` minted member relations
- `option(Relation)` companion relations
- relation-valued enum payloads
- imported and module-qualified relation types

Exclude unrelated keyed arrivals, keyed edges, logs, and level relations when no relation-valued edge targets them.

## Required shape

Choose one canonical compiler predicate/fact with a nominal target identity. Reuse expanded compiler metadata and existing relation hashes. Do not infer from SQL table names or emitted JSON.

Add a focused refusal when wrapper expansion has erased the target without retained metadata. Do not guess.

## Verification

- Focused PlUnit fixtures for all five positive cases and the unrelated-keyed negative cases.
- A fresh compiler/IR dump assertion proving the nominal targets.
- `git diff --check`.
- Run the narrowest relevant existing compiler gate.

## Delivery

- Work only in the assigned worktree.
- Commit the completed slice with `Refs-Issue: @relation-identity-ir`.
- Report changed files, exact tests, failures, and commit hash.
- Stop and hail the coordinator if the existing expanded metadata cannot express one of the five cases without a semantic choice.
