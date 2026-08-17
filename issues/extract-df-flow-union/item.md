---
created: 2026-08-16
updated: 2026-08-17
type: task
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
closed: 2026-08-17
closed_by: extract-driver
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

## Decisions

### 2026-08-16T19:32:03Z · @chris

Fork B: cross-function value edges are a SEPARATE family (FlowF), own plane, own closure. Existing df rules keep their intra-function meaning; cross-wall walks opt in by naming the new family. Consistent with the CfgF-as-new-family lock (2026-08-16). Implementation dispatchable: glue = DfArg x resolved_edge x DfParam.

## Comments

### 2026-08-16T21:14:04Z · @coordinator

FLAG FOR CHRIS: shipped FlowEdgeKind has FOUR variants (ArgToParam, RetToCallRes, LambdaElem, LambdaRet), not the three in the decision note. Lane's evidence: sprefa-seed _3_extract/_0_shape.rs:141-148 already reserves this vocabulary; 'higher-order' maps to the two lambda hops. Only ArgToParam and RetToCallRes are produced today. Merged in PR #313; rename/veto is cheap while nothing downstream reads the lambda variants.

## Resolution

### 2026-08-17T04:56:49Z · @extract-driver

AUDITED against origin/main d1a5556b0, acceptance met under the 2026-08-16 fork-B decision (FlowF as its own family). Receipts: DfEdgeKind stays Direct-only with the pointer to FlowF (v6/sprefa-extract/src/types.rs:765-771, the 'Flow(FlowEdgeKind) // PENDING epic 5' line is gone); FlowF family + FlowEdgeKind (types.rs:788-816, four variants ArgToParam/RetToCallRes/LambdaElem/LambdaRet); the join flow_edges = DfArg x resolved call edge x DfParam plus callee Ret (types.rs:838-905, ArgToParam minted at :886, RetToCallRes at :905); wire record flow_edge family=flow (src/schema.rs:44, kind slugs :137); CLI door --resolve --family flow (src/bin/extract.rs:479, PR #330); tests tests/13_flow_join.rs (PR #313) and tests/23_flow_cli_dispatch.rs (PR #330), both in the 151/151 gate. Still open and NOT this card: only ArgToParam and RetToCallRes are produced, LambdaElem/LambdaRet are declared unproduced (coordinator flag 2026-08-16T21:14 stands, rename/veto is Chris's); the plane legend at types.rs:15 still carries the '(* = pending, commented out)' key with no starred entry, cosmetic leftover.
