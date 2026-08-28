---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: sol
status: open
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-sol]
size: L
lane: dl7-contract
lane_seq: 0
collision: [v7-design]
---

# DL7 kernel contracts: edge, bind, application, interning, evaluator

## Description

## Description

Read `v7/2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md`, Boop favorites 26 through
37, and the twelve V7 donor reports. Pin the smallest executable contracts for
prefix terms, `:/4`, callable output columns, interning, fixpoint requests, and
one evaluator used by compiler and runtime rule sets.

## Signatures

```prolog
read_dl7(+Path, +Text, -Forms, -SourceMap, -Diagnostics).
lower_dl7(+ModulePath, +Forms, -Rules, -Seeds, -Requests, -Diagnostics).
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).
compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics).
```

Add pseudocode comments for each signature directly to the plan.

## Timeline and storage

Trace one compiler call and one runtime call through the same evaluator. Pin
every row key, lifetime, cleanup boundary, and canonical construction identity.

## Acceptance Criteria

- [ ] Public edge order is Owner, Name, Target, Index.
- [ ] Relation output remains a tuple column.
- [ ] Application lowering serves values and types.
- [ ] One evaluator body runs compiler and runtime rules.
- [ ] `intern/3` is the only domain-construction primitive.
- [ ] Partial remains a `.dl7` proof goal.
- [ ] Plan stays within four production modules and one test.
- [ ] Report written to `v7/3_TASKS/results/0_KERNEL_CONTRACT.md`.

## Tests Run

Read-only checks. Run no suite.

## Stop condition

Hail the parent when a choice changes source syntax or semantic identity and
two coherent options remain after inspecting prior decisions.
