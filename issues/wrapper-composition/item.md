---
created: 2026-08-15
updated: 2026-08-15
type: task
reporter: fable
status: open
priority: high
epic: type-plane-design
labels:
- size:large
- area:design
- needs-chris
- pkg:prolog
---

# Wrapper composition: option(option(T)), option(<enum>), absence-vs-null, one session

## Description

The three open type-plane items are one design: option(option(T)) thrown at 0_option_expand.pl:74, option(<enum>) stopped at 0_option_expand.pl:43, and the COLUMN plane cannot say key-absent vs present-null (4_emit_jsonschema.pl:121 papers over it). A top-level enum rel already works as a named type (probed, compiles rc=0). Deliverable: decisions in rulings.pl + the lowering for each accepted composition. Lang design lands with Chris in the room.
