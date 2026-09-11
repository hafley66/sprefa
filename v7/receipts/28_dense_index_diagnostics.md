# Dense pending-edge index diagnostics

## Status

Implemented and validated in the shared checkout. The checker now builds one
owner-to-count association for ground edge inputs, then keeps the original
per-edge diagnostic traversal. Nonground `Edges` or `All` inputs use the prior
`count_owner_edges/3` scan path, preserving its unification behavior.

No compiler kernel, graph, type, binding, or evaluation semantics changed. No
commit or push.

## Call path and signatures

The only production caller is `bind_diagnostics/3` in
`v7/src/2_comptime/1_checker.pl`:

```prolog
bind_diagnostics(+PendingEdges, +Origins, -Diagnostics)
  -> dense_index_diagnostics(+Edges, +AllEdges, +Origins, -Diagnostics)
```

The indexed path is:

```prolog
owner_edge_count_index(+AllEdges, -CountIndex)
  -> dense_index_diagnostics_indexed(+Edges, +CountIndex, +Origins, -Diagnostics)
```

The fallback path retains:

```prolog
dense_index_diagnostics_scanned(+Edges, +AllEdges, +Origins, -Diagnostics)
  -> count_owner_edges(+AllEdges, +Owner, -Count)
```

For ground inputs, owner counts are grouped once with `keysort/2` and
`group_pairs_by_key/2`, then stored in an association. Each input edge still
produces exactly one check in authored list order. Duplicate edge rows remain
duplicate diagnostics. Multiple owners have independent counts. Empty input
returns `[]`. `edge_origin/5` remains the origin lookup used for each emitted
diagnostic.

## Exact behavior coverage

The focused tests cover:

- multiple owners with independent dense counts;
- sparse indices producing `non_dense_index(Owner, Index)`;
- duplicate invalid rows preserving diagnostic multiplicity and order;
- empty inputs;
- nonground owner inputs staying on the scan path.

The indexed and scanned implementations produce identical diagnostics for the
ground duplicate/multiple-owner fixture.

## Synthetic scaling

The local probe used ground rows of the form
`pending_edge(owner, Index, target(Index), Index)` with empty origins and
indices `0..N-1`. It measured statistics deltas in fresh SWI processes:

| N | prior scan | indexed pass |
| ---: | ---: | ---: |
| 100 | 20,903 | 1,589 |
| 200 | 81,803 | 2,989 |
| 400 | 323,603 | 5,789 |

The coordinator audit's dense synthetic baseline was `11,003 / 42,002 /
164,002` inferences for `N=100 / 200 / 400`; the local probe retains the same
quadratic-to-linear shape while including the direct private predicate call and
empty-origin lookup cost.

## Compiler gate measurements

Nearest-shadow after the preceding closure-collection optimization was
`2,927,019` cold inferences and `504 ms` median cold wall over five runs. After
this checker change it is `2,408,708` cold inferences. Five post-change cold
wall values were `547, 597, 496, 499, 543 ms`, median `543 ms`. The wall result
is load-sensitive; rows remain `810` and diagnostics remain `[]`.

The traced `2_partial` profile changed from `6,557,302` to `6,002,405` cold
inferences in the measured runs. Its live output remains `910` rows with eight
closure rounds and the existing CLI row-checkpoint mismatch remains
`compiler_row_checkpoint(910,15562)`.

## Canonical output parity

`write_canonical(output(Rows, Runtime, Diagnostics))` SHA-256 hashes remained:

| Fixture | rows | hash |
| --- | ---: | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

## Validation

| Command | Result |
| --- | --- |
| focused checker/compiler subset in `v7/test/1_entrypoints.test.pl` | 17/17 passed; slowest 1.119 s |
| `v7/test/3_compiler_trace.test.pl` | 15/15 passed; slowest 0.403 s |
| `v7/test/18_binding_symmetry.test.pl` | 16/16 passed; one run's slowest 3.060 s under shared setup/load |
| `v7/test/19_lexical_binding.test.pl` | 10/10 passed |
| `v7/test/20_compiler_performance.test.pl` | 17/17 passed |
| `v7/test/21_compiler_profile.test.pl` | 25/25 passed |
| nearest-shadow live compiler gate | pass; 810 rows, empty diagnostics |
| `2_partial` live compiler gate | reaches 910 rows and exits 1 on the existing 15,562-row checkpoint |

All probes used one SWI process at a time and `timeout 20`.
