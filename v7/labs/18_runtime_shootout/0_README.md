# Native-logic runtime shootout

This lab measures transitive closure through one native logic route per runtime:

| Runtime | Route |
| --- | --- |
| SBCL 2.6.7 | Common Lisp host-native data structures |
| SWI-Prolog 10.0.2 | Tabled recursive reachability |
| Racket CS 9.3 | Installed `datalog` package evaluator |
| `dbsp-kernel` | Rust RAM evaluator over the emitted operator contract |
| `dbsp-generated` | The same Rust kernel over native constructors generated from DL7 |
| `dbsp-sqlite` | SQLite fixed point over DDL and SQL rules generated from the same DL7 program |
| `native-postgres-query` | Opt-in native PostgreSQL ordinary recursive SQL full query |
| `pglite-query` | Opt-in PGlite ordinary recursive SQL full query |

The PostgreSQL-family arms are enabled with `POSTGRES_SHOOTOUT=1`. Both compare
all ordered closure pairs with an independent JavaScript BFS oracle. pg_ivm is
reported as unsupported because it rejects recursive view definitions.

The algorithms are idiomatic to each runtime. The measurements compare these selected logic routes. They do not hold the low-level closure algorithm constant.

## Graphs

For a positive integer `N`:

- `chain` contains `i -> i+1` for `0 <= i < N-1`. Its distinct transitive closure contains `N*(N-1)/2` pairs.
- `ring` contains `i -> (i+1) mod N`. Its distinct transitive closure contains `N*N` pairs, including each node reaching itself through the cycle.

Every arm materializes all distinct reachability pairs, counts them, and validates the exact expected count.

## Timing contract

- Graph or fact setup is timed separately from closure evaluation and materialization.
- Runtime process startup is measured with an empty invocation.
- `/usr/bin/time -lp` records process elapsed time and peak resident set size.
- A full run performs one discarded warmup and five measured repetitions for every runtime and graph case.
- Every arm prints one compact JSON object containing runtime, version, case, `N`, edge count, closure count, setup milliseconds, and closure milliseconds.

## Commands

From `v7/`:

```text
just runtime-shootout-smoke
just runtime-shootout
just runtime-shootout 48
POSTGRES_SHOOTOUT=1 just runtime-shootout-smoke
```

The default `N=48` follows one-pass smoke sweeps on this machine: `N=32` took
1.08 seconds, `N=48` took 2.41 seconds, and `N=64` took 6.36 seconds. The full
one-warmup/five-repeat gate uses the selected default and enforces the 60-second
bound.

The full runner replaces `5_RESULTS.md` with measurements from the current machine. It exits nonzero if any count differs, a required field is absent, or total shootout time reaches 60 seconds.
