---
name: reference_scip_name_not_dl_split
description: "SCIP descriptor-name extraction belongs in-engine (scip_name relation), NOT a pure-dl split chain — measured 42% wrong on real rust-analyzer monikers"
metadata: 
  node_type: memory
  type: reference
  originSessionId: 7322e02c-67ee-4fd7-8304-4c7ef80db5d0
---

`scip_name(symbol, name)` relation added (engine.rs, commit d132951) surfaces the
existing `scip_descriptor_name` helper (last identifier run) as a query relation.

The dead-end it replaced: extracting a SCIP symbol's descriptor name with a pure-dl
`split` chain (`split(split(sym,"/",-1),"(",0)`). MEASURED against a real
`rust-analyzer scip` index over v5 (2513 defs): **1059 = 42% wrong**. Methods and
trait impls carry `impl#[Type]` / `for#[Type]` segments and members are `Type#member`;
a single-separator `split` can't strip `#`/`[`/`]`, and term descriptors keep a
trailing `.`. dl `split` is the wrong tool for SCIP descriptor grammar.

Do NOT re-attempt the dl-split approach. The descriptor logic lives in Rust
(`scip_descriptor_name`, engine.rs ~575) where the moniker grammar belongs.

Machine-checked by `tests/it/scip_name.rs` (runtime-skips without rust-analyzer):
real RA index over `tests/fixtures/scip_names` (free fn, inherent method, trait
impl, field, variant) — every name reduces to a bare identifier.

Untested edge: scip-typescript overload disambiguators (`name(2).` would make
last-ident-run return `2`). RA emitted none in 2513 symbols; needs
`@sourcegraph/scip-typescript` to confirm on the TS half. Powers the cross-lang
flow page (`bench/flow/flow_scip.dl`, see [[project_v5_dl_engine]]).
