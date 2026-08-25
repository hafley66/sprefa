---
created: 2026-08-24
updated: 2026-08-24
type: epic
owner: chris
status: deferred
priority: low
related: ['@userland-type-graph']
labels:
- area:dl6
- area:compiler
- intent:research
- size:large
- model:large
- task-graph
- future
size: L
---

# DL6 self-hosted backends and dynamically loaded monomorphized modules

## Goal

Express backend transformation rules in DL6 over queryable compiler facts.
Emit concrete SQLite and specialized Rust artifacts for one program, then load
the compiled module into a long-running server through a versioned protocol.

Detailed plan: `plans/2026-08-24-dl6-self-hosted-backends.md`.

## Current and Target Shapes

```text
current
plan/9 -> lowered/8 -> emit_rust.pl -> ProgramJson -> generic Rust dispatch

target
$type + $plan + $lower -> backend rules written in DL6
                       -> schema.sql + specialized module.rs
                       -> dynamic module ABI
```

The current ProgramJson and TSV2 paths remain parity oracles during research.
The target specializes relation row types, prepared SQL, rule functions,
schedules, storage representations, and boundary codecs.

## Research Task Graph

```text
compiler IR relation inventory
  -> normalized plan and lower source relations
       -> ordered artifact relation and collector
            +-> DL6 ProgramJson parity emitter
            |    -> TSV2 neutral-plan consumer lab
            +-> monomorphized SQLite and Rust backend lab
                 +-> native C ABI module lab
                 +-> WASM component lab
                 +-> RPC and shared-memory lab

module labs -> measured boundary matrix -> load/reload/cache contract
```

`@compiler-plane-expression-parity` is a reused foundation. This epic adds no
blocker to `@userland-type-graph`.

## Acceptance Criteria

- [ ] `plan/9`, `lowered/8`, and boot statements have normalized compiler-row contracts.
- [ ] A DL6 emitter produces deterministic artifacts through ordered output rows.
- [ ] DL6 emission matches the current ProgramJson backend over a pinned corpus.
- [ ] A generated Rust fixture uses concrete row types and static rule functions.
- [ ] Native, WASM, and RPC module boundaries are measured against one workload.
- [ ] One server loads two compiled program versions without recompiling itself.
- [ ] SQLite ownership, ABI versioning, schema compatibility, drain, and rollback are specified.

## Tests Run

Deferred research. Required receipts are defined in the linked plan.

## Implementation Notes

No implementation authorization. Later decomposition assigns model size per
lab after the compiler-row inventory fixes signatures and blast radius.
