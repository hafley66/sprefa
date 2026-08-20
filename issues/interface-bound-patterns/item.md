---
created: 2026-08-18
updated: 2026-08-19
type: feature
assignee: luna
status: done
priority: high
epic: relational-types
labels:
- area:dl6
- intent:type-system
- codex
closed: 2026-08-19
---

# TypeScript-shaped interface bounds

## Description

Plan: plans/2026-08-18-typescript-shaped-interface-bounds.md

Implement `T: json_encodable(any)` as an interface application constraint. `T` is the implementing type; `any` is a wildcard interface argument. Preserve exact argument matching, compiler-only `$type` proofs, and runtime erasure.

## Acceptance Criteria

- [x] Bound syntax parses and round-trips bare, exact, and wildcard interface applications.
- [x] Full ordered interface arguments survive normalized constraints, implementations, IDs, diagnostics, and type-artifact metadata.
- [x] Exact arguments, wildcard arguments, arity errors, missing evidence, and repeated-subject refusal have deterministic tests.
- [x] Distinct applications of one interface coexist for one implementing type.
- [x] Generated TypeScript and Rust express parameterized interface bounds.
- [x] Compiler-local wildcard and proof rows never enter runtime SQLite, boot data, host plans, or Differential Dataflow inputs.

## Tests Run

## Implementation Notes

Terra review requires application identity to include interface name, arity, and ordered arguments. Structural JSON evidence is `json_encodable(json)`; `any` may match evidence but cannot manufacture it.

## Comments

### 2026-08-19T12:53:30Z · @codex

CI: rel_template_and_is_clause 52/52; emit_type_renderers 8/8; typegen_golden.sh passed cross-target Prolog, TSV2, Rust, and JSON Schema after rerun outside the socket-restricted sandbox.
