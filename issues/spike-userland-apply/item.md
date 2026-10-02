---
created: 2026-10-02
updated: 2026-10-02
type: task
status: open
priority: normal
epic: emitter-v1-burndown
related: ['@spike-application-context']
labels: [spike]
---

# Spike: product application defined in userland

## Description

User 2026-10-02: can product itself be defined in userland, since applying a product uses one type-system reading of its edges? Split: kernel keeps edges, rounds, call, and minting an id from contents (intern, compiler-only). Userland defines apply: (1) check each parameter's edge target through call ((?System.check Argument Bound ?Checked)), (2) kernel mints the instance id, (3) fill instance edges from the template's return binds over edge_snapshot. Different type systems = different userland apply. Cost: every prelude generic build becomes a call, +1 comptime round. Sketch in chat 2026-10-02 (applied / bound_fails / instance_of).
