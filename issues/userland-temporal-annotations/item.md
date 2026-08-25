---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: terra
status: open
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:temporal
- size:med
- model:medium
size: M
lane: temporal
lane_seq: 20
collision: [generic-type-core, compiler-oracle]
blocked_by: ['@temporal-v2-salvage', '@userland-constraint-graph', '@typed-annotation-corrections']
---

# Derive temporal relation schemas from user-land annotations

## Description

Replace `relation_kind_request/2` and `relation_keep_request/2` builtins with ordinary DL6 annotation and target-schema rows. Preserve temporal-v2 parity before old syntax removal.

## Provisional Outputs

```dl6
$storage.relation_kind(TargetId, log).
$storage.relation_keep(TargetId, all).
$storage.relation_keep(TargetId, count, Count).
```

`history(Target)` derives log plus keep-all. Generic targets use existing canonical application demand.

## Acceptance Criteria

- [ ] `log`, `keep`, and `history` are DL6 library relations and rules.
- [ ] Temporal request builtins are gone.
- [ ] Ordinary and generic targets lower through canonical IDs.
- [ ] Invalid targets, policies, and conflicts retain diagnostics.
- [ ] Call syntax matches legacy runtime declarations, SQL, SQLite, and timelines.
- [ ] Compiler-only retention variants create no runtime tables.

## Tests Run

Temporal test module, full compiler suite, typegen golden, SQLite retention.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by temporal salvage, constraint rows, and typed annotation corrections.
