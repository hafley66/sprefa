---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: S
---

# ProgramJson ir_version on both doors; delete incremental_safe fossil

## Description

## Description
`ProgramJson` has no version field and `incremental_safe: true` is a fossil (`emit_rust.pl:587`, kept only because `program.rs` deserializes it). A binary built by one dl6c must refuse an IR from another.
## Acceptance Criteria
- [ ] `ir_version: <int>` emitted by both emitters (`emit_rust.pl`, `emit_ts.pl`) and read by both runtimes; mismatch = named error at boot on both doors, test each.
- [ ] `incremental_safe` deleted from emitter and both deserializers; sweep shows every fixture identical except that key.
- [ ] `docs/failure-modes.md` entry (no incident yet; the rail).
