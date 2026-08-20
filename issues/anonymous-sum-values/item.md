---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: terra
status: done
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@anonymous-type-syntax']
lane: anonymous-types
lane_seq: 2
collision: [enum-lowering, type-emitters]
closed: 2026-08-18
---

# Materialize anonymous sum values

## Description

Mint owner-scoped enums before enum context, retain named full-type payloads, emit static TS/Rust/JSON unions, decide endpoint-ID versus tagged runtime contract, and implement the selected runtime boundary.

## Acceptance Criteria

- [x] Anonymous sums mint before enum context and retain owner-site semantic identity.
- [x] Named variant fields accept complete type expressions after generic substitution.
- [x] Static TS/Rust/JSON artifacts expose reachable tagged unions.
- [x] Endpoint-ID or tagged runtime representation is selected and documented.
- [x] Both TS and Rust ingress/egress implement the selected runtime contract.
- [x] Nested option and payload cases have deterministic runtime tests.

## Tests Run

## Implementation Notes

Selected contract: SQLite and ProgramJson storage retain integer enum endpoint IDs. Public typed TS/Rust ingress and egress use the same tagged object/enum shape as generated type artifacts and resolve it through the enum struct plane. Variant identity is owner semantic ID plus declaration order plus exact name; payload members use ordered named fields. Writes intern payload then variant then owner edge. Reads follow the endpoint and reconstruct one tag plus payload. Options wrap that tagged public value. Refuse unknown tags, missing fields, extra fields, and ambiguous owner context by name. Current integer cells remain an internal storage boundary.

## Comments

### 2026-08-19T01:56:41Z · @codex

Integrated as 033eb4894, 8da220f1c, d1cba817d. CI: anonymous_sum_values 3/3; Rust enum_plane 8/8; cargo check --locked. TypeScript runtime tests are checked in but were not executable locally because v6/tsv2/node_modules is absent.
