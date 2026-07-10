---
name: feedback-lowering-vs-runtime-separation
description: Lowering (OperatorDef) and runtime (Component) stay in separate file trees in v4; do not colocate them per-op
metadata: 
  node_type: memory
  type: feedback
  originSessionId: ad1ddc5a-c1d4-44c0-8d82-0ea3992f8c76
---

Lowering and runtime are two distinct layers in v4 and must live in separate
file trees. `OperatorDef` (compile-time lowering) belongs under
`v4/src/compile/lower/`. `Component` (runtime execution) belongs in a sibling
tree (today `v4/src/v2_ops.rs`, eventually a split runtime/components tree).
Do not propose colocating `Def` + `Component` in one file per op.

**Why:** User flagged on 2026-05-20 when reviewing the refactor-audit plan's
Phase 6, which had proposed `v4/src/compile/lower/ops/<name>.rs` carrying
both the Def and the Component. Their architectural intent: compile-time and
runtime are separate concerns; the registry is the coupling point (op-name
key), not the filesystem.

**How to apply:** Any refactor that touches per-op organization should split
the lowering side and the runtime side independently, mirrored by op-name on
both sides. A Def file imports nothing from a Component file. When sketching
file layouts in plans for [[project_genericization_initiative]] follow-ups,
the v2_ops rename, or trait-collapse work, propose two parallel trees, not
one. Phase 2 of the refactor plan (trait surface collapse on `OperatorDef`)
stays entirely on the lowering side.

Related: [[project_genericization_initiative]] is what "moved the graph up"
into effect_runtime; the Def/Component split is the analogous compile/runtime
split inside v4 itself.
