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

# scip_occurrence and scip_binding outside the v5-vocab door

## Description

## needs-chris

Wire-vocabulary decision that was already made in writing once. Re-opening it
is Chris's call, not a lane's.

## Description

v5's SCIP relation set has ten members; v6's `--family scip` door carries eight.
`scip_occurrence` and `scip_binding` are excluded by a written decision in the
schema text.

## Receipts

| fact | receipt |
|---|---|
| v5's ten | `src/rels/scip.rs:41-50` |
| v5 `scip_occurrence` decl | `src/rels/scip.rs:77-82` (file, symbol, line, col, end_line, end_col, role, repo) |
| v5 `scip_binding` decl | `src/rels/scip.rs:83-88` (file, symbol, local_name, line, col, repo) — "an occurrence's LOCAL binding text (source slice at its range)" |
| v6's eight, and the exclusion | `v6/sprefa-extract/src/schema.rs:160-173` |
| the stated reason | `schema.rs:168-171`: `scip_occurrence` is already a record tag on the passthrough wire with different fields, and two shapes under one tag is the drift the goldens exist to stop |
| the passthrough row that answers it | `schema.rs:44` `record=scip_occurrence` under `--scip-facts` — carries spans and every role bit |

## Verdict per row

**`scip_occurrence`: doc close, no code owed.** The passthrough row is a
superset of v5's columns (byte spans instead of line/col, every role bit
instead of one `role` string). A consumer converts spans to line/col from the
file bytes, which is the same conversion v6 makes everywhere else
(`src/types.rs:30-34`: "Byte offsets into the file; line/col derived, never
stored").

**`scip_binding`: the door does NOT fully answer it.** `schema.rs:172-173`
concedes the point: "scip_binding additionally wants the source slice at those
spans." Nothing on the wire carries source text at an occurrence span, so the
join the schema text describes cannot be completed by a consumer reading the
JSONL alone. It needs the corpus bytes beside it.

## Fork, decided by nobody

| fork | shape | cost |
|---|---|---|
| A. leave it | a `scip_binding` consumer reads the corpus itself | the wire stops being self-contained for this one question |
| B. slice on the passthrough row | `record=scip_occurrence` grows a `text` field, populated only when a flag asks for it | one field, but it widens an existing tag |
| C. a real `scip_binding` record under `--family scip` | v5 vocabulary restored exactly | a ninth record tag, and the alias/default-import parse v5 does |

v5's stated purpose for the row is the alias hop: `import { foo as bar }`, where
`scip_name`'s canonical-only name drops `bar` (`src/rels/scip.rs:88`). Whether
v6 owes that answer at all is the actual question.
