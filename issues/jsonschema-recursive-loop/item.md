---
created: 2026-08-20
updated: 2026-08-20
type: bug
status: testing
priority: high
labels: [compiler]
---

# 4_emit_jsonschema.pl loops on recursive-enum fixtures; bounded_emit is containment only

## Description

## Comments

### 2026-08-20T16:05:19Z · @jsonschema-rail-fix

Fixed on fix/jsonschema-loop-and-rail, PR https://github.com/hafley66/sprefa/pull/385. Commits 484f8fb7f (fix) and bab631960 (report), base 3993e44aa.

RCA: a recursive enum types its own variant's field, so 4_emit_jsonschema.pl's kind_schema/7 enum arm re-entered enum_schema/4 forever. 7_emit_ts_types.pl and 8_emit_rust_types.pl never looped on the same catalog rows because they NAME the type (`left: Tree`).

Fix: recursive_enum_row/2 detects an enum reachable from itself and renders it as one `$defs` entry plus a `$ref` per occurrence. A bottoming-out enum still inlines, so no other fixture's schema.json changed a byte.

Measured: 10334ms and 10412ms (both cut by the 10s alarm) to 12ms and 13ms; SWEEP_EMIT_TIMEOUT lines 2 to 0. The committed schema.json files were from the pre-feature era (28ec02ef8's types.ts said `left: number`); the regenerated ones agree with types.ts and types.rs field for field.

Fail-first: plunit wrapper_composition:recursive_enum_column_renders_a_named_ref_and_terminates FAILED (5.216 sec) with throw(time_limit_exceeded) before the fix. docs/failure-modes.md 58 closed.
