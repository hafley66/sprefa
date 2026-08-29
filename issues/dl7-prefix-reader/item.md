---
created: 2026-08-28
updated: 2026-08-29
type: task
assignee: codex
status: done
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-glm53f, model-codex]
size: M
lane: dl7-reader
lane_seq: 0
collision: [v7-reader]
closed: 2026-08-29
commits:
- hash: 0a477a098
  summary: finish root datum reader
- hash: f9dc96cd0
  summary: order compiler source by dependency
---

# Read the bounded DL7 prefix surface

## Description

Read `.dl7` characters before SWI applies Prolog capitalization rules. Bare
identifiers remain names at every capitalization. `?Name` creates a logical
variable identity shared within one top-level form. Quoted symbol data,
strings, integers, comments, nested forms, spans, and deterministic
diagnostics use the same reader path for files and SWI quasi quotations.

## Signature

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics).
```

## Acceptance Criteria

- [x] PascalCase and lowercase identifiers both read as `atom(Name)`.
- [x] `?Name` identities share within one top-level form and `?_` stays fresh.
- [x] Prefix forms, literals, comments, escapes, and complete spans are retained.
- [x] Files and quasi quotations share one text-to-unit pipeline.
- [x] Reader modules import no DL6 declaration or statement dispatcher.
- [x] Production code lives in `v7/src/0_reader/` in dependency order.
- [x] Existing tests remain consolidated under `v7/test/`.

## Tests Run

- [x] The six reader and entrypoint tests pass in one focused SWI command.

## Implementation Notes

The reader emits ground compiler data with explicit variable identities. SWI
variables are introduced later by `v7/src/1_libtime/0_evaluator.pl`.

## Resolution

### 2026-08-29T04:05:35Z · @issuectl

The bounded reader, file loader, quasi quotation path, spans, diagnostics, and six focused tests satisfy the reconciled reader card.
