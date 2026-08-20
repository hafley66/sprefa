---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: luna
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
lane: type-core
lane_seq: 0
collision: [generic-type-core]
closed: 2026-08-18
---

# Repair interface-bound typegen transport

## Description

Reconcile pending 6b08 interface-bound work. Preserve complete ordered application patterns through `typegen_export`, define the temporary target representation of a direct wildcard, add real DL6 fixture coverage, and compile emitted TS/Rust artifacts. Module-qualified semantic IDs belong to the immediately following `semantic-type-identity` card.

## Acceptance Criteria

- [x] Complete ordered bound patterns cross `typegen_export` as `type_pattern(constraint_id, ordinal, value)` rows.
- [x] Direct Prolog and DL6 renderers emit equivalent TS/Rust constraints.
- [x] `any` is legal only as one direct bound argument and has defined TS/Rust output.
- [x] A real generic application in a `.dl6` fixture exercises wildcard proof.
- [x] Generated TypeScript and Rust compile.

## Tests Run

## Implementation Notes

Start from commit `6b08b83134aa27f8fba2a1b574fa83cc78a3732b`; treat it as a bound-pattern prerequisite, not a general compiler-relation engine. Direct wildcard target spelling is TypeScript `any` and compiler-owned Rust marker `Any`; matching remains compiler-side existential evidence. `semantic-type-identity` replaces raw term identity after this transport is stable.

## Comments

### 2026-08-18T19:14:14Z · @codex

CI: renderer tests 8/8; DL6 TS/Rust renderer programs compile; wildcard refusals 2/2; generated Rust wildcard artifact cargo check passed. Full compiler suite ran 820 tests with 6 unrelated current-main failures. Typegen golden could not open its sandbox socket.
