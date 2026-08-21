---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

The file-to-file and package-to-package edge planes are CLI-only. v5's whole
module graph (10 relations) has no dl6 spelling for 7 of them.

## Receipts

| v5 rel | v5 site | v6 record | v6 site | reachable from dl6 |
|---|---|---|---|---|
| `module_import` | `src/engine/decls.rs:470` | `record=specifier` | `schema.rs:44` | yes, `sh specifier_at` |
| `module_binding` | `decls.rs:503` | `record=specifier` | `schema.rs:44` | yes |
| `module_edge` | `decls.rs:473` | `record=file_edge` | `schema.rs:50` | **no** |
| `module_unresolved` | `decls.rs:480` | `record=file_unresolved` | `schema.rs:51` | **no** |
| `module_binding_resolved` | `decls.rs:494` | `file_edge` joined to `specifier` | `schema.rs:50` | **no** |
| `crate_edge` | `decls.rs:487` | `record=package_edge` | `schema.rs:52` | **no** |

| fact | receipt |
|---|---|
| `--deps` / `--scip-deps` / `--package-deps` are CLI flags | `v6/sprefa-extract/src/bin/extract.rs:87-138` |
| all three are tested | `tests/7_diet_deps_cli.rs` (286 lines), `tests/28_package_edges.rs` (144 lines) |
| graded against madge | `schema.rs:215` recall 0.992 precision 0.988 (`--scip-deps`), `:219` recall 1.000 precision 1.000 (`--deps`) |
| the in-process executor refuses every one | `v6/sprefa-engine-rs/src/hosts.rs:1071-1074` |
| `package_edge` is a SUPERSET of `crate_edge` | `schema.rs:232-233`: v5's was Cargo-only and keyed on crate NAMES; v6 keys on manifest paths and covers Cargo.toml, package.json and go.mod |

## Fix shape

Two host names, both with a project-root input rather than a per-file one:

```
host_input_contract(deps,         [col(root, text), col(digest, text)], [identity, freshness]).
host_input_contract(package_deps, [col(root, text), col(digest, text)], [identity, freshness]).
```

`--deps` and `--scip-deps` fold to the SAME `file_edge` record (`schema.rs:210`),
so one host name with an evidence suffix mirrors the `scip.call` /
`scip.diet.call` pair that already exists: `deps` and `deps.scip`.

The executor arm is `deps::diet_file_edges_jsonl` / `package_edges_jsonl`, both
already imported by the binary (`extract.rs:21-22`).

## Also here: @extract-module-plane-non-ts

The specifier record is emitted for rust, go, kotlin, ts, dl6 and prolog
(`schema.rs:119-121`), but the RESOLVER arm behind `--deps` is TypeScript's
ladder. That issue is open and is the other half of this plane.

## Gate

```bash
cd v6/sprefa-extract && timeout 900 cargo test --release --features cli
# plus a dl6 fixture declaring `sh deps(...)` and reading one file_edge row
```
