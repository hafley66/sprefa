---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
---

# df aux: df_field and df_lit across four langs

## Description

## Description

`DfFAux` carries `params` and `args` only. v5's dataflow family carries five
more label facets; two of them (`df_field`, `df_lit`) are per-node labels with
no graph consequence and are the cheapest half.

## Receipts

| fact | receipt |
|---|---|
| v6 aux | `v6/sprefa-extract/src/types.rs:540-543` (`DfFAux { params, args }`) |
| v5 roster | `src/engine/family/mod.rs:475-491` `DATAFLOW_RELS` (15 rels) |
| v5 rel docs | `src/engine/family/mod.rs:455-474` (`df_field` = named value flow into a composite; `df_lit` = one row per string-carrying df_node, kind lit/template/concat) |
| v6 deferral notes | `lang/ts.rs:1828` ("the string text is deferred `lits` aux"), `lang/ts.rs:1861` and `lang/rust.rs:1427` ("field names are deferred `fields` aux"), `lang/go.rs:19-20`, `lang/kotlin.rs:28` |
| ledger | `tests/golden_parity.rs:22-24` DEFERRED v5-only |

## Fix shape

1. `DfField { node: Span, name: NameId }` and `DfLit { node: Span, text: String, raw: String }`
   on `DfFAux`.
2. Two `FlatFact` arms + two `SCHEMA` lines, following `record=arg` /
   `record=param` (`src/schema.rs:29-30`).
3. Per-lang emission at the sites already marked deferred above: ts, rust, go,
   kotlin. Emit at the SAME walk that already builds the node, never a second pass.
4. Flip the `df aux (fields/lits/loops/nests)` row at `src/types.rs:1836` to
   `fields/lits` done.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 2_df_aux_cli
cargo test --features cli --test golden_parity
```

## Comments

### 2026-08-16T17:29:18Z · @extract-closeout-driver

Oracle rows already committed and byte-gradable: 8 df_lits and 8 df_fields across v6/sprefa-extract/tests/fixtures/*/*.v5.jsonl. Count them with: cat v6/sprefa-extract/tests/fixtures/*/*.v5.jsonl | cut -f1 | sort | uniq -c. The parity leg is a normalize arm in golden_parity.rs plus a narrowed per-facet test, the same move the docs facet used in PR #304.

### 2026-08-16T21:32:36Z · @coordinator

FLAG FOR CHRIS (scope expansion, merged): the lane flipped df_field/df_lit into the global PORTED list with a normalize arm mapping v6 push order to v5 dense node index — an asserted v5/v6 index-order coupling nobody decided. Green across 9 fixtures / 16 oracle rows today. Loosening is a one-line PORTED revert if you want the coupling gone.

## Decisions

### 2026-08-16T23:24:05Z · @chris

PORTED order-coupling declined ('idgaf about v5'): df_field/df_lit dropped from golden_parity PORTED, content grade in 18_df_aux_fields_lits stands. Pushed direct to main ac86acb32.
