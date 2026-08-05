# REPORT-XHARNESS

## TOC
- What and where
- Deliverable inventory
- The workload and three surfaces
- Harness behavior
- Gates (verbatim)
- Self-test standings
- Tuned parameters per case
- Deviations
- Style and rail compliance

## What and where

The exec_shootout harness: a standalone cargo crate that generates the three
graph families (chain / layered / grid) deterministically from a seed, tunes
each (family, scale) case into the 1M..20M derived band, runs the engine
binaries it is given, checks every engine agrees on (derived, checksum),
times best-of-3, and writes STANDINGS.md.

Location: `v6/labs/exec_shootout/harness/` (the lane-owned crate). Output:
`v6/labs/exec_shootout/STANDINGS.md` (lane-owned file at the shootout root).

Next action for the coordinator: build the three engine crates and pass their
release binaries via `harness --engines <bin>[,<bin>...]`.

## Deliverable inventory

| file | role |
|---|---|
| `src/gen.rs` | seeded PRNG (splitmix64), chain/layered/grid generators, `Params` |
| `src/refengine.rs` | semi-naive reference closure, fnv1a64 checksum, getrusage RSS, bitset DAG counter |
| `src/bin/ref_engine.rs` | reference engine binary, implements the engine CLI + 3 JSONL contract |
| `src/tuner.rs` | per-family tuning into the derived band |
| `src/runner.rs` | spawn engine, 120s timeout, parse 3 JSONL events |
| `src/standings.rs` | STANDINGS.md writer |
| `src/main.rs` | CLI, best-of-3, cross-check, build measurement |
| `Cargo.toml` | one crate, empty `[workspace]` (isolated from the root workspace), deps `fxhash` + `libc` only |
| `STANDINGS.md` | committed self-test (reference only at 10k) |

CLI: `harness --engines <bin>[,<bin>...]|[ref] [--scales a,b,c] [--work DIR] [--standings PATH] [--measure-builds]`.

## The workload and three surfaces

Semi-naive transitive closure:

```
reachable(x, y) <- edge(x, y).
reachable(x, z) <- reachable(x, y), edge(y, z).
```

The harness ships only the reference engine. The three racing engines (interp,
rxgraph, mono) are sibling lanes, wired later by the coordinator; each gets the
same input file and emits the contract JSONL events.

## Harness behavior

- Generator: seeded, deterministic, committed as code; no input files committed.
- Tuner lands every (family, scale) in the 1M..20M band and records the chosen
  params. Chain and grid are tuned analytically; layered is tuned by exact
  bitset reachability count with the layer count binary-searched per fanout.
- Timing: best of 3 runs, best kept by smallest fixpoint ms. THE number =
  derived rows/sec in the fixpoint phase.
- Correctness: at 10k the internal reference anchors truth and every engine
  must match its (derived, checksum). At 100k and 1M every engine must match
  every other. Any mismatch exits nonzero before any standings are written.
- The reference speed is never reported: reference rows in STANDINGS show
  derived and the derived count is labeled "(reference)", timing columns are
  "-", and the THE number ignores reference rows.
- Per-engine release binary size and cold build seconds: recorded when
  `--measure-builds` is set and the engine crate can be located.

## Gates (verbatim)

`cargo build --release` in the harness dir:

```
   Compiling exec-shootout-harness v0.1.0 (...)
   Finished `release` profile [optimized] target(s) in 1.21s
```
(no warnings; checked with a full rebuild)

`cargo test --release` in the harness dir:

```
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
(5 in `src/refengine.rs`, 3 in `src/runner.rs`, total 8;
0 failures)

Required unit tests present: semi-naive stops on empty delta; checksum matches
a hand-computed value on a 3-edge graph; loaded/fixpoint/done events parse as
JSON.

Hand-run against a tiny inline input (10 edges, 11-node path):

```
$ printf 'p 11 10\n0 1\n1 2\n2 3\n3 4\n4 5\n5 6\n6 7\n7 8\n8 9\n9 10\n' > work/tiny10.in
$ ./target/release/ref_engine --input work/tiny10.in
{"event":"loaded","edges":10,"ms":0}
{"event":"fixpoint","derived":55,"ms":0}
{"event":"done","checksum":"298615f12c00bb95","peak_rss_kb":1696}
```

Expected derived by hand: for a single path of n=11 nodes the reachable set is
every x<y pair, so derived = n(n-1)/2 = 55. Reported 55, matches. The checksum
was independently recomputed as XOR over fnv1a64(pair) and matched.

## Self-test standings

The committed STANDINGS.md is the self-test: only the reference engine at the
10k scale. Reference speed is not reported, so the THE number row is 0 by
design. The run proves the generator, tuner, runner, JSON parse, cross-check,
and standings writer all execute standalone and in band:

| family | derived (10k) | in band |
|---|---|---|
| chain | 9,996,213 | yes |
| grid | 9,979,359 | yes |
| layered | 9,951,396 | yes |

## Tuned parameters per case

| family | scale | tuned params | edges | derived | in band |
|---|---|---|---|---|---|
| chain | 10000 | segment_len=2582 | 7743 | 9,996,213 | yes |
| chain | 100000 | segment_len=200 | 99898 | 9,989,800 | yes |
| chain | 1000000 | segment_len=20 | 999989 | 9,989,890 | yes |
| layered | 10000 | layers=193 width=26 fanout=2 | 9984 | 9,951,396 | yes |
| layered | 100000 | layers=6 width=1250 fanout=16 | 100000 | 10,403,068 | yes |
| layered | 1000000 | layers=4 width=15000 fanout=8 | 360000 | 9,815,343 | yes |
| grid | 10000 | rows=79 cols=79 | 12324 | 9,979,359 | yes |
| grid | 100000 | rows=79 cols=79 | 12324 | 9,979,359 | yes |
| grid | 1000000 | rows=79 cols=79 | 12324 | 9,979,359 | yes |

Every case lands inside the 1M..20M band. Chain and grid are tuned
analytically and match the reference run exactly; layered is tuned on an exact
bitset count and agrees with the semi-naive reference (cross-checked in a unit
test on a DAG).

## Deviations

1. Grid actual edges sit far below the nominal scale ladder. A full 2D lattice
   closure grows O(n^4), so a lattice that filled the 1M-edge ladder would
   blow the 20M upper band. The tuner lands a square lattice in band; the same
   geometry is produced at all three scales. Actual edges are recorded.
2. Chain actual edges at 10k sit below the ladder (7743 of 10000). A single
   path long enough to hold 10000 edges has O(n^2) closure that over-runs the
   band, so the tuner splits it into segments, trimming edges.
3. Layered actual edges at 1M sit below the ladder (360000). The width is
   capped to keep total nodes at 60000 so tuning stays fast and exact.

In all three cases the hard contract bound, derived in [1M, 20M], is satisfied;
the edge ladder is treated as nominal and the actual edge count is recorded and
reported per case. Numeral: the harness reports whatever edges the engine
binary reports in its `loaded` event. No STOP condition triggered.

## Style and rail compliance

- Deps on the allowlist only: `fxhash` and `libc`.
- No comments in `src/` (comment budget satisfied). No em dashes, no banned
  words (provenance, substrate, load-bearing, regime) in prose or identifiers.
- Identifiers descriptive, no single-letter loop variables.
- `work/` and `target/` are gitignored; generated inputs are not committed.
