---
created: 2026-08-25
updated: 2026-08-25
type: task
status: done
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:small
- model:small
assignee: codex
closed: 2026-08-25
closed_by: codex
commits:
- hash: 26312ab0a
  summary: collapse type member to canonical relation
---

# Remove the type.member plane argument

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

- [x] `type.member/5` returns the unwrapped canonical target ID.
- [x] `type.member/6` and its plane key are removed.
- [x] Every DL6 type operator uses the five-column relation.
- [x] `member_plane_rows/3` and `canonical_storage_backend/1` are removed.
- [x] Storage projection and target lowering retain their existing outputs.
- [x] Compiler member rows erase before runtime planning.
- [x] Focused and complete Prolog gates pass.

## Tests Run

- Focused compiler relation, type graph, userland operator, and storage projection suites: 65/65 passed.
- Complete Prolog suite: 1111/1111 passed in 18 seconds.
- `git diff --check`: passed.

## Implementation Notes

Size `S`, model `small`. sprefa-extract and `rg` identified the direct source callers; the semantic expectation update brought the implementation to eight files. Implementation commit: `26312ab0a`.

## Resolution

### 2026-08-25T13:09:39Z · @codex

Implemented the canonical type.member/5 source, removed the plane-bearing source and adapter, updated all DL6 consumers, and passed 65 focused plus 1111 complete Prolog tests.
