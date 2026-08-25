---
created: 2026-08-25
updated: 2026-08-25
type: task
status: in-progress
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:small
- model:small
assignee: codex
---

# Remove the type.member plane argument

## Description

## Description

Collapse the compiler type-member view to one canonical five-column relation. The compiler fixpoint currently emits only `logical` rows, while `member_plane_rows/3` constructs `storage(sqlite)` rows for one test and has no production caller.

## Required Signature

```dl6
type.member(Member, Owner, Position, Name, Target).
```

Functional dependencies:

```text
Member -> Owner, Position, Name, Target
(Owner, Position) -> Member, Name, Target
```

## Timeline

Canonical semantic members freeze, the compiler exposes one unwrapped target ID per member, user DL6 rules query those rows, generated type requests refreeze through the existing boundary, and compiler rows erase before runtime planning.

## Storage and Uniqueness

This relation creates no runtime table. Physical storage projection remains a later target-plan operation keyed by `MemberId`. Delete the unused plane adapter and its `canonical_storage_backend(sqlite)` selector while retaining target-neutral `storage_relation`, `storage_column`, and `storage_key` rows.

## Acceptance Criteria

- [ ] `type.member/5` returns the unwrapped canonical target ID.
- [ ] `type.member/6` and its plane key are removed.
- [ ] Every DL6 type operator uses the five-column relation.
- [ ] `member_plane_rows/3` and `canonical_storage_backend/1` are removed.
- [ ] Storage projection and target lowering retain their existing outputs.
- [ ] Compiler member rows erase before runtime planning.
- [ ] Focused and complete Prolog gates pass.

## Tests Run

Pending implementation.

## Implementation Notes

Size `S`, model `small`. Implement directly because this worktree already owns the compiler and type-operator files. sprefa-extract and `rg` identified seven affected source/test files.
