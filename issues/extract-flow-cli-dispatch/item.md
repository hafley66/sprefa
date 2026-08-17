---
created: 2026-08-16
updated: 2026-08-16
type: task
reporter: chris
status: done
priority: normal
epic: soopy-full-wiring
closed: 2026-08-16
---

# Wire flow family into the extract CLI dispatch

## Description

flow_edges (types.rs:700-768) and flatten_flow (wire.rs:256-268) are working, tested code with zero production callers: parse_mask accepts only cst/type/call/df (extract.rs:487-501) and resolve_project never dispatches the flow join. Add the flow name to parse_mask + dispatch flow_edges in resolve_project; then the cpg_taint_walk golden's two derived flow_* rels collapse into a direct wire read (follow-up named at tests/13_flow_join.rs:3-5 and PR #317). Also fix two drifts found in #317: help.rs:137 claims unknown --family names are ignored while extract.rs:494 errors; --family cfg --bench drops the cfg pass at extract.rs:338.

## Comments

### 2026-08-17T03:35:38Z · @soopy-driver

PR #330 posted, green. flow lands as a RESOLVE arm not a FamilyMask bit (receipt: types.rs:722-724 declares FlowF phase-2 with no mask bit; flow_edges consumes resolved call edges that only exist under --resolve), so parse_arms + ResolveArms is the door and parse_mask is untouched. Both #317 drifts fixed. call and flow share ONE resolve pass per input. Gate: cargo test --features cli 136/0 twice. NOTE FOR EVERY EXTRACT CARD: the gate is 'cargo test --features cli', never bare 'cargo test' -- Cargo.toml:117-120 puts the extract bin behind required-features, so bare cargo test hands 1_resolve_cli a nonexistent CARGO_BIN_EXE_extract and reports 1 passed 8 failed on a clean tree. Follow-up left open: the cpg_taint_walk golden's two derived flow_* dl6 rels collapsing to a direct wire read (v6/tsv2, coordinator-owned).
