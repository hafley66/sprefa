---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: fable
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# extract: Go door on go/ssa + go/callgraph emitting cfg, flow and resolved edges

## Description

## Description

Compiler IR is the only origin of types-as-data and precise call graphs. Go is the cheapest door: `go/packages` + `go/types` + `golang.org/x/tools/go/ssa` + `go/callgraph` (CHA/RTA/VTA), one Go binary, and scip-go already sits on go/packages. Emit into the existing vocabulary: `cfg_*` from SSA basic blocks (defer/recover lowered), `flow_edge` from SSA def-use, `resolved_edge` from the chosen call-graph algorithm with a `precision` field. See research/2026-09-08-fact-sources-beyond-scip.md §2.9, §5 row 4.

## Acceptance Criteria

- [ ] `extract --family ssa main.go` (or the door's flag) emits cfg, flow, resolved edges on a fixture with an interface call
- [ ] `resolved_edge.precision` in {cha, rta, vta, static}
- [ ] cold time on a 20-file module recorded in the issue

## Tests Run

- [ ] fixture module

## Implementation Notes

- [ ] separate Go binary under tools/, spawned like the scip indexers
