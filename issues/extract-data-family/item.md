---
created: 2026-08-17
updated: 2026-08-17
type: task
status: open
priority: normal
epic: openapi-clap-uds-lab
related: ['@engine-rs-serve-uds']
---

# sprefa-extract: data family (json/jsonl/yaml/toml), v5 datapath plane ported, hosted both doors

## Description

## Ask (user 2026-08-17)
Port v5's data plane (`src/datapath.rs`: json, jsonl, yaml, toml as ONE plane,
dispatch by extension, tree-sitter-json / tree-sitter-yaml / tree-sitter-toml-ng,
every hit carries a byte span) into sprefa-extract as family `data`, hosted on
both doors through the existing extract executor. dl6's `decode/2` brace
pattern is v5's `json(p, rev, q:{...})` over its rows.

## Steps
1. `lang/data/` source in sprefa-extract: `matches` on `.json .jsonl .yaml .yml .toml`; one `doc` record per document (multi-doc yaml = many), plus span rows per leaf.
2. `--family data` in `parse_mask` (`src/bin/extract.rs:484`).
3. Golden: pokeapi.openapi.yml -> rows; byte-diff on both doors (scip_combo shape).

## Rails
Comment budget, no eprintln, surrogate keys, 10-second law. Cargo deps: the three tree-sitter grammars v5 already pins (root Cargo.toml:121-130 for versions).
