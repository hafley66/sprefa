---
created: 2026-08-25
updated: 2026-08-25
type: bug
status: open
priority: high
epic: extract-port-closeout
labels:
- pkg:extract
- area:dl6
- size:small
- model:small
---

# extract call family truncates after repeated DL6 facts

## Description

## Description

The standalone `extract --family call` CLI loses the remaining DL6 file after two consecutive facts for the same relation. The DL6 compiler accepts this source, and `rg` sees the later call site.

## Minimal Reproduction

```dl6
rel shape(Type: type).
rel seen(Type: type).

shape(widget).
shape(gadget).

seen(Type) <-
  shape(Type).
```

```text
extract --family call repeated-facts.dl6
```

Actual: zero `node` rows and zero `site` rows.

Expected: function nodes for `shape` and `seen`, plus a call site from `seen` to `shape`.

## Corpus Receipt

`v6/dl/type/0_operators.dl6` contains 8 `type.member(...)` calls by `rg`. The CLI emits 6 `site` rows with `callee_path = "type.member"`. Its last function node ends at byte 2,902 in a 4,679-byte file. The two recursive serializability calls at lines 102 and 123 are absent.

## Acceptance Criteria

- [ ] Repeated DL6 facts retain later call-family nodes and sites.
- [ ] `0_operators.dl6` emits all 8 `type.member` call sites.
- [ ] A regression fixture covers facts followed by rules.
- [ ] Existing Prolog and DL6 extraction tests pass.

## Tests Run

Standalone release CLI compared against exact `rg` occurrences and a reduced seven-line source.

## Implementation Notes

Size `S`, model `small`. Inspect DL6 tree-sitter recovery and the call-family top-level traversal around repeated fact clauses.
