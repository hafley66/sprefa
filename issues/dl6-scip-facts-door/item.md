---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

v5's ten `scip_*` relations all exist as v6 records and are all tested. None of
them is reachable from a `.dl6` program.

## Receipts

| fact | receipt |
|---|---|
| v5's ten | `src/rels/scip.rs:61-88` |
| v6's eight, behind `--family scip` | `v6/sprefa-extract/src/schema.rs:66-73`, `:239-252` |
| the other two ride `--scip-facts` | `schema.rs:56` (`scip_occurrence`, a superset of v5's columns) and `schema.rs:131-133` (`--occurrence-text` answers `scip_binding`'s source slice) |
| both modes are tested | `tests/8_scip_families_cli.rs` (702 lines), `tests/5_scip_facts_cli.rs` (407 lines), `tests/6_occurrence_text_cli.rs` |
| `--family scip` is refused in-process | `v6/sprefa-engine-rs/src/hosts.rs:1111-1116` "mode `scip` is not linked in-process" |
| `--scip-facts` is refused in-process | `hosts.rs:1071-1074`, the generic unknown-flag stop |
| what a dl6 program CAN reach | four resolved namespaces at `v6/prolog/compile/registry.pl:499-502`, answering `resolved_edge` and `resolved_type_edge` only |
| the in-process scip executor | `hosts.rs:699-844` `ScipNamespaceExecutor`, index-or-diet, one fold per (root, set digest, evidence) |

## Why this is not "just add a flag"

`ScipNamespaceExecutor` already ensures an index and runs a resolve. The eight
v5-vocab rows are a different PROJECTION of the same index: `scip_def`,
`scip_ref` and friends are per-symbol rows, not per-file resolved edges. The
executor's `prime`/`fold` shape is per-file, and these rows are per-index.

Also carried here, because it is the same decision: v5's `call_edge` (30 rails)
and `type_edge` (16 rails) want the `--resolve` arm's output. The four resolved
namespaces answer them today but key on `caller_path`/`owner_path` rather than
v5's symbol strings, which changes what a joining program can say.

## Fix shape

A `scip.facts.<record>` host namespace beside the four resolved ones, keyed on
`(repo, digest)` rather than `(repo, path, digest)`, since the answer is
per-index. One `host_output_contract` clause per record shape.

## Gate

```bash
cd v6/sprefa-extract && timeout 900 cargo test --release --features cli
cd v6 && timeout 600 just scip-families
# plus a dl6 fixture declaring `sh scip.facts.def(...)` and reading one row
```
