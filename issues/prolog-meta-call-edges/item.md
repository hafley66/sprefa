---
created: 2026-08-28
updated: 2026-08-28
type: bug
status: fixed
priority: normal
labels:
- sprefa-extract
- size:med
closed: 2026-08-28
closed_by: codex
commits:
- hash: aec4ca9a1
  summary: 'extract: resolve Prolog meta-call edges'
---

# Resolve Prolog meta-call edges

_Source: v7/0_SWIPL/5_driver.pl_

## Description

## Observation

Running sprefa-extract over `v7/0_SWIPL/*.pl` with:

```text
extract --resolve --family call v7/0_SWIPL/1_reader.pl v7/0_SWIPL/2_expand.pl v7/0_SWIPL/3_quasi.pl v7/0_SWIPL/4_loader.pl v7/0_SWIPL/5_driver.pl
```

emits 101 direct `resolved_edge` rows and resolves the cross-file path
`driver_exit_code/2 -> load_dl7/3 -> dl7_text_unit/5 -> read_dl7/5`.

The graph stops at calls represented as callable arguments of Prolog
meta-predicates:

- `main/1` calls `driver_exit_code/2` through `catch/3` argument 1.
- `read_dl7/5` calls `read_top_forms/5` through `once/1`.
- `expand_dl7/6` calls `expand_nodes/3` through `once/1`.

The extractor already emits these as `reference` rows with
`position: term_arg`, but emits no corresponding call edge. A hop query from
the executable entrypoint therefore terminates early.

## Acceptance Criteria

- [x] A derived program or extraction fact represents statically callable
      Prolog meta-predicate arguments for at least `once/1` and `catch/3`.
- [x] The DL7 SWI fixture resolves continuously from `main/0` through
      `read_top_forms/5` and `expand_nodes/3`.
- [x] Ordinary non-callable term arguments remain excluded from call edges.
- [x] Tests cover direct goals, nested `once/1`, and `catch/3` goal/recovery
      arguments.

## Tests Run

- [x] Direct five-file `extract --resolve --family call` reproduction.

## Implementation Notes

The extractor boundary says language semantics can remain a Datalog program
over emitted facts. The existing `reference(position=term_arg)` rows may be
sufficient input if meta-predicate argument positions are represented as data
above extraction.

## Comments

### 2026-08-28T23:51:24Z · @codex

Validated patched extractor across all five v7/0_SWIPL files: 105 resolved edges, 4 cross-file edges, and continuous entrypoint paths from main/0 through read_top_forms/5 and expand_nodes/3. cargo test --features cli passed the extractor and Prolog coverage; the unrelated tsc_is_clean_on_the_committed_tree test failed after 70.93s because npx attempted registry.npmjs.org with DNS unavailable.
