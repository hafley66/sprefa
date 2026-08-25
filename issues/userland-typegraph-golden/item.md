---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: flash4
status: open
priority: normal
epic: userland-type-graph
labels:
- area:tests
- intent:conformance
- size:small
- model:small
size: S
lane: typegraph-ci
lane_seq: 10
collision: [type-emitters, ci-scripts, conformance-fixtures]
blocked_by: ['@anonymous-sum-dot-projection', '@sqlite-constraint-emitter', '@remove-temporal-suffix', '@quoted-sqlite-storage-names', '@retire-type-specialcases']
---

# Add cross-target user-land type graph goldens

## Description

Add one authored cross-target golden covering the complete user-land type graph path and compiler-row erasure.

## Fixture Coverage

- Brace and dot paths with explicit parent columns where needed.
- Anonymous member sum through `A.x`.
- Serializable, Partial, extends, impl, and concat rules.
- Composite primary and alternate unique constraints.
- Call-form temporal retention.
- Quoted SQLite physical names containing dot and hyphen.

## Acceptance Criteria

- [ ] Prolog compiler and reference runtime execute the fixture.
- [ ] SQLite enforces constraints and retention.
- [ ] TypeScript and Rust programs compile and execute.
- [ ] ProgramJson, JSON Schema, and type snapshots retain intended identities.
- [ ] Compiler-only rows create no runtime tables.
- [ ] Full compiler suite and typegen golden pass.

## Tests Run

Record exact declared, passed, failed, and target counts.

## Implementation Notes

Execution tier: Small, size `S`, label `size:small`. Flash4 maximum-thinking Boop OpenCode lane with completion hail. This is the final epic gate.
