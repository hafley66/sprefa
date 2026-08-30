---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
size: M
epic: dl7-programmable-compiler
lane: dl7-programmable-compiler
lane_seq: 2
blocked_by: ['@dl7-generated-rules']
closed: 2026-08-30
closed_by: codex
commits:
- hash: 67ab6c44b
  summary: Admit generated DL7 programs
---

# Refreeze generated programs

## Description

Extend compiler stability to type edges, intern requests, generated definitions, and generated rules. Execute frozen generated rules in the following round and retain checked generated program metadata in runtime output. Model class: medium.

## Acceptance Criteria

- [x] Generated relations and rules participate in compiler stability.
- [x] Round N generated rules execute during round N+1.
- [x] Runtime output retains generated declarations, rules, dependencies, and strata.
- [x] The existing 16-round compiler bound covers generated program refreeze.

## Tests Run

- [x] SWI suite: 15 of 15 passed.
- [x] Tree-sitter corpus: 1 of 1 passed.

## Implementation Notes

Implemented in `2_compiler.pl` and committed as `67ab6c44b`.
