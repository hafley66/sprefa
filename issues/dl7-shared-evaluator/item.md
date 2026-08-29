---
created: 2026-08-28
updated: 2026-08-29
type: task
assignee: codex
status: in-progress
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-sol, model-codex]
size: L
lane: dl7-evaluator
lane_seq: 0
collision: [v7-evaluator, v7-libtime]
blocked_by: ['@dl7-datalog-checks']
---

# Execute checked positive Datalog through one shared SWI fixpoint

## Description

Implement the phase-independent evaluator immediately below the checked
Datalog IR. Compiler and runtime callers pass the same rule and seed shapes.
SWI tabling supplies recursive positive closure. The checked IR remains the
portable input for later SQL and Rust emitters.

## Signature

```prolog
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).

% Require ground checked IR.
% Allocate one evaluation identity.
% Install rules and seeds under that identity.
% Replace each reified var(Identity) with one fresh SWI variable per rule use.
% Close recursive positive calls through one tabled predicate.
% Return sorted, deduplicated call rows in the checked argument vocabulary.
% Abolish that evaluation's table and temporary facts on every exit path.
```

## Timeline

```text
ground Rules + Seeds
    -> install call-local rows
    -> query tabled closure
    -> copy and sort closure
    -> abolish tables and temporary rows
```

The evaluator has no compiler/runtime phase argument or branch. Compiler
effects, runtime effects, interning requests, negation, aggregates, ticks,
functional-key policy, and outer request loops stay outside this task.

## Storage and uniqueness

- `EvalId` identifies temporary facts and tabled calls for one invocation.
- Repeated `var(Id)` terms inside one rule invocation share one SWI variable.
- A later invocation of the same rule receives fresh SWI variables.
- Closure rows are set-valued and sorted by standard term order.
- Cleanup removes only rows and tabled subgoals carrying the current `EvalId`.

## Acceptance Criteria

- [x] `evaluate/4` runs compiler and runtime-shaped inputs without a phase branch.
- [x] Positive recursion reaches a finite set fixpoint through SWI tabling.
- [x] Reified variables share by identity within each rule invocation.
- [x] Closure rows retain `call(ref(Relation), Arguments)` compiler data.
- [x] Evaluation-local tables and facts are cleaned through `setup_call_cleanup/3`.
- [x] Production code lives in `v7/src/1_libtime/0_evaluator.pl`.
- [x] No DL6, Rust, TypeScript, effect, interning, or negation dependency is added.
- [x] No standalone test file is added.

## Tests Run

- [x] One direct SWI receipt proves two-hop recursive closure, deduplication,
      variable sharing, and a second clean invocation.

## Implementation Notes

The checked caller contract is produced by
`v7/src/2_comptime/0_compiler.pl`. `v7/src/1_libtime` contains algorithms
shared by comptime and runtime; it contains no phase policy.
