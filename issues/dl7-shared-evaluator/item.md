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
lane: dl7-evaluator
lane_seq: 0
collision: [v7-evaluator]
blocked_by: ['@dl7-contract-critique']
---

# Extract one shared compiler and runtime fixpoint evaluator

## Description

## Description

Extract or adapt the smallest SWI-Prolog fixpoint evaluator allowed by the
contract. The same exported predicate must evaluate compiler and runtime rule
sets. Include deterministic interning requests and lower-stratum negation.

## Signature

```prolog
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).
```

## Timeline and storage

One call allocates one evaluation identity, installs copied rules and seeds,
computes closure, copies results, and clears tables plus temporary facts.

## Acceptance Criteria

- [ ] No phase branch exists inside `evaluate/4`.
- [ ] No dependency on DL6 parser, `prog/2`, `col_type/3`, `plan/9`, or
      expansion driver.
- [ ] Positive recursion and one lower-stratum anti-join work.
- [ ] Functional-key conflicts produce deterministic diagnostics.
- [ ] Reused semantic comments remain attached to predicates.
- [ ] Production changes stay under `v7/2_EVALUATOR/`.
- [ ] Adds no standalone test file.

## Test Run

Use one direct SWI receipt until the single oracle lands. Run no V6 suite.

## Stop condition

Hail the parent if the donor evaluator cannot be detached within this module
boundary.
