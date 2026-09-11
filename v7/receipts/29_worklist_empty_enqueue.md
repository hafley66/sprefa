# Stratification worklist empty-enqueue fast path

## Status

Implemented and validated in the shared checkout. `worklist_loop/5` now reuses
`Queue0` when `reader_levels/6` returns `Enqueued == []`; nonempty enqueues
still use the existing `append(Queue0, Enqueued, Queue)` FIFO operation.

No compiler kernel, graph, type, binding, evaluation, or output semantics
changed. No commit or push.

## Exact predicates and modes

The changed path is:

```prolog
relax_worklist(+Queue, +DependencyIndex, +Levels0, -Levels)
  -> worklist_loop(+Queue, +DependencyIndex, +LevelByRelation0,
                   -LevelByRelation, +Debug)
  -> reader_levels(+Readers, +BodyLevel, +LevelByRelation0,
                   -LevelByRelation, -Enqueued, +Debug)
```

When a relation has indexed readers whose required levels are already
satisfied, `reader_levels/6` returns `Enqueued = []`. The new branch sets
`Queue = Queue0` directly. When at least one reader level grows, the old
`append/3` call remains unchanged, preserving FIFO order. Repeated reader keys,
cycles, and debug counter updates remain on the same `reader_levels/6` and
recursive `worklist_loop/5` paths.

## Focused invariant coverage

Added tests:

- `stratification_worklist_reuses_queue_on_empty_enqueue`
- `stratification_worklist_keeps_nonempty_enqueue_order`
- `stratification_worklist_repeated_reader_keys_keep_fixpoint`

Existing tests cover positive chains, negative gaps, recursive SCCs, mixed
cycles, aggregate gaps, scoped indices, test-local reference parity, and the
nearest-shadow and `2_partial` rule sets. Debug-row behavior is covered by the
existing evaluator trace and compiler trace suites.

## Synthetic worklist scaling

The probe used `N` queued self-reader keys with gap zero, zero initial levels,
and measured `worklist_loop/5` after constructing its dependency and level
associations in a fresh SWI process:

| N | prior audit scan | empty-enqueue fast path |
| ---: | ---: | ---: |
| 100 | 6,810 | 1,603 |
| 200 | 23,302 | 3,203 |
| 400 | 86,602 | 6,403 |

The queue shape has no level changes, so every indexed reader set returns an
empty enqueue list. The prior values show the quadratic queue-tail copying;
the changed values are linear in the queued keys for this shape.

## Whole-compile inference deltas

These measurements compare the current checkout immediately before and after
this one-line queue branch, with separate fresh SWI processes:

| Fixture | before cold inferences | after cold inferences | delta | rows | diagnostics |
| --- | ---: | ---: | ---: | ---: | --- |
| `7_nearest_shadow.dl7` | 2,408,708 | 2,403,099 | -5,609 (-0.23%) | 810 | `[]` |
| `2_partial.dl7` | 6,002,405 | 5,993,738 | -8,667 (-0.14%) | 910 | `[]` |

The nearest-shadow post-change gate run reported `443 ms` cold wall, `2,240`
warm inferences, and `810` rows. The live `2_partial` gate still reaches 910
rows and exits on the existing `compiler_row_checkpoint(910,15562)`.

## Canonical output parity

`write_canonical(output(Rows, Runtime, Diagnostics))` SHA-256 remained stable:

| Fixture | rows | hash |
| --- | ---: | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

## Validation

| Command | Result |
| --- | --- |
| focused evaluator/stratification subset in `v7/test/1_entrypoints.test.pl` | 16/16 passed; nearest-shadow parity 0.496 s, partial parity 0.873 s |
| `v7/test/3_compiler_trace.test.pl` | 15/15 passed; slowest 0.246 s |
| `v7/test/20_compiler_performance.test.pl` | 17/17 passed |
| nearest-shadow live compiler gate | pass; 810 rows, empty diagnostics |
| `2_partial` live compiler gate | existing row checkpoint failure only |

All probes used one SWI process at a time with `timeout 20`.
