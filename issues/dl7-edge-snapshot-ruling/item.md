---
created: 2026-08-29
updated: 2026-08-29
type: task
status: needs-info
priority: high
epic: dl7-minimal-kernel
labels: [dl7, needs-ruling]
size: S
lane: dl7-kernel
lane_seq: 2
collision: [v7-design]
---

# Resolve source and generated edge strata

## Description

## Description

Resolve the relation-level stratum boundary between source type edges and
generated type edges before userland Pick and Exclude rules land.

Current Pick and Exclude ranking reads checked `':'/4` rows, derives a strict
count rank, then writes generated `':'/4` rows. The dependency graph is:

```text
':' output -> rank -> selected edge -> ':' input
```

Because count dependencies have strict gap 1, the checked stratifier reports
aggregate recursion. Owner values differ at runtime, but Datalog strata are
computed per relation identity.

## Type signatures

The decision must select an exact checked term contract for one of these
boundaries:

```prolog
source_edge(+Owner, +Name, +Target, +Index).

freeze_type_edges(+CheckedGraph, -FrozenEdgeRows).
refreeze_type_edges(+GeneratedRows, -NextCheckedGraph).

seed_and_derived_relation(+RelationRef, +SeedRows, +Rules).
```

## Instance timelines

Option A exposes an immutable checked source-edge snapshot before userland
rules execute. Option B adds a freeze, evaluate, discover, and refreeze round
between source and generated edges. Option C gives seed rows and derived rows
of one relation different aggregate-read semantics inside one evaluation.

## Storage, reads, writes, uniqueness

The selected representation remains target-neutral compiler data. Source edge
rows keep the settled `(Owner, Name)` and `(Owner, Index)` keys. Generated
colon rows still pass final closure key validation. No physical table, SQL,
or emitter contract belongs in this ruling.

## Required rulings

- [ ] Select immutable source-edge rows, freeze/refreeze rounds, or exact
  seed-versus-derived semantics for one relation.
- [ ] Specify whether kernel signature edges participate in the frozen input
  edge relation.
- [ ] Specify the checked row identity and functional keys.
- [ ] Specify when generated colon rows become visible to another type
  operator in the same compilation.

## Acceptance Criteria

- [ ] Pick and Exclude have an acyclic checked dependency graph under the
  selected contract.
- [ ] Partial's current positive closure remains unchanged.
- [ ] The contract contains no target or emitter vocabulary.

## Tests Run

- [ ] Run no implementation suite until the ruling is selected.

## Implementation Notes

Blocker discovered after `@dl7-count-aggregate` closed. The kernel correctly
rejects the unseparated `':' -> aggregate rank -> ':'` relation cycle.
