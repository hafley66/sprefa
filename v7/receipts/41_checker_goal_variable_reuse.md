# Checker goal-variable reuse

## Scope

The nearest-shadow profile reported 4,629 calls to
`dl7_checker:goal_variables/2` and 69,265 inclusive inferences. The calls
were attributed with a temporary `library(prolog_wrap)` probe in one fresh
SWI process:

| scope | calls | unique goal shapes |
| --- | ---: | ---: |
| `check_goal/4` | 1,542 | 314 |
| post-check transition accumulation | 1,511 | 308 |
| `head_safety_from_variables/4` body collection | 1,576 | 321 |
| total | 4,629 | 321 |

The check and transition scopes overlapped on 301 exact goal shapes. Their
per-shape minimum was 1,477 repeated calls. The repeated path was the
successful positive transition: `check_goal/4` collected variables for bound
state, then `check_goal_transition/6` collected the same variables again for
the produced-variable state.

## Change

`check_goal_transition/6` now collects positive-goal variables once and calls
`check_goal_with_variables/5`. The worker preserves the existing positive
kernel branches and receives the already collected list. The existing
`check_goal/4` predicate remains available as the qualified wrapper and uses
the same worker. Negative-goal handling remains on the original path.

Changed predicates:

- `check_goal_transition/6`
- `check_goal/4`
- new `check_goal_with_variables/5`

The positive transition now performs one `goal_variables/2` call instead of
the two calls previously performed by the successful positive path. Bound
state, produced state, mode failures, diagnostics, and ordering are unchanged.

## Measurements

The post-change fresh profile reported:

```text
goal_variables/2 total=3152
  check_goal/4                         65
  positive transition precollection    1511
  head safety body collection          1576
```

The 1,477 check/transition overlap calls were removed. The clean HEAD baseline
(`e62f4309f`) was measured in a fresh process at 1,926,842 nearest-shadow
compile inferences. Three candidate fresh processes each measured 1,899,633
inferences, a delta of `-27,209`. Candidate wall samples were 419.80 ms,
415.57 ms, and 416.36 ms. The baseline wall sample was 417.84 ms.

## Exact parity and gates

Canonical compiler hashes were equal between clean HEAD and the candidate:

| fixture | rows | diagnostics | SHA-256 |
| --- | ---: | --- | --- |
| `7_nearest_shadow.dl7` | 810 | `[]` | `f42d4b6a73ce4cdd97594ff9710377c499a8c78e2ff8987517793cbbca29a99a` |
| `2_partial.dl7` | 910 | `[]` | `d47c0778343e976d0fc648324cbb4003c643543ee00b61e817c407ae4ce69519` |

Validation:

- focused checker and parity tests: 6/6, each under 3 seconds
- `v7/test/20_compiler_performance.test.pl`: 17/17
- nearest-shadow live gate: passed, 810 rows
- `2_partial` live gate: 910 rows, eight closure rounds, empty diagnostics;
  existing `compiler_row_checkpoint(910,15562)` caused exit 1

No parser, evaluator, kernel, type, binding, or phase files were changed.
