---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: luna
status: open
priority: normal
epic: dl7-engine-adapter
labels: [dl7, model-luna]
lane: dl7-layout
lane_seq: 1
collision: [v7-layout, v7-test]
size: M
blocked_by: ['@dl7-layout-planner']
---

# Write DL7 ProgramJson engine artifact

## Description

Serialize the bounded V7 layout into the existing ProgramJson contract and wrap it in the generated Rust module text consumed by run::load_program.

## Signatures

layout_to_program_json(+Layout, -ProgramJson).
write_program_module(+ProgramJson, -ModuleText).

## Instance lifetimes

Layout and ProgramJson live for one compilation artifact. Module text survives on disk or in a temporary smoke path. The Rust loader owns the decoded GenProgram lifetime.

## Storage, reads, writes, uniqueness

Emit ir_version 1 from one V7 constant. Emit every required ProgramJson field exactly once. Module text contains one PROGRAM_JSON raw string constant. This card writes no runtime database.

## Acceptance Criteria

- [ ] ProgramJson contains all fields required by the existing Rust serde type.
- [ ] ir_version is 1 from one V7 source.
- [ ] Output order and escaping are deterministic.
- [ ] Existing run::load_program accepts the wrapper.
- [ ] No V6 parser terms or emitter module are imported.
- [ ] No Rust engine source changes.

## Tests Run

- [ ] Exact ProgramJson and module snapshots in the existing V7 test file.
