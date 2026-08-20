---
created: 2026-08-19
updated: 2026-08-19
type: feature
status: done
closed: 2026-08-19
priority: high
epic: applicative-type-annotations
labels:
- area:language
- pkg:prolog
- pkg:tsv2
- pkg:engine-rs
blocked_by: ['@type-annotation-surface']
---

# Typed annotation evaluation and key bridge

## Description

Execute typed type applications through compiler relations, retain site
evidence, and lower key evidence through current key normalization. Plan:
`plans/2026-08-19-applicative-type-annotations.md`.

## Acceptance Criteria

- [x] Type applications require a first `type` input and final `return: type` output.
- [x] Each application returns exactly one type or raises a named diagnostic.
- [x] `key(Target)` evidence produces current key SQL behavior.
- [x] Compiler annotation transport is absent from runtime DDL and facts.

## Tests Run

- `compiler_relations`: 28/28.
- `annotation_surface`: 8/8.

## Implementation Notes

- `5f0fdc98a`: typed compiler evaluation, exact arguments, evidence, key bridge,
  wrapper outputs, and compiler transport erasure.
