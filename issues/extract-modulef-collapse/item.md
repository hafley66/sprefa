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

# ModuleF is collapsed and flagged for human review

## Description

## needs-chris

Design decision on the plane roster. No lane.

## Description

`ModuleF` is written down as a sketch and commented out. The resolution half
was folded into SCIP namespace edges and the binding half into aux metadata,
by an agent, and the file itself flags the call for human review.

## Receipts

| fact | receipt |
|---|---|
| the collapse, the sketch, and the flag | `v6/sprefa-extract/src/types.rs:629-645` ("PENDING - collapsed; not yet a family ... ADDENDUM 4a RULING: phase-1 specifier rows live in `CallFAux.specifiers` ... ModuleF stays collapsed. Flagged for human review; revival stays possible") |
| the plane roster still lists it | `v6/sprefa-extract/src/types.rs:17-19` |
| the resolve surface deliberately declares no arm | `v6/sprefa-extract/src/types.rs:1084-1088` |
| where the binding half actually lives | `v6/sprefa-extract/src/types.rs:497-503` (`CallFAux.specifiers`) |
| what v5 spends on the plane instead | `src/engine/family/mod.rs:397-408` — ten relations, incl. four `module_binding` shapes and `crate_edge` |

## The consequence, measured

@extract-module-plane-non-ts is blocked behind this in shape though not in
sequence: the per-language specifier emission is the same work either way, but
where the resolved edges LAND differs. Today a cross-file import edge is a
`file_edge` record (`src/schema.rs:36`) with a symbol count and no kind, where
v5 distinguishes import / edge / unresolved / binding / crate_edge.

## Forks, decided by nobody

| fork | shape |
|---|---|
| A. keep collapsed | specifiers stay `CallFAux` aux; `file_edge` stays the only module-level output; v5's five distinctions never come back |
| B. revive `ModuleF` | the sketch at `types.rs:637-645` becomes real; a fifth plane, its own Resolve arm, its own wire records |
| C. middle | no new family, but the `file_edge` record grows a `kind` column carrying v5's import/unresolved/binding distinction |

## Comments

### 2026-08-16T19:52:24Z · @chris

Deferred 2026-08-16: no call yet. User wants the port census absorbed first; the fold stands meanwhile. extract-module-plane-non-ts holds with it.
