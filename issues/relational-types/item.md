---
created: 2026-08-18
updated: 2026-08-18
type: epic
status: done
priority: high
labels:
- area:dl6
- intent:type-system
closed: 2026-08-18
closed_by: codex
commits:
- hash: dca9e788a
  summary: lower interface bounds through compile relations
- hash: 710eef0e5
  summary: add relation result arrow sugar
---

# Relational interfaces and result arrows

## Description

Plan: plans/2026-08-18-relational-interfaces-and-result-arrows.md

## Acceptance Criteria

- [x] Interface syntax lowers through compile-time relations.
- [x] Relation arrow syntax uses generic Result(E, T).
- [x] Existing compiler and cross-target type-generation CI passes.

## Implementation Notes

Two independent implementation cards. Keep runtime storage unchanged for equivalent explicit relation forms.

## Resolution

### 2026-08-18T14:32:57Z · @codex

Both child cards integrated and their combined compiler and golden coverage CI passes.
