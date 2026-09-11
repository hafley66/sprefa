# Head-variable reuse

## Change

Rule checking already computes sorted `HeadVariables` for mode checking. The
same sorted identities now flow into an internal safety worker, avoiding a
second `head_variables/2` traversal.

The existing qualified wrapper remains:

```prolog
head_safety_diagnostics(
    +Head, +Body, +Origins, +RuleIndex, -Diagnostics)
```

Its behavior is unchanged: it computes `head_variables/2`, resolves the rule
origin, collects body variables in authored goal order, and emits
`unsafe_head_var/1` diagnostics in sorted head-variable order.

The new internal path is:

```prolog
head_safety_diagnostics_with_variables(
    +SortedHeadVariables, +Body, +Origins, +RuleIndex, -Diagnostics)
  -> head_safety_from_variables(
         +SortedHeadVariables, +Body, +NodeId, -Diagnostics)
```

Both `resolved_rule_diagnostic/3` and `resolve_rules/8` call the worker with
their already computed `HeadVariables`. `head_safety_diagnostics/5` remains a
compatibility wrapper for qualified callers. No graph, type, binding, or
kernel contract changed.

## Exact invariants

- `argument_variables/2` and its `sub_term/2` traversal remain unchanged.
- `head_variables/2` still gathers every nested `var(Identity)` from every
  head argument and applies `sort/2`.
- `goal_variables/2` and body-variable collection remain unchanged, including
  duplicate body identities and authored goal order.
- `unsafe_vars/4` retains sorted-head diagnostic ordering and multiplicity.
- `rule_origin/3` is still resolved from the same `Origins` and `RuleIndex`.
- The wrapper and worker produce identical safe and unsafe diagnostics.

Added focused coverage compares the wrapper and worker for repeated head
identities and an unsafe variable.

## Measurements

The nearest-shadow profile contained 648 safety calls. Before the change,
those calls caused a second 648-call `head_variables/2` traversal, measured at
66,570 inclusive inferences. After the change, the profile contains:

```text
head_variables/2                         calls=648
head_safety_diagnostics_with_variables/5 calls=648
head_safety_from_variables/4             calls=648
unsafe_vars/4                            calls=648
```

The duplicate head-variable traversal is absent. Three fresh-process whole
compile probes measured:

| path | before inferences | after inferences | delta |
| --- | ---: | ---: | ---: |
| nearest-shadow run 1 | 1,994,061 | 1,926,843 | -67,218 |
| nearest-shadow run 2 | 1,994,061 | 1,926,843 | -67,218 |
| nearest-shadow run 3 | 1,994,061 | 1,926,843 | -67,218 |

Rows remained 810 with empty diagnostics in all probes. Wall samples were
before `1,039.40 / 849.27 / 1,010.98 ms` and after
`1,167.77 / 905.94 / 819.80 ms`; medians were `1,010.98 ms` and `905.94 ms`.

## Exact output and validation

Canonical compiler SHA-256 values remained unchanged:

| fixture | rows | diagnostics | SHA-256 |
| --- | ---: | --- | --- |
| `7_nearest_shadow.dl7` | 810 | `[]` | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `[]` | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

Current validation:

- focused wrapper/worker safety test: passed in 0.005 s
- focused checker, generated-rule, mode, and stratification tests: 5/5
- `v7/test/20_compiler_performance.test.pl`: 17/17
- nearest-shadow live gate: passed, 810 rows, empty diagnostics
- `2_partial` live gate: 910 rows, eight closure rounds, empty diagnostics,
  existing `compiler_row_checkpoint(910,15562)` exit 1

No CI files were changed.
