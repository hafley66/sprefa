---
created: 2026-08-19
updated: 2026-08-21
type: improvement
status: open
priority: high
labels: [compiler, sqlite, performance]
related: ['@cheap-fast-analysis']
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

## Comments

### 2026-08-21T16:00:23Z · @chris

2026-08-21 measured on main: frontier(shared) removes 2 tables + 1 index per rel and 17-26% statements; wall on 54k rows 5.38/5.39/6.50s vs per_rel 5.39/5.27/5.28s, no win; 171/319 corpus programs compile under it (edge_rules 81, aggregate_head 43, non_set_rel 12, host 5, recursion 3, retention 2 stop). Parked.
