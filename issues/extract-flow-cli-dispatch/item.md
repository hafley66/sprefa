---
created: 2026-08-16
updated: 2026-08-16
type: task
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# Wire flow family into the extract CLI dispatch

## Description

flow_edges (types.rs:700-768) and flatten_flow (wire.rs:256-268) are working, tested code with zero production callers: parse_mask accepts only cst/type/call/df (extract.rs:487-501) and resolve_project never dispatches the flow join. Add the flow name to parse_mask + dispatch flow_edges in resolve_project; then the cpg_taint_walk golden's two derived flow_* rels collapse into a direct wire read (follow-up named at tests/13_flow_join.rs:3-5 and PR #317). Also fix two drifts found in #317: help.rs:137 claims unknown --family names are ignored while extract.rs:494 errors; --family cfg --bench drops the cfg pass at extract.rs:338.
