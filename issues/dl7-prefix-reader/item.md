---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: glm53f
status: open
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-glm53f]
size: M
lane: dl7-reader
lane_seq: 0
collision: [v7-reader]
blocked_by: ['@dl7-contract-critique', '@dl7-kernel-contract']
---

# Implement bounded DL7 prefix reader

## Description

## Description

Implement the bounded prefix reader contract after the Sol and Opus reports
land. Reuse audited literal, escape, comment, span, and diagnostic predicates
where their inputs are independent of DL6 declarations.

## Signature

```prolog
read_dl7(+Path, +Text, -Forms, -SourceMap, -Diagnostics).
```

## Timeline and storage

One call owns one variable table and source map. Repeated `?x` inside one rule
shares identity. Cleanup completes on success and failure.

## Acceptance Criteria

- [ ] Parses only the kernel forms pinned in the contract.
- [ ] Returns canonical prefix trees and spans.
- [ ] Imports no DL6 statement dispatch or declaration carrier.
- [ ] Production changes stay under `v7/0_READER/`.
- [ ] Adds no standalone test file.

## Test Run

Run the eventual single oracle command only when it exists. Otherwise use one
direct SWI read receipt and record it in the commit body.

## Stop condition

Hail the parent for any missing lexical ruling. Do not invent syntax.
