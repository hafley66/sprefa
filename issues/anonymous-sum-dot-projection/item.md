---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: codex
status: done
priority: normal
epic: userland-type-graph
labels:
- area:dl6
- area:parser
- intent:type-system
- size:med
- model:medium
size: M
lane: dot-path
lane_seq: 30
collision: [parser-type-expr, enum-lowering]
blocked_by: ['@userland-dot-projection']
commits:
- hash: a6a38ffc0ff22bb282f733a50e76d8e0a526a5d6
  summary: 'dl6: PASS - anonymous sums expose owner-qualified dot paths'
- hash: a32c0671fd0014f9b939184806660e00444897a4
  summary: 'issues: PASS - scope anonymous sum dot projection'
closed: 2026-08-25
closed_by: codex
---

# Lower anonymous member sums into dotted nested type paths

## Description

Give a right-hand-side anonymous sum an owner/member path so `A.x` and its variants are addressable without losing owner-scoped anonymous identity.

## Example

```dl6
rel A(x: (left(); right())).
```

The semantic path contains `A.x`, `A.x.left`, and `A.x.right`. The approved plan selects nested type edges over the anonymous enum ID or generated declarations. Existing anonymous storage remains authoritative.

## Acceptance Criteria

- [x] `A.x` projects to the canonical anonymous sum.
- [x] Variant siblings resolve through dot paths.
- [x] Authored nested `rel A.x` collisions have deterministic diagnostics.
- [x] Unrelated declaration insertion cannot change identity.
- [x] Generic substitution and recursive paths remain deterministic.
- [x] Existing anonymous sum storage and typegen artifacts remain valid.

## Tests Run

Anonymous syntax/value tests, dot-reference matrix, and cross-target type generation.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by `@userland-dot-projection`.

## Decisions

### 2026-08-25T13:36:23Z · @codex

Implementation contract: preserve anonymous(Owner, SitePath, Shape) identity and all generated storage names. Before strict qualified-type resolution, derive rel_path_decl aliases only for directly declared member sums whose internal type paths resolve against authored declarations. Add aliases for OwnerPath + SitePath and each generated variant. Project type.path rows by prefixing every anonymous SitePath with each declared owner path, then append variant labels. Reuse existing path-collision validation. No undeclared relation inference and no emitter-specific behavior.

### 2026-08-25T13:48:34Z · @codex

Regression revision: semantic aliases use resolver-only type_path_alias rows and are erased before runtime planning. rel_path_decl remains the authority for authored physical relation nesting. This preserves existing catalog and typegen names while retaining A.x and A.x.variant resolution.


## Agent Runs

### 2026-08-25T13:47:59Z · @codex

Implementation a6a38ffc0: resolver-only type_path_alias declarations support A.x and A.x.variant in type and rule positions, then erase before runtime planning. Canonical type.path rows use owner-qualified anonymous paths. Imported anonymous enums preserve module identity for variants. Post-expansion relation type mirrors use final stored columns. Verification: focused Prolog 77/77, new tests 5/5, full Prolog 1117/1117 in 20.3 seconds. No TypeScript test command was run.
