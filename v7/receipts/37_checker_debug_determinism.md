# Checker debug determinism

## Change

The structured clause of `debug_checker_input/2` now commits immediately
after matching `basement_program(...)` with `!` at
[`1_checker.pl:101`](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl:101).
The fallback clause remains for other input shapes. This removes the second
successful helper path that previously caused `check_datalog/4` to enumerate
the checker body twice on backtracking.

The checker output, diagnostics, and exception and failure control flow remain
unchanged. The phase-boundary regression is at
[`3_compiler_trace.test.pl:333`](/Users/chrishafley/projects/sprefa/v7/test/3_compiler_trace.test.pl:333).
The exact public solution-count regression is at
[`1_entrypoints.test.pl:1870`](/Users/chrishafley/projects/sprefa/v7/test/1_entrypoints.test.pl:1870).

## Raw solution count

For the valid structured basement
`basement_program(root_graph([],[]),datalog_program([],[],[]))`, with empty
origins:

```text
DL7_TRACE=off:   one Checked-Diagnostics pair, Diagnostics = []
DL7_TRACE=debug: one Checked-Diagnostics pair, Diagnostics = []
```

The test compares the checked terms from both modes with `==` and verifies
that the caller's `DL7_TRACE` value is restored after each scoped run. Before
the cut, the same `findall/3` shape returned two pairs for this structured
input.

## Phase finalization

`check_phase_finalizes_immediately_without_once` runs
`run_compile_phase(check, check_datalog(...), Measurement)` inside an active
`collect` trace, then reads `collected_compile_phases/1` in the next goal. It
does not use `once/1`. The observed phase list is immediately:

```text
[phase(check, Measurement)]
```

and `Measurement` is ground. The focused trace tests also passed:

```text
debug_phase_events_preserve_failure_and_exception
debug_exception_propagates_and_clears_state
debug_trace_preserves_compile_output
debug_compile_emits_all_instrumented_boundaries
```

Each individual test stayed below the 3-second V7 limit.

## Focused results

The focused entrypoint subset passed `5/5` in one SWI process under 20
seconds, including the new solution-count test, evaluator trace tests, graph
parity, and nearest-shadow/partial demand-cone fixture parity. The focused
compiler-trace subset passed `5/5` in one SWI process under 20 seconds.

The performance comparator subset passed `4/4`, including nearest-shadow warm
output parity and the partial profile boundary comparator.

Direct consecutive compiler runs produced exact canonical parity for both
fixtures:

```text
7_nearest_shadow.dl7: equal, 810 rows, 0 diagnostics
2_partial.dl7:         equal, 910 rows, 0 diagnostics
```

## Trace counters and deltas

Receipt 34 recorded the uncommitted-helper baseline with check phase counters
of `81,890` and `1,201,235` inferences, and the temporary one-clause wrapper
at `17,740` and `145,992`. The final live nearest-shadow run recorded:

```text
first check:  17,737 inferences, 3 ms
second check: 145,989 inferences, 76 ms
outer trace:  1,993,610 inferences, 421 ms
```

Relative to the uncommitted-helper phase observations, the check counters are
`-64,153` and `-1,055,246`. Relative to the temporary committed wrapper,
they differ by `-3` each in this run. Relative to receipt 34's outer trace
total of `2,010,416`, the final outer trace is `-16,806` inferences. Receipt
34's separate 426.804 ms compile observation compares with the live 421 ms
profile observation as `-5.804 ms`; wall time is run-dependent.

The nearest-shadow live performance gate passed with 810 rows, empty
diagnostics, cold `1,994,058` inferences and `421 ms`, and warm `2,240`
inferences. The partial live gate exited `1` because its existing row
checkpoint expects `15,562`, while the current compile produced `910` rows;
it reported empty diagnostics, eight closure rounds, and no warm-output
mismatch. The direct consecutive-run parity above remained exact.

## Scope

Owned files changed:

- [`1_checker.pl`](/Users/chrishafley/projects/sprefa/v7/src/2_comptime/1_checker.pl)
- [`1_entrypoints.test.pl`](/Users/chrishafley/projects/sprefa/v7/test/1_entrypoints.test.pl)
- [`3_compiler_trace.test.pl`](/Users/chrishafley/projects/sprefa/v7/test/3_compiler_trace.test.pl)
- this receipt

No parser files were edited. No commit or push was made. No CI coverage was
added outside the focused tests.
