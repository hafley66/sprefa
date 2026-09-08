---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: codex
status: untriaged
priority: normal
provenance: codex
source_ref: falcon-gdscript-extraction-2026-09-08
---

# Add GDScript source extraction support

## Description

## Request

Add GDScript support to extract. First inspect existing grammar registration and older implementations to establish what is already supported; reuse available parser integrations.

## Real input

Repository hafley-rs: games/blender-godot-sqlite-proof/falcon-lab/godot/2_stage.gd. Commit 4496f1e contains three repeated mesh-upload/acknowledgment blocks; commit 88767b1 consolidates them.

## Acceptance Criteria

- [ ] Discover .gd files and report parser capability and unsupported families explicitly.
- [ ] Emit source-identified CST facts with byte spans for the real Falcon script.
- [ ] Cover functions, local bindings, member calls, indentation-based blocks, and typed declarations with deterministic fixtures.
- [ ] Expose the CST to structural clone detection, preserving normalization and approximation evidence.
- [ ] Document syntax-only resolution limits without claiming compiler-backed type resolution.

## Related context

@extract-slow-diagnostics contains the CST similarity request and the possible historical distance/measure implementation to investigate.

## Implementation Notes

Feature request only. Installed GDScript grammar coverage has not yet been verified.
