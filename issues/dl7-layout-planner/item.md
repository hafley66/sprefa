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
lane_seq: 1
collision: [v7-layout, v7-test]
size: M
blocked_by: ['@dl7-compiler-split', '@dl7-layout-rulings']
---

# Plan one DL7 relation into a layout graph

## Description

Build the minimum target-neutral layout for one declared stored relation and
its authored seed rows. Keep target names, DDL, and statements at the adapter
boundary.

## Signatures

build_layout(+CheckedDatalog, -Layout, -Diagnostics).

## Instance lifetimes

Checked logical rows live through compilation. The immutable layout exists after checks and before target writing. Runtime table contents remain engine-owned.

## Storage, reads, writes, uniqueness

One logical relation identity maps to one layout relation. Ordered colon edges
supply columns and semantic types. The selected layout policy supplies artifact
role, encoded representation, and key metadata. The first slice supports one
base relation and no rule edge.

## Acceptance Criteria

- [ ] Input is V7 checked_datalog only.
- [ ] Output has explicit semantic relation identity, artifact role, ordered
  columns, semantic types, encoded representations, keys, and authored seeds.
- [ ] One base relation with one seed row plans deterministically.
- [ ] Unsupported recursive or edge programs produce a named diagnostic.
- [ ] Layout vocabulary contains no V6 parser term, target table name, DDL, or
  engine statement.
- [ ] No Rust, TypeScript, or V6 file changes.

## Tests Run

- [ ] One exact layout snapshot in the existing V7 test file.

## Blocker evidence

Status remains open. `checked_datalog/4` supplies relation arity, ordered
colon edges, ground seeds, rules, dependencies, and strata. It supplies none
of the required physical rows below:

| Required layout field | Missing signature | Competing choices |
| --- | --- | --- |
| relation kind | `relation_storage_kind(+Relation, -Kind)` | `set`; `log`; explicit stored declaration classifier |
| key indices | `relation_key_indices(+Relation, -KeyIndices)` | `[]`; all columns; explicit authored positions |
| artifact role | `layout_artifact(+Relation, -ArtifactRole)` | current state; event/log; history; transient frontier |
| encoded representation | `layout_column_representation(+SemanticType, -Representation)` | target-neutral scalar, reference, list, and type-ID representations |

ProgramJson still requires table names, boundary types, DDL, arrival SQL, and
seed placement. Those fields belong to `@dl7-program-json-rulings` and
`@dl7-program-json-writer`. Detailed sources and the exact existing engine
fields: `v7/tasks/results/10_LAYOUT_BLOCKER.md`.
