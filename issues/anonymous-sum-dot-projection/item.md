---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: codex
status: in-progress
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

- [ ] `A.x` projects to the canonical anonymous sum.
- [ ] Variant siblings resolve through dot paths.
- [ ] Authored nested `rel A.x` collisions have deterministic diagnostics.
- [ ] Unrelated declaration insertion cannot change identity.
- [ ] Generic substitution and recursive paths remain deterministic.
- [ ] Existing anonymous sum storage and typegen artifacts remain valid.

## Tests Run

Anonymous syntax/value tests, dot-reference matrix, and cross-target type generation.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by `@userland-dot-projection`.

## Decisions

### 2026-08-25T13:36:23Z · @codex

Implementation contract: preserve anonymous(Owner, SitePath, Shape) identity and all generated storage names. Before strict qualified-type resolution, derive rel_path_decl aliases only for directly declared member sums whose internal type paths resolve against authored declarations. Add aliases for OwnerPath + SitePath and each generated variant. Project type.path rows by prefixing every anonymous SitePath with each declared owner path, then append variant labels. Reuse existing path-collision validation. No undeclared relation inference and no emitter-specific behavior.
