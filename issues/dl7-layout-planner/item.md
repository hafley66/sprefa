---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: terra
status: open
priority: normal
epic: dl7-engine-adapter
labels: [dl7, model-terra]
lane: dl7-layout
lane_seq: 0
collision: [v7-layout, v7-test]
size: M
blocked_by: ['@dl7-compiler-split']
---

# Plan one DL7 relation into a layout graph

## Description

Build the minimum target-neutral physical plan for one declared stored relation and its authored seed rows. Keep SQL dialect data at the adapter boundary and keep SQLite names out of the logical type and rule graphs.

## Signatures

build_layout(+CheckedDatalog, -Layout, -Diagnostics).

## Instance lifetimes

Checked logical rows live through compilation. The immutable layout exists after checks and before target writing. Runtime table contents remain engine-owned.

## Storage, reads, writes, uniqueness

One logical relation identity maps to one physical relation plan. Ordered colon edges supply columns and types. Key metadata, table identity, DDL, arrival statements, and boundary statements occur once per relation. The first slice supports one base relation and no rule edge.

## Acceptance Criteria

- [ ] Input is V7 checked_datalog only.
- [ ] Output has explicit relation, column, type, key, DDL, arrival, and boundary fields.
- [ ] One base relation with one seed row plans deterministically.
- [ ] Unsupported recursive or edge programs produce a named diagnostic.
- [ ] Layout vocabulary contains no V6 parser term.
- [ ] No Rust, TypeScript, or V6 file changes.

## Tests Run

- [ ] One exact layout snapshot in the existing V7 test file.
