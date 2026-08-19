---
created: 2026-08-19
updated: 2026-08-19
type: improvement
status: open
priority: high
labels: [compiler, sqlite, performance]
---

# Share SQLite frontier state across relations

## Description

Replace relation-specific transient SQLite plans with shared frontier/support state while retaining one typed durable table per materialized relation. Plan: plans/2026-08-19-shared-sqlite-frontier.md

## Acceptance Criteria

- [ ] Each materialized DL6 relation owns one typed durable SQLite table.
- [ ] Frontier and support state use shared runtime tables keyed by relation and row IDs.
- [ ] Rust and TypeScript load the same compact program-data contract.
- [ ] Rule-free programs contain zero relation-specific frontier or rule plans.
- [ ] Old and compact paths produce equal tick, final, replacement, retraction, recursion, and restart results.
- [ ] PokeAPI CI reports artifact bytes plus compile, load, and first-tick wall times.

## Tests Run

- [ ] TypeScript old-versus-compact runtime matrix
- [ ] Rust old-versus-compact runtime matrix
- [ ] PokeAPI 212-schema roundtrip
- [ ] Full compiler CI

## Implementation Notes

Keep the legacy lowering available until the compact path passes the behavioral matrix in both engines. No authored DL6 syntax or type semantics change in this card.
