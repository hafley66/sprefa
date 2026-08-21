---
created: 2026-08-19
updated: 2026-08-20
type: task
status: testing
priority: high
epic: productionize-rust-door
size: S
---

# ProgramJson ir_version on both doors; delete incremental_safe fossil

## Description

## Description
`ProgramJson` has no version field and `incremental_safe: true` is a fossil (`emit_rust.pl:587`, kept only because `program.rs` deserializes it). A binary built by one dl6c must refuse an IR from another.
## Acceptance Criteria
- [x] `ir_version: <int>` emitted by both emitters (`emit_rust.pl`, `emit_ts.pl`) and read by both runtimes; mismatch = named error at boot on both doors, test each.
- [x] `incremental_safe` deleted from emitter and both deserializers; sweep shows every fixture identical except that key.
- [x] `docs/failure-modes.md` entry (no incident yet; the rail).

## Comments

### 2026-08-20T16:05:19Z · @jsonschema-rail-fix

Reverted by 65607a8d5, restored on fix/jsonschema-loop-and-rail (PR https://github.com/hafley66/sprefa/pull/385, commit 484f8fb7f).

Every AC on this card was checked while the code was gone: `grep -c ir_version v6/prolog/emit_ts.pl` was 4 at 942cf1443 and 0 at 3993e44aa, emit_rust.pl the same, and program.rs carried neither IR_VERSION nor try_from_json. The consumers stayed: irVersion.ts enforces RUNTIME_IR_VERSION=1 at serve/0_compile.ts:125, and build_template/main.rs plus three test files still import program::IR_VERSION, so `cargo check --all-targets` in sprefa-engine-rs did not compile at 3993e44aa.

Restored: ir_version(1) and the emission site in both emitters; ProgramJson.ir_version (serde default) and GenProgram.ir_version, IR_VERSION, IrVersionMismatch, try_from_json in the Rust runtime. tests/fixtures/resident-coroutine.program.rs regenerated so the Rust guard deserializes.

Guard added (nothing pinned the STAMP): plunit incremental_mode:both_doors_stamp_the_ir_version_the_runtimes_interpret. Fail-first at base: `Unknown procedure: emit_ts:ir_version/1`. docs/failure-modes.md 57 now carries the revert incident.
