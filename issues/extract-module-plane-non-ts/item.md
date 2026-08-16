---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:large
blocked_by: ['@extract-modulef-collapse']
---

# module plane beyond TypeScript

## Description

## Description

The module plane exists for TypeScript only. v5 resolves modules for five
languages and emits ten module relations; v6 emits `Specifier` rows from the ts
front-end alone and resolves them with a ts-only diet resolver.

## Receipts

| fact | receipt |
|---|---|
| v5 rels | `src/engine/family/mod.rs:397-408` `MODULE_RELS` (module_import, module_edge(+_rev), module_unresolved(+_rev), crate_edge, module_binding x4) |
| v5 resolvers | `src/graph/modgraph/{rust,ts,go,kotlin,python}.rs`, contract at `src/graph/modgraph/mod.rs:1-15` |
| v6 diet resolver is ts-only, and says so | `v6/sprefa-extract/src/deps.rs:1-2`, extension table `:57-70` |
| only ts emits specifiers | `grep -rn Specifier v6/sprefa-extract/src/lang/` hits `lang/ts.rs` only (`:892-1010`) |
| v6 has no ModuleF | `v6/sprefa-extract/src/types.rs:629-645` (collapsed, commented out) |
| `crate_edge` has no v6 analog | no hit for `crate_edge` anywhere under `v6/sprefa-extract/src` |

## Fork, for the record

Whether the module plane becomes a family or stays specifier-rows-plus-a-
resolver is @extract-modulef-collapse, which is Chris's call. THIS issue is the
per-language SPECIFIER EMISSION and diet resolution, which is the same shape
either way: rust `use`/`mod`/`#[path]`, go imports, kotlin package/import,
python import/from-import become `CallFAux.specifiers` rows
(`src/types.rs:497-503`), and `deps.rs` grows a per-language resolution policy
beside the ts one.

Size is large; split per language when dispatching, rust first (v5's
`src/graph/modgraph/rust.rs` is the one with an oracle test,
`tests/oracle_rust.rs`).

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 7_diet_deps_cli
```
