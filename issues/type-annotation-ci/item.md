---
created: 2026-08-19
updated: 2026-08-19
type: task
status: done
closed: 2026-08-19
priority: high
epic: applicative-type-annotations
labels:
- area:testing
- pkg:prolog
- pkg:tsv2
- pkg:engine-rs
blocked_by: ['@type-annotation-eval']
---

# Applicative type annotation cross-target CI

## Description

Authored cross-target golden and implementation review for applicative type
annotations. Plan: `plans/2026-08-19-applicative-type-annotations.md`.

## Acceptance Criteria

- [x] Authored DL6 covers plain, direct, configured, and nested type applications.
- [x] Existing key spelling and `key(T)` have equivalent SQL behavior.
- [x] Prolog, TS + SQLite, Rust + SQLite, ProgramJson, JSON Schema, and generated type CI execute.
- [x] Scalar and relation-valued `key(option(T))` execute replacement and stale retraction.
- [x] Review findings are corrected before close.

## Tests Run

## Implementation Notes
