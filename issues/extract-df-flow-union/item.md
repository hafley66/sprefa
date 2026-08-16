---
created: 2026-08-16
updated: 2026-08-16
type: task
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- needs-chris
---

# DfEdgeKind Flow union is commented out

## Description

## needs-chris

Language/type-system design. No lane. This is already an open row in
`CLAUDE.md` ("Interprocedural dataflow is unbuilt").

## Description

`DfEdgeKind` has one variant. The interprocedural union (arg to param, ret to
call_res, higher-order) is commented out, so every dataflow answer v6 gives
stops at a function boundary.

## Receipts

| fact | receipt |
|---|---|
| the enum | `v6/sprefa-extract/src/types.rs:605-612` — `Direct` only, `// Flow(FlowEdgeKind), // PENDING epic 5` |
| the plane note | `v6/sprefa-extract/src/types.rs:17-19` ("VALUE-FLOW (native): DfF (+ typed Flow* edges)", `* = pending, commented out`) |
| the raw material already exists | `DfArg` (`src/types.rs:542`) records which positional slot an argument feeds and `DfParam` records parameter positions, on all four langs (`src/types.rs:1835`) |
| the missing hop | nothing joins a `DfArg` at a call site to the `DfParam` of the resolved callee, though `resolved_edge` already carries caller site to callee (`src/schema.rs:38`) |

## Forks, decided by nobody

| fork | shape |
|---|---|
| A. Flow as a `DfEdgeKind` variant | one edge plane, kinded; a df closure walk crosses functions by default, which changes the meaning of every existing `closure(df_edge)` rule |
| B. Flow as a separate family | `FlowF`, its own plane, its own closure; existing df rules keep their meaning |
| C. Flow as a phase-2 join only | no new edge type; the join lives in dl, and the leaf just guarantees `DfArg`/`DfParam`/`resolved_edge` line up |

Fork C is the only one that adds no type. Whether that is a feature or a dodge
is the question for Chris.
