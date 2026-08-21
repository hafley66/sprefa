---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

`--family cfg` is a per-file family on the CLI. It is not linked into the
in-process executor, so a `.dl6` program cannot ask for a control-flow graph.

## Receipts

| fact | receipt |
|---|---|
| the CLI accepts it | `v6/sprefa-extract/src/bin/extract.rs:530` `"cfg" => mask.cst = true` |
| the CLI emits it | `extract.rs:21-22` `cfg_bundle`, `flatten_cfg`; `:366-373` |
| the plane is fully tested | `v6/sprefa-extract/tests/17_cfg_first_plane.rs`, the whole edge set of one function per language asserted exactly |
| the in-process executor refuses it | `v6/sprefa-engine-rs/src/hosts.rs:1117-1121` "family `cfg` is not a known family; in-process families are cst, type, call, df, data" |
| `FamilyMask` has no `cfg` field | `v6/sprefa-extract/src/types.rs:1886-1892` |

Probed 2026-08-21:

```
extract --family cfg probe.rs
  {"record":"node","family":"cfg","span":{"start":0,"end":49},"kind":"entry","name":null}
  {"record":"edge","family":"cfg","kind":"next","from":{...},"to":{...}}
```

## The confusion this row prevents

`v6/dl/deadcode/dead-module-rail.dl6:39-40` declares `sh cfg_at(...)` and it
works. That host runs `--family call` and selects `record=cfg_scope`, which is
the cfg-GUARD predicate on a definition (`#[cfg(test)]`), not the control-flow
plane. Two different things wearing the same three letters.

## Fix shape

`cfg` is derived from the cst parse rather than carried in `FamilyMask`, so the
executor needs a `want_cfg` boolean beside `want_file_fact`, mirroring
`extract.rs:366-373`:

```rust
"cfg" => { mask.cst = true; want_cfg = true; }
...
if want_cfg {
    for fact in sprefa_extract::flatten_cfg(&sprefa_extract::cfg_bundle(&out)) { ... }
}
```

Plus a host name in `registry.pl` with the `(path, digest)` input contract the
other extract-shaped hosts use, and a `.dl6` fixture that reads a `cfg` row.

## Gate

```bash
cd v6/sprefa-extract && timeout 900 cargo test --release --features cli
# plus a new dl6 fixture declaring `sh cfg_plane_at(...)` and reading one edge
```
