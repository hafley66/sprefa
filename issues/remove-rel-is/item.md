---
created: 2026-08-23
updated: 2026-08-23
type: feature
status: done
priority: normal
assignee: codex
closed: 2026-08-23
closed_by: codex
---

# Remove relation conformance is syntax

## Description

Remove the declaration suffix `rel R(...) is Interface.` and its
`rel_is_implementation/2` compiler representation. Applicative compiler
relations in ordinary type position are the annotation surface.

This issue retains `interface` declarations and generic parameter bounds such
as `T: Bound`. It also retains the unrelated live `is/2` binding operator from
the expression registry.

## Compiler Signatures

```prolog
%! rel_stmt_in(+Prefix, -Decls, -Sites)// is semidet.
%  A concrete relation ends after modifiers at `.` or a nested declaration
%  block. There is no conformance suffix production.

%! semantic_type_rows(+Decls, -Rows) is det.
%  Produce declaration, parameter, member, constraint, application, and
%  annotation rows without implementation rows.

%! catalog_type_metadata_rows(+Decls, +ModuleId, +RelMap, +ListMap,
%!                            +Id0, -Rows, -IdFinal) is det.
%  Emit interface and generic metadata without implementation catalog rows.
```

Pseudo-code:

```text
parse relation columns
parse arrow and modifiers
parse declaration terminator

freeze semantic type rows without rel implementation evidence
validate interface declarations and generic bounds
lower interface and generic metadata without implementation rows
```

## Instance Timeline

The removed suffix previously created one compile-time implementation fact per
interface application. Those facts lived through type-row freeze, conformance
validation, and catalog lowering, then disappeared before runtime. Removal
eliminates that compiler-lifetime fact stream. Type-position compiler relation
applications continue through annotation elaboration and retain their existing
site evidence.

## Storage, Reads, Writes, and Uniqueness

- No runtime table or durable row is added.
- Interface implementation catalog rows are removed.
- Interface declarations, generic declarations, parameters, constraints, and
  concrete generic-instance metadata keep their existing identities.
- Applicative annotation evidence remains keyed by member site and sequence.

## Acceptance Criteria

- [x] Authored `rel R(...) is Interface.` no longer parses.
- [x] The parser emits no `rel_is_implementation/2` term.
- [x] The printer has no relation-conformance suffix path.
- [x] Semantic type freeze emits no implementation rows.
- [x] Generic validation no longer reads direct implementation evidence.
- [x] Catalog lowering emits no implementation metadata rows.
- [x] Interface declarations and generic `T: Bound` syntax still parse, print,
      and emit their declaration and constraint metadata.
- [x] Applicative compiler-relation annotations such as `key(int)` remain green.
- [x] The unrelated expression registry entry `is/2` remains live.
- [x] Full compiler PLUnit and typegen golden gates pass.

## Tests Run

- `just --justfile v6/justfile plunit`: 1106 passed, 0 failed.
- Focused type/interface/annotation/compiler-relation slice: 232 passed,
  0 failed.
- `just --justfile v6/justfile typegen-golden`: `TYPEGEN GOLDEN: HOLDS`.
- `just --justfile v6/justfile golden-flex`: conformance coverage requirement
  removed. The full gate retains two merged-main baseline failures:
  `arrival_identity/2` is absent from the golden fixture, and `codec(any)`
  reaches runtime emission as a malformed code list.

## Implementation Notes

Worktree: `/private/tmp/sprefa-remove-rel-is`

Branch: `feature/remove-rel-is`

Base: merged `main` at `a6294d1e4`.

The merged main worktree has overlapping uncommitted changes in
`v6/prolog/0_generic_expand.pl` and `v6/prolog/lower.pl`; this removal remains
isolated on its branch until those changes are reconciled.

## Resolution

### 2026-08-24T03:48:04Z · @codex

Removed the relation conformance suffix, implementation semantic rows and IDs, direct conformance proof selection, and implementation catalog rows. Retained interfaces, generic bounds, applicative annotations, and expression is/2. PLUnit passes 1106/1106; TYPEGEN GOLDEN holds.
