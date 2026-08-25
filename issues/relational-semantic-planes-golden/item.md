---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: flash4
status: open
priority: normal
epic: relational-semantic-planes
labels:
- area:tests
- intent:conformance
- size:small
- model:small
size: S
lane: semantic-ci
lane_seq: 10
collision: [type-emitters, ci-scripts, conformance-fixtures]
blocked_by: ['@sqlite-integrity-emitter', '@remove-temporal-suffix', '@quoted-sqlite-storage-names', '@csp-protocol-library', '@retire-type-specialcases']
---

# Add relational semantic plane goldens

## Description

Add one authored fixture that crosses the Type, Integrity, Flow, Materialization, and target-plan boundaries and proves compiler-row erasure.

## Fixture Coverage

- Relation and primitive type nodes.
- Type edges and relation applications.
- Composite primary and alternate unique integrity.
- Event occurrence, state replacement, retention, and history.
- B, N, Z, boundary, carry, and delayed flow evidence.
- Logical materialization and quoted SQLite target names.
- CSP protocol evidence selected by the protocol card.

## Acceptance Criteria

- [ ] Prolog compiler and reference runtime execute the fixture.
- [ ] SQLite enforces integrity and retention.
- [ ] Rust program compiles and executes.
- [ ] ProgramJson and generated schema snapshots retain intended identities.
- [ ] Compiler-only rows create no runtime tables.
- [ ] Full compiler suite and relevant typegen goldens pass.

## Tests Run

Record exact declared, passed, failed, and target counts. TypeScript-v2 tests are excluded.

## Implementation Notes

Execution tier: Small, size `S`, model `small`. Flash4 maximum thinking through a Boop OpenCode lane; completion notification and independent review required. This is the final epic gate.
