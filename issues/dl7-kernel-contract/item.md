---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: sol
status: needs-info
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-sol]
size: L
lane: dl7-contract
lane_seq: 0
collision: [v7-design]
commits:
- hash: 7e3303be5
  summary: v7 plan blocked on module identity
blocked_by: ['@dl7-contract-critique']
---

# DL7 kernel contracts: edge, bind, application, interning, evaluator

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

- [x] Public edge order is Owner, Name, Target, Index.
- [x] Relation output remains a tuple column.
- [x] Application lowering serves values and types.
- [x] One evaluator body runs compiler and runtime rules.
- [x] `intern/3` is the only domain-construction primitive.
- [x] Partial remains a `.dl7` proof goal.
- [x] Plan stays within four production modules and one test.
- [x] Report written to `v7/3_TASKS/results/0_KERNEL_CONTRACT.md`.

## Tests Run

Read-only checks. Run no suite.

## Stop condition

Hail the parent when a choice changes source syntax or semantic identity and
two coherent options remain after inspecting prior decisions.

## Agent Runs

### 2026-08-28T04:21:05Z · @codex-v7

2026-08-28: spawned Boop lane chore-dl7-kernel-contract at base a8bcda72c with preset sol/high. Expected report v7/3_TASKS/results/0_KERNEL_CONTRACT.md and at least one commit.

### 2026-08-28T04:38:51Z · @codex-v7

2026-08-28: reviewed Sol commit 297d90b9a, cherry-picked as 7e3303be5. Diff contains only the plan and blocked result report; git diff --check passed; production files 0; tests 0. All original acceptance criteria are evidenced. Status is needs-info because declared-node semantic identity remains unresolved between named(ModuleHash, Kind, Name) and named(module(ModulePath), Kind, Name).
