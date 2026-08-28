---
created: 2026-08-25
updated: 2026-08-25
type: bug
status: closed
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

## Root Cause

Two independent gaps in `tree-sitter-dl6/grammar.js`.

1. `path` spelled its dot as a separate `"."` token, so a clause-terminating
   dot could extend a path across a newline into the next clause's head. A
   bare relation constant such as `widget` was also missing from `expression`,
   so `shape(widget).` already recovered through an `ERROR` node and a second
   fact turned the whole rest of the file into one `ERROR`.
2. Every declared name slot accepted only the lowercase `identifier` token,
   while `parse_dl_dcg.pl` `ident//1` starts on any alpha or underscore, so
   `Type: type` columns and `rel Partial(...)` heads did not parse. The
   `rel name(...) -> type.` arrow return was also absent.

## Acceptance Criteria

- [x] Repeated DL6 facts retain later call-family nodes and sites.
- [x] `0_operators.dl6` emits all 8 `type.member` call sites.
- [x] A regression fixture covers facts followed by rules.
- [x] Existing Prolog and DL6 extraction tests pass.

## Tests Run

`tree-sitter-dl6/test/corpus/statement_sequence.txt` and
`test/corpus/declared_names.txt` pin the repeated-fact sequence, dotted call
paths, and variable-cased declared names. `cargo test --features cli` in
`v6/sprefa-extract` is green. Over the 129 `.dl6` files under `v6/dl/**` and
`v6/prolog/conformance/**` the `ERROR` cst-node count fell from 3,423 to 331
with zero files regressing.

## Implementation Notes

Size `S`, model `small`. Inspect DL6 tree-sitter recovery and the call-family top-level traversal around repeated fact clauses.
